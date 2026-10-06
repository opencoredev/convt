// Runs the real Rust verifier: `convt license activate <key>` with a file key store
// and HOME and XDG directories in a temporary directory, so nothing touches the
// user's keyring or config.

import { mkdtempSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";

const root = join(import.meta.dir, "..", "..", "..");

export async function verifyWithCli(token: string, publicKey: string, buildDate?: string) {
  const home = mkdtempSync(join(tmpdir(), "convt-cli-"));
  try {
    const env = {
      ...process.env,
      PATH: `${process.env.HOME}/.cargo/bin:${process.env.PATH}`,
      HOME: home,
      XDG_CONFIG_HOME: join(home, "config"),
      XDG_DATA_HOME: join(home, "data"),
      CONVT_LICENSE_STORE: "file",
      CONVT_LICENSE_ENFORCE: "1",
      CONVT_LICENSE_PUBKEY: publicKey,
      ...(buildDate ? { CONVT_BUILD_DATE: buildDate } : {}),
    };
    // Build with the usual environment (so the build is not redone with a release
    // key), then pass the key and enforcement at run time, as source builds allow.
    const clean: Record<string, string | undefined> = { ...process.env, PATH: env.PATH };
    for (const k of [
      "CONVT_LICENSE_PUBKEY",
      "CONVT_LICENSE_ENFORCE",
      "CONVT_BUILD_DATE",
      "SOURCE_DATE_EPOCH",
    ])
      delete clean[k];
    const build = Bun.spawn(["cargo", "build", "-q", "-p", "convt-cli"], {
      cwd: root,
      env: clean,
      stdout: "inherit",
      stderr: "inherit",
    });
    const buildTimer = setTimeout(() => build.kill(), 600_000);
    if ((await build.exited) !== 0) throw new Error("cargo build -p convt-cli failed");
    clearTimeout(buildTimer);
    const p = Bun.spawn([join(root, "target", "debug", "convt"), "license", "activate", token], {
      cwd: home,
      env,
      stdout: "pipe",
      stderr: "pipe",
    });
    const timer = setTimeout(() => p.kill(), 300_000);
    const [out, err] = await Promise.all([
      new Response(p.stdout).text(),
      new Response(p.stderr).text(),
    ]);
    const status = await p.exited;
    clearTimeout(timer);
    return { status, text: `${out}${err}` };
  } finally {
    rmSync(home, { recursive: true, force: true });
  }
}
