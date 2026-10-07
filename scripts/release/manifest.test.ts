import { describe, test, expect } from "bun:test";
import { createOnlyPut } from "./r2-put";
import { mkdtempSync, writeFileSync, rmSync, symlinkSync, mkdirSync } from "node:fs";
import { tmpdir } from "node:os";
import { resolve, join } from "node:path";
import { createHash } from "node:crypto";
const repo = resolve(import.meta.dir, "../..");
const tool = resolve(import.meta.dir, "manifest.ts");
const run = (args: string[], env: Record<string, string> = {}) =>
  Bun.spawnSync(["bun", tool, ...args], {
    cwd: repo,
    env: { ...process.env, CONVT_UPDATE_PUBKEY: "", CONVT_LICENSE_PUBKEY: "", ...env },
  });
describe("release key and upload guards", () => {
  test("external key, symlink confinement, overwrite protection and tampered envelope", () => {
    const work = mkdtempSync(join(tmpdir(), "convt-release-key-test-"));
    try {
      const key = join(work, "update.seed");
      expect(run(["keygen", key]).exitCode).toBe(0);
      expect(run(["keygen", key]).exitCode).not.toBe(0);
      symlinkSync(repo, join(work, "repo"));
      expect(run(["keygen", join(work, "repo", "forbidden-update-test.seed")]).exitCode).not.toBe(
        0,
      );
      const publicKey = run(["public-key", key]).stdout.toString().trim();
      const input = join(work, "manifest.json"),
        signed = join(work, "signed.json");
      writeFileSync(input, '{"example":true}\n');
      expect(run(["sign", input, key, signed]).exitCode).toBe(0);
      expect(run(["verify", input, signed], { CONVT_UPDATE_PUBKEY: publicKey }).exitCode).toBe(0);
      writeFileSync(input, '{"example":false}\n');
      expect(run(["verify", input, signed], { CONVT_UPDATE_PUBKEY: publicKey }).exitCode).not.toBe(
        0,
      );
      // A key outside the tree whose final symlink points inside is refused.
      symlinkSync(resolve(repo, "Cargo.toml"), join(work, "inside.seed"));
      expect(run(["public-key", join(work, "inside.seed")]).stderr.toString()).toContain("outside");
    } finally {
      rmSync(work, { recursive: true, force: true });
    }
  });
  test("verification manifest refuses real upload before any network operation", () => {
    const work = mkdtempSync(join(tmpdir(), "convt-release-upload-test-"));
    try {
      const dir = join(work, "0.1.0");
      mkdirSync(dir);
      const bytes = Buffer.from("verification artifact");
      const sha = createHash("sha256").update(bytes).digest("hex");
      writeFileSync(join(dir, "convt.AppImage"), bytes);
      writeFileSync(join(dir, "source.tar.gz"), bytes);
      const a = {
        platform: "linux-x86_64",
        kind: "AppImage",
        url: "https://downloads.convt.app/0.1.0/convt.AppImage",
        size: bytes.length,
        sha256: sha,
      };
      const m = {
        schema_version: 1,
        sequence: 1,
        issued_at: 1791331200,
        expires_at: 1791331201,
        distribution_ready: false,
        purchase_url: "https://convt.app/pricing",
        builds: [
          {
            version: "0.1.0",
            build_date: "2026-10-07",
            artifacts: [a],
            source: {
              ...a,
              platform: "source",
              kind: "tar.gz",
              url: "https://downloads.convt.app/0.1.0/source.tar.gz",
            },
          },
        ],
      };
      writeFileSync(join(dir, "release-manifest.json"), JSON.stringify(m));
      writeFileSync(
        join(dir, "source-audit.json"),
        JSON.stringify({ distribution_ready: false, gaps: ["missing exact source"] }),
      );
      const command = ["bash", join(import.meta.dir, "upload.sh")];
      expect(Bun.spawnSync([...command, "--dry-run", dir]).exitCode).toBe(0);
      const result = Bun.spawnSync([...command, "--upload", dir]);
      expect(result.exitCode).not.toBe(0);
      expect(result.stderr.toString()).toContain("Upload blocked");
      writeFileSync(join(dir, "convt.AppImage"), Buffer.from("tampered"));
      expect(Bun.spawnSync([...command, "--dry-run", dir]).exitCode).not.toBe(0);
    } finally {
      rmSync(work, { recursive: true, force: true });
    }
  });
});

test("concurrent immutable writes send atomic HTTP preconditions", async () => {
  let stored = false;
  const server = Bun.serve({
    hostname: "127.0.0.1",
    port: 0,
    fetch(request) {
      if (request.method !== "PUT" || request.headers.get("if-none-match") !== "*")
        return new Response(null, { status: 400 });
      if (stored) return new Response(null, { status: 412 });
      stored = true;
      return new Response(null, { status: 200 });
    },
  });
  try {
    const url = `http://127.0.0.1:${server.port}/release`;
    const results = await Promise.allSettled([
      createOnlyPut(url, new Blob(["a"])),
      createOnlyPut(url, new Blob(["b"])),
    ]);
    expect(results.filter((r) => r.status === "fulfilled")).toHaveLength(1);
    expect(results.filter((r) => r.status === "rejected")).toHaveLength(1);
  } finally {
    await server.stop(true);
  }
});

test("readiness requires empty gaps and source coverage for every artifact platform", () => {
  const work = mkdtempSync(join(tmpdir(), "convt-release-coverage-test-"));
  try {
    for (const [name, audit, mac, expected] of [
      [
        "linux",
        { distribution_ready: true, gaps: [], covered_platforms: ["linux-x86_64"] },
        false,
        true,
      ],
      [
        "gap",
        { distribution_ready: true, gaps: ["missing source"], covered_platforms: ["linux-x86_64"] },
        false,
        false,
      ],
      [
        "mac",
        { distribution_ready: true, gaps: [], covered_platforms: ["linux-x86_64"] },
        true,
        false,
      ],
      [
        "mac-gap",
        {
          distribution_ready: true,
          gaps: [],
          covered_platforms: ["linux-x86_64", "macos-arm64"],
          platform_gaps: { "macos-arm64": ["unmatched x265"] },
        },
        true,
        false,
      ],
      [
        "windows-optional",
        { distribution_ready: true, gaps: [], covered_platforms: ["linux-x86_64"] },
        "windows",
        true,
      ],
    ] as const) {
      const dir = join(work, name, "0.1.0");
      mkdirSync(dir, { recursive: true });
      writeFileSync(join(dir, "convt-linux-x86_64.tar.gz"), "payload");
      writeFileSync(join(dir, "convt-0.1.0-source.tar.gz"), "sources");
      if (mac === true) writeFileSync(join(dir, "convt-macos-arm64.zip"), "Mac payload");
      if (mac === "windows")
        writeFileSync(join(dir, "convt-0.1.0-windows-x86_64.msi"), "Windows MSI");
      writeFileSync(join(dir, "source-audit.json"), JSON.stringify(audit));
      const result = run(["generate", dir, "0.1.0", "2026-10-07", "https://downloads.convt.app"], {
        SOURCE_DATE_EPOCH: "1791331200",
        CONVT_VERIFICATION_ONLY: "",
      });
      expect(result.exitCode).toBe(0);
      expect(
        JSON.parse(require("node:fs").readFileSync(join(dir, "release-manifest.json"), "utf8"))
          .distribution_ready,
      ).toBe(expected);
      if (!expected) {
        const upload = Bun.spawnSync(["bash", join(import.meta.dir, "upload.sh"), "--upload", dir]);
        expect(upload.exitCode).not.toBe(0);
        expect(upload.stderr.toString()).toContain("Upload blocked");
      }
    }
  } finally {
    rmSync(work, { recursive: true, force: true });
  }
});
