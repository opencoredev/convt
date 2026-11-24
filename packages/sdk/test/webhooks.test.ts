import { expect, test } from "bun:test";
import { verifyWebhook } from "../src/index";

// Same vectors as crates/convt-server/src/webhooks.rs.
const secret = "whsec_test";
const t = 1_700_000_000;

async function sign(body: string, timestamp = t) {
  const key = await crypto.subtle.importKey(
    "raw",
    new TextEncoder().encode(secret),
    { name: "HMAC", hash: "SHA-256" },
    false,
    ["sign"],
  );
  const mac = await crypto.subtle.sign(
    "HMAC",
    key,
    new TextEncoder().encode(`${timestamp}.${body}`),
  );
  return Array.from(new Uint8Array(mac), (b) => b.toString(16).padStart(2, "0")).join("");
}

test("matches the server's signature vector", async () => {
  expect(await sign('{"ok":true}')).toBe(
    "85876387ad9d6be57a04653bc0729da757049f58afb10ba6cac3bedaecf4fda3",
  );
});

test("accepts a valid event and refuses tampering, staleness and junk", async () => {
  const body = JSON.stringify({
    id: "evt_1",
    type: "job.succeeded",
    created_at: "2026-11-23T00:00:00Z",
    data: { job: { id: "job_1", status: "succeeded" }, outputs: [], expires_in: null },
  });
  const signature = `t=${t},v1=${await sign(body)}`;
  const event = await verifyWebhook({ body, signature, secret, now: t * 1000 });
  expect(event.type).toBe("job.succeeded");
  expect(event.data.job.id).toBe("job_1");
  const bytes = new TextEncoder().encode(body);
  expect((await verifyWebhook({ body: bytes, signature, secret, now: t * 1000 })).id).toBe("evt_1");
  await expect(
    verifyWebhook({ body: body.replace("job_1", "job_2"), signature, secret, now: t * 1000 }),
  ).rejects.toThrow("does not match");
  await expect(verifyWebhook({ body, signature, secret, now: (t + 301) * 1000 })).rejects.toThrow(
    "tolerance",
  );
  await expect(verifyWebhook({ body, signature: null, secret })).rejects.toThrow("Missing");
  await expect(
    verifyWebhook({
      body,
      signature: `t=${t},v1=${await sign(body)}`,
      secret: "whsec_other",
      now: t * 1000,
    }),
  ).rejects.toThrow("does not match");
  const junk = '{"hello":1}';
  await expect(
    verifyWebhook({
      body: junk,
      signature: `t=${t},v1=${await sign(junk)}`,
      secret,
      now: t * 1000,
    }),
  ).rejects.toThrow("not a convt event");
});
