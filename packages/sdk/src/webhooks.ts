import type { Job, Output } from "./index.js";

/** The body convt POSTs to a job's `webhook_url` when the job finishes. */
export type WebhookEvent = {
  /** Stable per job. Deliveries are at least once, so dedupe on it. */
  id: string;
  type: "job.succeeded" | "job.failed" | "job.cancelled";
  created_at: string;
  data: {
    job: Job;
    /** Signed output URLs for a succeeded job, valid for `expires_in` seconds. */
    outputs: Output[];
    expires_in: number | null;
  };
};

const encoder = new TextEncoder();

function toHex(bytes: ArrayBuffer): string {
  return Array.from(new Uint8Array(bytes), (b) => b.toString(16).padStart(2, "0")).join("");
}

/** Compares two strings in time that depends only on their length. */
function equalConstantTime(a: string, b: string): boolean {
  if (a.length !== b.length) return false;
  let difference = 0;
  for (let i = 0; i < a.length; i++) difference |= a.charCodeAt(i) ^ b.charCodeAt(i);
  return difference === 0;
}

function isWebhookEvent(value: unknown): value is WebhookEvent {
  if (typeof value !== "object" || value === null) return false;
  if (!("id" in value) || typeof value.id !== "string") return false;
  if (!("type" in value) || typeof value.type !== "string" || !value.type.startsWith("job."))
    return false;
  if (!("data" in value) || typeof value.data !== "object" || value.data === null) return false;
  return "job" in value.data && typeof value.data.job === "object" && value.data.job !== null;
}

/**
 * Checks a webhook's `Convt-Signature` header against the raw request body and
 * returns the parsed event. Throws when the signature is wrong or older than
 * `toleranceSeconds` (default five minutes). Pass the body exactly as received,
 * before any JSON parsing. Get the secret once with `convt.webhookSecret()`.
 */
export async function verifyWebhook({
  body,
  signature,
  secret,
  toleranceSeconds = 300,
  now = Date.now(),
}: {
  body: string | Uint8Array;
  signature: string | null | undefined;
  secret: string;
  toleranceSeconds?: number;
  now?: number;
}): Promise<WebhookEvent> {
  const parts = new Map(
    (signature ?? "").split(",").map((part) => {
      const at = part.indexOf("=");
      return [part.slice(0, at).trim(), part.slice(at + 1).trim()] as const;
    }),
  );
  const timestamp = Number(parts.get("t"));
  const expected = parts.get("v1");
  if (!Number.isInteger(timestamp) || !expected)
    throw new Error("Missing or malformed Convt-Signature header.");
  if (Math.abs(now / 1000 - timestamp) > toleranceSeconds)
    throw new Error("Webhook timestamp is outside the tolerance window.");
  const raw = typeof body === "string" ? encoder.encode(body) : body;
  const key = await globalThis.crypto.subtle.importKey(
    "raw",
    encoder.encode(secret),
    { name: "HMAC", hash: "SHA-256" },
    false,
    ["sign"],
  );
  const signed = new Uint8Array(encoder.encode(`${timestamp}.`).length + raw.length);
  signed.set(encoder.encode(`${timestamp}.`));
  signed.set(raw, encoder.encode(`${timestamp}.`).length);
  const actual = toHex(await globalThis.crypto.subtle.sign("HMAC", key, signed));
  if (!equalConstantTime(actual, expected)) throw new Error("Webhook signature does not match.");
  const event: unknown = JSON.parse(new TextDecoder().decode(raw));
  if (!isWebhookEvent(event)) throw new Error("Webhook body is not a convt event.");
  return event;
}
