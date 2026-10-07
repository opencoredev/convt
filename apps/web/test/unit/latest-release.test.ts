import { afterEach, expect, test } from "bun:test";

import { fetchLatestManifest, LATEST_MANIFEST_URL } from "../../src/server/latest-release";

const realFetch = globalThis.fetch;
afterEach(() => {
  globalThis.fetch = realFetch;
});

const sha = "a".repeat(64);
const artifact = (url: string) => ({
  platform: "linux-x86_64",
  kind: "tar.gz",
  url,
  size: 10,
  sha256: sha,
});
const manifest = (url: string) => ({
  schema_version: 1,
  sequence: 1,
  issued_at: 1_790_000_000,
  expires_at: 1_890_000_000,
  distribution_ready: true,
  purchase_url: "https://convt.app/pricing",
  builds: [
    {
      version: "0.2.0",
      build_date: "2026-10-07",
      artifacts: [artifact(url)],
      source: { ...artifact(url), platform: "source" },
    },
  ],
});
const serve = (status: number, body: unknown) => {
  const seen: string[] = [];
  globalThis.fetch = (async (input: RequestInfo | URL) => {
    seen.push(String(input));
    return new Response(JSON.stringify(body), { status });
  }) as typeof fetch;
  return seen;
};

test("reads the newest GitHub release's manifest", async () => {
  const url = "https://github.com/opencoredev/convt/releases/download/v0.2.0/convt.tar.gz";
  const seen = serve(200, manifest(url));
  expect((await fetchLatestManifest())?.builds[0].version).toBe("0.2.0");
  expect(seen).toEqual([LATEST_MANIFEST_URL]);
});

test("no release yet, or files outside this repository's releases, link nothing", async () => {
  serve(404, "Not Found");
  expect(await fetchLatestManifest()).toBeNull();
  serve(200, manifest("https://example.com/convt.tar.gz"));
  expect(await fetchLatestManifest()).toBeNull();
  serve(200, manifest("https://github.com/opencoredev/convt/releases/download/../../../evil/x/a"));
  expect(await fetchLatestManifest()).toBeNull();
  serve(200, { schema_version: 2 });
  expect(await fetchLatestManifest()).toBeNull();
});
