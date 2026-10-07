// Build-time content for the public site. Runs before `vite build`:
//
// 1. Validates content/release-manifest.json when present (the release pipeline's
//    manifest, packaging/release/manifest.schema.json).
// 2. Regenerates content/formats.json from `convt formats --json`, `convt engines`
//    and `convt targets` for every format, so the formats page matches the registry.
//
// Formats come from the registry on this machine, and `default_registry()` only
// registers engines that run here. When an engine is unavailable the matrix would
// be missing conversions, so the script keeps the committed snapshot and warns.
// CONVT_FORMATS_STRICT=1 (release builds) turns that warning into a failure.
//
// Environment:
//   CONVT_BIN              use this convt binary instead of building convt-cli
//   CONVT_LIBHEIF_DIR      bundled libheif, as for the CLI; when unset and a Linux
//                          bundle exists in packaging/out, the script uses it
//   CONVT_FORMATS_SKIP=1   do not touch formats.json
//   CONVT_FORMATS_STRICT=1 fail instead of keeping the snapshot

import { spawnSync } from "node:child_process";
import { existsSync, readFileSync, writeFileSync } from "node:fs";
import { join, resolve } from "node:path";

import { parseReleaseManifest } from "../src/lib/release-manifest.ts";

const web = resolve(import.meta.dir, "..");
const root = resolve(web, "../..");
const content = join(web, "content");

function warn(message: string) {
  console.warn(`generate-content: ${message}`);
}

// 1. Release manifest (optional until the first release)
const manifestPath = join(content, "release-manifest.json");
if (existsSync(manifestPath)) {
  const manifest = parseReleaseManifest(JSON.parse(readFileSync(manifestPath, "utf8")));
  console.log(
    `generate-content: release manifest sequence ${manifest.sequence}, distribution_ready=${manifest.distribution_ready}`,
  );
} else {
  console.log("generate-content: no release manifest; downloads show Shipping today");
}

// 2. Formats
const strict = process.env.CONVT_FORMATS_STRICT === "1";
const formatsPath = join(content, "formats.json");

function keepSnapshot(reason: string) {
  if (strict) {
    console.error(`generate-content: ${reason} (CONVT_FORMATS_STRICT=1)`);
    process.exit(1);
  }
  if (!existsSync(formatsPath)) {
    console.error(`generate-content: ${reason}, and there is no formats.json snapshot`);
    process.exit(1);
  }
  warn(`${reason}; keeping the committed content/formats.json`);
}

type CliFormat = {
  id: string;
  name: string;
  category: string;
  extensions: string[];
  mime: string;
};

function convtBinary(): string | null {
  if (process.env.CONVT_BIN) return process.env.CONVT_BIN;
  const build = spawnSync("cargo", ["build", "-q", "-p", "convt-cli"], {
    cwd: root,
    stdio: ["ignore", "inherit", "inherit"],
  });
  const target = process.env.CARGO_TARGET_DIR ?? join(root, "target");
  const bin = join(target, "debug", process.platform === "win32" ? "convt.exe" : "convt");
  if (build.error || build.status !== 0) {
    if (strict || !existsSync(bin)) return null;
    warn("cargo build failed; using the last built target/debug/convt");
  }
  return existsSync(bin) ? bin : null;
}

function runEnv() {
  const env = { ...process.env };
  const bundle = join(root, "packaging/out/convt/lib");
  if (!env.CONVT_LIBHEIF_DIR && existsSync(join(bundle, "libheif.so"))) {
    env.CONVT_LIBHEIF_DIR = bundle;
    env.CONVT_LIBHEIF_PLUGIN_DIR = join(bundle, "libheif/plugins");
    // The plugins link the bundle's codec libraries.
    env.LD_LIBRARY_PATH = bundle;
  }
  return env;
}

function generateFormats() {
  if (process.env.CONVT_FORMATS_SKIP === "1") return warn("CONVT_FORMATS_SKIP=1, not regenerating");
  const bin = convtBinary();
  if (!bin) return keepSnapshot("could not build or find the convt CLI");
  const env = runEnv();
  const run = (args: string[]) => {
    const out = spawnSync(bin, args, { env, encoding: "utf8" });
    if (out.error || out.status !== 0)
      throw new Error(`convt ${args.join(" ")} failed: ${out.stderr || out.error}`);
    return out.stdout;
  };

  const engines = run(["engines"]).trim().split("\n");
  const unavailable = engines.filter((line) => line.includes("(unavailable"));
  if (unavailable.length) return keepSnapshot(`engines unavailable: ${unavailable.join("; ")}`);

  const formats = JSON.parse(run(["formats", "--json"])) as CliFormat[];
  const result = {
    engines: engines.map((line) => line.trim()),
    formats: formats.map((f) => ({
      id: f.id,
      name: f.name,
      category: f.category,
      extensions: f.extensions,
      targets: run(["targets", `file.${f.extensions[0]}`])
        .split("\n")
        .filter(Boolean),
    })),
  };
  const json = `${JSON.stringify(result, null, 2)}\n`;
  const previous = existsSync(formatsPath) ? readFileSync(formatsPath, "utf8") : "";
  if (previous !== json) writeFileSync(formatsPath, json);
  console.log(
    `generate-content: ${result.formats.length} formats from ${result.engines.length} engines${previous === json ? " (unchanged)" : ""}`,
  );
}

generateFormats();
