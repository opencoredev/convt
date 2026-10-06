// Webhook signing for the mock. Deliberately not the verifier's code: the current
// scheme comes from the `standardwebhooks` library, the legacy one from node's
// HMAC, so a bug in packages/billing/src/verify.ts cannot hide in both.

import { createHmac } from "node:crypto";

import { Webhook } from "standardwebhooks";

export type Scheme = "standard" | "legacy";

export function signDelivery(
  secret: string,
  scheme: Scheme,
  id: string,
  timestampSeconds: number,
  body: string,
): string {
  if (scheme === "standard") {
    return new Webhook(secret).sign(id, new Date(timestampSeconds * 1000), body);
  }
  // Secrets created before 8 September 2026: the UTF-8 bytes of the whole string.
  const mac = createHmac("sha256", Buffer.from(secret, "utf8"))
    .update(`${id}.${timestampSeconds}.${body}`)
    .digest("base64");
  return `v1,${mac}`;
}

export function deliveryHeaders(
  secret: string,
  scheme: Scheme,
  id: string,
  timestampSeconds: number,
  body: string,
): Record<string, string> {
  return {
    "content-type": "application/json",
    "user-agent": "billing-mock (Polar webhooks)",
    "webhook-id": id,
    "webhook-timestamp": String(timestampSeconds),
    "webhook-signature": signDelivery(secret, scheme, id, timestampSeconds, body),
  };
}
