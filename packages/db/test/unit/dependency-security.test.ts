import { expect, test } from "bun:test";

test("every locked esbuild is patched for GHSA-67mh-4wv8-2f99", async () => {
  const lock = await Bun.file(new URL("../../../../bun.lock", import.meta.url)).text();
  const versions = [...lock.matchAll(/\["esbuild@(\d+)\.(\d+)\.(\d+)"/g)];
  expect(versions.length).toBeGreaterThan(0);
  for (const [, major, minor, patch] of versions) {
    const safe =
      Number(major) > 0 || Number(minor) > 24 || (Number(minor) === 24 && Number(patch) > 2);
    expect(safe).toBe(true);
  }
});
