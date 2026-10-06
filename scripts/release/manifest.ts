import {
  createHash,
  createPrivateKey,
  createPublicKey,
  randomBytes,
  sign,
  verify,
} from "node:crypto";
import {
  readFileSync,
  writeFileSync,
  readdirSync,
  statSync,
  realpathSync,
  existsSync,
} from "node:fs";
import { dirname, resolve, basename } from "node:path";
import { tmpdir } from "node:os";

const repo = realpathSync(resolve(import.meta.dir, "../.."));
const b64 = (b: Buffer) => b.toString("base64url");
function outside(path: string) {
  const resolved = resolve(realpathSync(dirname(resolve(path))), basename(path));
  const p = existsSync(resolved) ? realpathSync(resolved) : resolved;
  if (p === repo || p.startsWith(`${repo}/`))
    throw Error("signing keys must be outside the checkout");
  return p;
}
function privateKey(seed: Buffer) {
  if (seed.length !== 32) throw Error("expected a 32-byte base64url update signing seed");
  return createPrivateKey({
    key: Buffer.concat([Buffer.from("302e020100300506032b657004220420", "hex"), seed]),
    format: "der",
    type: "pkcs8",
  });
}
function publicKey(key: ReturnType<typeof privateKey>) {
  return b64(createPublicKey(key).export({ format: "der", type: "spki" }).subarray(-32));
}
const hash = (p: string) => createHash("sha256").update(readFileSync(p)).digest("hex");
const [cmd, ...args] = process.argv.slice(2);
if (cmd === "keygen") {
  if (args.length !== 1) throw Error("usage: bun run update:keygen PATH_OUTSIDE_REPO");
  const path = outside(args[0]);
  const seed = randomBytes(32);
  writeFileSync(path, `${b64(seed)}\n`, { flag: "wx", mode: 0o600 });
  console.log(`update public key: ${publicKey(privateKey(seed))}`);
} else if (cmd === "public-key") {
  if (args.length !== 1) throw Error("usage: manifest.ts public-key EXTERNAL_SEED");
  const seed = readFileSync(outside(args[0]), "utf8").trim();
  console.log(publicKey(privateKey(Buffer.from(seed, "base64url"))));
} else if (cmd === "verify" || cmd === "unpack-history") {
  if (args.length !== 2)
    throw Error("usage: manifest.ts verify MANIFEST ENVELOPE | unpack-history ENVELOPE OUTPUT");
  const trusted = process.env.CONVT_UPDATE_PUBKEY;
  if (!trusted || !/^[A-Za-z0-9_-]{43}$/.test(trusted))
    throw Error("CONVT_UPDATE_PUBKEY trust root required");
  const e = JSON.parse(readFileSync(args[cmd === "verify" ? 1 : 0], "utf8"));
  const key = createPublicKey({
    key: Buffer.concat([
      Buffer.from("302a300506032b6570032100", "hex"),
      Buffer.from(trusted, "base64url"),
    ]),
    format: "der",
    type: "spki",
  });
  if (
    !verify(
      null,
      Buffer.from(`convt-update-v1\n${e.payload}`),
      key,
      Buffer.from(e.signature, "base64url"),
    )
  )
    throw Error("bad update signature");
  const decoded = Buffer.from(e.payload, "base64url");
  if (cmd === "unpack-history") {
    const manifest = JSON.parse(decoded.toString("utf8"));
    if (
      manifest.schema_version !== 1 ||
      !manifest.distribution_ready ||
      !Array.isArray(manifest.builds)
    )
      throw Error("history must be a distributable signed v1 manifest");
    writeFileSync(args[1], decoded, { flag: "wx" });
    console.log("PASS authenticated release history");
  } else {
    if (!decoded.equals(readFileSync(args[0])))
      throw Error("website manifest differs from signed update payload");
    console.log("PASS update signature and exact website payload");
  }
} else if (cmd === "sign") {
  if (args.length !== 3) throw Error("usage: manifest.ts sign MANIFEST KEY OUTPUT");
  const [input, keyPath, output] = args;
  const path = outside(keyPath);
  if ((statSync(path).mode & 0o077) !== 0) throw Error("update seed must have mode 0600");
  const text = readFileSync(path, "utf8").trim();
  if (!/^[A-Za-z0-9_-]{43}$/.test(text)) throw Error("malformed update seed");
  const key = privateKey(Buffer.from(text, "base64url"));
  if (publicKey(key) === process.env.CONVT_LICENSE_PUBKEY?.trim())
    throw Error("update and license keys must be separate");
  if (process.env.CONVT_UPDATE_PUBKEY && publicKey(key) !== process.env.CONVT_UPDATE_PUBKEY.trim())
    throw Error("seed does not match the embedded update public key");
  const payload = b64(readFileSync(input));
  writeFileSync(
    output,
    JSON.stringify({
      payload,
      signature: b64(sign(null, Buffer.from(`convt-update-v1\n${payload}`), key)),
    }) + "\n",
    { flag: "wx" },
  );
} else if (cmd === "generate") {
  if (args.length < 4 || args.length > 5)
    throw Error("usage: manifest.ts generate DIR VERSION DATE BASE_URL [HISTORY_JSON]");
  const [dir, version, date, base, history] = args;
  if (!/^\d+\.\d+\.\d+$/.test(version)) throw Error("version must be MAJOR.MINOR.PATCH");
  const buildTime = Date.parse(`${date}T00:00:00Z`);
  if (!Number.isFinite(buildTime) || new Date(buildTime).toISOString().slice(0, 10) !== date)
    throw Error("invalid build date");
  const url = new URL(base);
  if (url.protocol !== "https:" || url.username || url.password || url.search || url.hash)
    throw Error("BASE_URL must be a clean HTTPS URL");
  const issued = Number(process.env.SOURCE_DATE_EPOCH);
  const sequence = Number(process.env.CONVT_MANIFEST_SEQUENCE ?? issued);
  if (
    !Number.isSafeInteger(issued) ||
    issued < buildTime / 1000 ||
    !Number.isSafeInteger(sequence) ||
    sequence <= 0
  )
    throw Error("set SOURCE_DATE_EPOCH and a positive CONVT_MANIFEST_SEQUENCE");
  const expires = Number(process.env.CONVT_MANIFEST_EXPIRES ?? issued + 90 * 86400);
  if (!Number.isSafeInteger(expires) || expires <= issued) throw Error("invalid manifest expiry");
  const names = readdirSync(dir).sort();
  function artifact(name: string, platform: string, kind: string) {
    return {
      platform,
      kind,
      url: `${base.replace(/\/$/, "")}/${encodeURIComponent(version)}/${encodeURIComponent(name)}`,
      size: statSync(resolve(dir, name)).size,
      sha256: hash(resolve(dir, name)),
    };
  }
  const artifacts = names.flatMap((name) => {
    if (name.endsWith("-source.tar.gz")) return [];
    let platform: string, kind: string;
    if (name.endsWith(".deb")) {
      platform = "linux-x86_64";
      kind = "deb";
    } else if (name.endsWith(".rpm")) {
      platform = "linux-x86_64";
      kind = "rpm";
    } else if (name.endsWith(".AppImage")) {
      platform = "linux-x86_64";
      kind = "AppImage";
    } else if (name.includes("linux-x86_64") && name.endsWith(".tar.gz")) {
      platform = "linux-x86_64";
      kind = "tar.gz";
    } else if (/macos-universal.*\.(dmg|zip)$/.test(name)) {
      platform = "macos-universal";
      kind = name.split(".").at(-1)!;
    } else if (/windows-x86_64.*\.(msi|exe|zip)$/.test(name)) {
      platform = "windows-x86_64";
      kind = name.split(".").at(-1)!;
    } else return [];
    return [artifact(name, platform, kind)];
  });
  if (!artifacts.length) throw Error("no release artifacts");
  if (new Set(artifacts.map((a) => `${a.platform}/${a.kind}`)).size !== artifacts.length)
    throw Error("duplicate platform/kind");
  const sourceName = `convt-${version}-source.tar.gz`;
  if (!existsSync(resolve(dir, sourceName))) throw Error("missing matching source archive");
  const audit = JSON.parse(readFileSync(resolve(dir, "source-audit.json"), "utf8"));
  const previous = history ? JSON.parse(readFileSync(history, "utf8")) : null;
  if (previous && (previous.schema_version !== 1 || previous.sequence >= sequence))
    throw Error("history must precede this metadata sequence");
  const build = {
    version,
    build_date: date,
    artifacts,
    source: artifact(sourceName, "source", "tar.gz"),
  };
  const builds = [...(previous?.builds ?? []).filter((b: any) => b.version !== version), build];
  const manifest = {
    schema_version: 1,
    sequence,
    issued_at: issued,
    expires_at: expires,
    distribution_ready:
      audit.distribution_ready === true &&
      Array.isArray(audit.gaps) &&
      audit.gaps.length === 0 &&
      Array.isArray(audit.covered_platforms) &&
      artifacts.every(
        (a) =>
          audit.covered_platforms.includes(a.platform) &&
          !audit.platform_gaps?.[a.platform]?.length,
      ) &&
      !process.env.CONVT_VERIFICATION_ONLY,
    purchase_url: "https://convt.app/pricing",
    builds,
  };
  const output = resolve(dir, "release-manifest.json");
  if (existsSync(output)) throw Error("manifest already exists");
  const temporary = `${output}.pending`;
  writeFileSync(temporary, JSON.stringify(manifest, null, 2) + "\n", { flag: "wx" });
  const check = Bun.spawnSync(
    [
      "cargo",
      "run",
      "--offline",
      "--locked",
      "-p",
      "convt-update",
      "--example",
      "validate-manifest",
      "--",
      temporary,
    ],
    {
      cwd: repo,
      env: {
        ...process.env,
        CARGO_TARGET_DIR:
          process.env.CONVT_RELEASE_TOOL_TARGET ??
          resolve(tmpdir(), `convt-release-tools-${process.getuid?.() ?? "user"}`),
      },
      stdout: "inherit",
      stderr: "inherit",
      timeout: 120000,
    },
  );
  if (check.exitCode !== 0)
    throw Error("client schema validation failed; pending manifest was not published");
  const { renameSync } = await import("node:fs");
  renameSync(temporary, output);
  console.log(
    `manifest: ${artifacts.length} artifacts, distribution_ready=${manifest.distribution_ready}`,
  );
} else throw Error("commands: keygen, generate, sign");
