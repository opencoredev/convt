// The verifier against independent signers: the `standardwebhooks` library for the
// current scheme, node's HMAC for Polar's legacy scheme, and the SDK's own
// validateEvent as a second opinion on what Polar accepts.

import { createHmac, randomBytes } from "node:crypto";

import { webhooks } from "@polar-sh/sdk/2026-10";
import { describe, expect, test } from "bun:test";
import { Webhook } from "standardwebhooks";

import { isRejected, maxBodyBytes, verifyWebhook } from "../../src/verify";

const secret = `whsec_${randomBytes(24).toString("base64")}`;
const now = new Date("2026-10-05T12:00:00Z");
const nowSec = Math.floor(now.getTime() / 1000);
const body = JSON.stringify({
  type: "checkout.created",
  timestamp: now.toISOString(),
  api_version: "2026-10",
  data: { id: "chk_1" },
});

function standardSign(id: string, ts: number, payload: string, key = secret) {
  return new Webhook(key).sign(id, new Date(ts * 1000), payload);
}

function legacySign(id: string, ts: number, payload: string, key = secret) {
  // Polar's original scheme: the UTF-8 bytes of the whole whsec_ string as the key.
  const mac = createHmac("sha256", Buffer.from(key, "utf8"))
    .update(`${id}.${ts}.${payload}`)
    .digest("base64");
  return `v1,${mac}`;
}

function headers(id: string, ts: number | string, signature: string) {
  return new Headers({
    "webhook-id": id,
    "webhook-timestamp": String(ts),
    "webhook-signature": signature,
  });
}

const bytes = (s: string) => new TextEncoder().encode(s);

async function sdkAccepts(payload: string, h: Headers, at: Date) {
  const realNow = Date.now;
  Date.now = () => at.getTime();
  try {
    await webhooks.validateEvent(payload, Object.fromEntries(h.entries()), secret);
    return true;
  } catch (e) {
    if (e instanceof webhooks.PolarWebhookVerificationError) return false;
    throw e;
  } finally {
    Date.now = realNow;
  }
}

describe("verifyWebhook", () => {
  test("accepts the Standard Webhooks key and the legacy key, like the SDK", async () => {
    for (const sign of [standardSign, legacySign]) {
      const h = headers("msg_1", nowSec, sign("msg_1", nowSec, body));
      const result = await verifyWebhook(bytes(body), h, secret, now);
      expect(isRejected(result)).toBe(false);
      if (!isRejected(result)) expect(result.body).toBe(body);
      expect(await sdkAccepts(body, h, now)).toBe(true);
    }
  });

  test("the SDK and the verifier agree on every forgery", async () => {
    const good = standardSign("msg_1", nowSec, body);
    const otherSecret = `whsec_${randomBytes(24).toString("base64")}`;
    const changed = body.replace("chk_1", "chk_2");
    const cases: Array<[string, Uint8Array, Headers, string]> = [
      [
        "wrong secret",
        bytes(body),
        headers("msg_1", nowSec, standardSign("msg_1", nowSec, body, otherSecret)),
        "no_match",
      ],
      [
        "wrong legacy secret",
        bytes(body),
        headers("msg_1", nowSec, legacySign("msg_1", nowSec, body, otherSecret)),
        "no_match",
      ],
      ["one body byte changed", bytes(changed), headers("msg_1", nowSec, good), "no_match"],
      ["signature for another webhook-id", bytes(body), headers("msg_2", nowSec, good), "no_match"],
      [
        "6 minutes old",
        bytes(body),
        headers("msg_1", nowSec - 360, standardSign("msg_1", nowSec - 360, body)),
        "stale",
      ],
      [
        "6 minutes ahead",
        bytes(body),
        headers("msg_1", nowSec + 360, standardSign("msg_1", nowSec + 360, body)),
        "future",
      ],
      [
        "garbage signature header",
        bytes(body),
        headers("msg_1", nowSec, "v1,!!!not-base64 v2,abc garbage"),
        "no_match",
      ],
      [
        "timestamp that is not a number",
        bytes(body),
        headers("msg_1", "12a", good),
        "bad_timestamp",
      ],
    ];
    for (const [name, raw, h, reason] of cases) {
      const result = await verifyWebhook(raw, h, secret, now);
      expect({ name, reason: isRejected(result) ? result.reason : "accepted" }).toEqual({
        name,
        reason,
      });
      expect({ name, sdk: await sdkAccepts(new TextDecoder().decode(raw), h, now) }).toEqual({
        name,
        sdk: false,
      });
    }
  });

  test("missing headers are refused", async () => {
    for (const drop of ["webhook-id", "webhook-timestamp", "webhook-signature"]) {
      const h = headers("msg_1", nowSec, standardSign("msg_1", nowSec, body));
      h.delete(drop);
      const result = await verifyWebhook(bytes(body), h, secret, now);
      expect(isRejected(result) && result.reason).toBe("missing_headers");
    }
  });

  test("one bad and one good signature is accepted", async () => {
    const bad = standardSign("msg_1", nowSec, body, `whsec_${randomBytes(24).toString("base64")}`);
    const good = legacySign("msg_1", nowSec, body);
    const result = await verifyWebhook(
      bytes(body),
      headers("msg_1", nowSec, `${bad} ${good}`),
      secret,
      now,
    );
    expect(isRejected(result)).toBe(false);
  });

  test("an oversized body is refused before any HMAC", async () => {
    const big = "x".repeat(maxBodyBytes + 1);
    const h = headers("msg_1", nowSec, standardSign("msg_1", nowSec, big));
    const result = await verifyWebhook(bytes(big), h, secret, now);
    expect(isRejected(result) && result.status).toBe(413);
  });

  test("invalid UTF-8 with a valid signature is refused", async () => {
    const raw = Uint8Array.from([0x7b, 0xff, 0xfe, 0x7d]);
    const mac = createHmac("sha256", Buffer.from(secret.slice(6), "base64"))
      .update(Buffer.concat([Buffer.from(`msg_1.${nowSec}.`), Buffer.from(raw)]))
      .digest("base64");
    const result = await verifyWebhook(raw, headers("msg_1", nowSec, `v1,${mac}`), secret, now);
    expect(isRejected(result) && result.reason).toBe("bad_utf8");
  });
});
