import { expect, test } from "bun:test";
import { assertOwnedDatabase } from "../../../packages/db/src/guard";
import { localFetch, meterMatches, type Receipt } from "../src/safety";

test("rejects remote storage URLs and redirect hops before dispatch", async () => {
  await expect(
    localFetch("https://example.com/signed-upload", { method: "PUT", body: "fixture" }),
  ).rejects.toThrow("non-loopback");
  const server = Bun.serve({
    hostname: "127.0.0.1",
    port: 0,
    fetch: () =>
      new Response(null, { status: 307, headers: { location: "https://example.com/fixture" } }),
  });
  try {
    await expect(localFetch(server.url.href)).rejects.toThrow("non-loopback");
  } finally {
    server.stop(true);
  }
});

test("follows local pricing redirect and bounds stalled response bodies", async () => {
  const server = Bun.serve({
    hostname: "127.0.0.1",
    port: 0,
    fetch(request) {
      const path = new URL(request.url).pathname;
      if (path === "/pricing")
        return new Response(null, { status: 302, headers: { location: "/#pricing" } });
      if (path === "/stall")
        return new Response(
          new ReadableStream({
            start(c) {
              c.enqueue(new Uint8Array([1]));
            },
          }),
        );
      return new Response("pricing section");
    },
  });
  try {
    expect(await (await localFetch(server.url.href + "pricing")).text()).toBe("pricing section");
    const started = performance.now();
    await expect(
      localFetch(server.url.href + "stall", {}, 50).then((r) => r.arrayBuffer()),
    ).rejects.toThrow();
    expect(performance.now() - started).toBeLessThan(1000);
  } finally {
    server.stop(true);
  }
});

test("meter validation refuses additional IDs, duplicate IDs and excessive quantities", () => {
  const usage = [{ job_id: "job1", user_id: "customer1", reported_at: "now" }];
  const receipt: Receipt = {
    external_id: "job1",
    external_customer_id: "customer1",
    metadata: { quantity: 1 },
  };
  expect(meterMatches([receipt], usage, ["customer1"])).toBe(true);
  expect(
    meterMatches([receipt, { ...receipt, external_id: "unexpected-charge" }], usage, ["customer1"]),
  ).toBe(false);
  expect(meterMatches([receipt, receipt], usage, ["customer1"])).toBe(false);
  expect(meterMatches([{ ...receipt, metadata: { quantity: 2 } }], usage, ["customer1"])).toBe(
    false,
  );
  expect(
    meterMatches(
      [receipt, { ...receipt, external_id: "historical", external_customer_id: "other" }],
      usage,
      ["customer1"],
    ),
  ).toBe(true);
});

test("database guard refuses a different local database before fixture writes", () => {
  expect(() => assertOwnedDatabase("postgresql://convt_owner:invalid@127.0.0.1:1/convt")).toThrow();
});
