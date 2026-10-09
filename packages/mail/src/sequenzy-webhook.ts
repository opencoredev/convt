// Sequenzy's outbound webhooks. Each request carries X-Sequenzy-Timestamp and
// X-Sequenzy-Signature: `v1=` HMAC-SHA256 values (one per active secret) over
// `v1:{timestamp}:{raw body}`. Sequenzy does not say whether the digest is hex or
// base64, so both are accepted.

/** Events that mean the address must get no more campaign email. */
export const optOutEvents = [
  "subscriber.unsubscribed",
  "email.unsubscribed",
  "email.complained",
  "email.bounced",
] as const;

export type OptOutEvent = (typeof optOutEvents)[number];

export type SequenzyWebhook =
  | {
      kind: "opt_out";
      id: string;
      type: OptOutEvent;
      /** convt's user id, when the contact has one. */
      externalId: string | null;
      email: string | null;
    }
  | { kind: "other"; id: string; type: string };

export type WebhookCheck =
  | { ok: true; event: SequenzyWebhook }
  | { ok: false; reason: "signature" | "stale" | "malformed" };

const toleranceSeconds = 5 * 60;

function hex(bytes: Uint8Array): string {
  return [...bytes].map((b) => b.toString(16).padStart(2, "0")).join("");
}

function base64(bytes: Uint8Array): string {
  return btoa(String.fromCharCode(...bytes));
}

function sameText(a: string, b: string): boolean {
  if (a.length !== b.length) return false;
  let diff = 0;
  for (let i = 0; i < a.length; i++) diff |= a.charCodeAt(i) ^ b.charCodeAt(i);
  return diff === 0;
}

export async function signSequenzyWebhook(
  secret: string,
  timestamp: number,
  raw: Uint8Array,
): Promise<string> {
  const key = await crypto.subtle.importKey(
    "raw",
    new TextEncoder().encode(secret),
    { name: "HMAC", hash: "SHA-256" },
    false,
    ["sign"],
  );
  const prefix = new TextEncoder().encode(`v1:${timestamp}:`);
  const message = new Uint8Array(prefix.length + raw.length);
  message.set(prefix);
  message.set(raw, prefix.length);
  return hex(new Uint8Array(await crypto.subtle.sign("HMAC", key, message)));
}

function text(value: unknown): string | null {
  return typeof value === "string" && value.trim() !== "" ? value.trim() : null;
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}

/** The first of `keys` set on `data` or on `data.subscriber`. */
function field(data: Record<string, unknown>, ...keys: string[]): string | null {
  const subscriber = isRecord(data.subscriber) ? data.subscriber : {};
  for (const key of keys) {
    const value = text(data[key]) ?? text(subscriber[key]);
    if (value) return value;
  }
  return null;
}

function parseEvent(raw: Uint8Array): SequenzyWebhook | null {
  let body: unknown;
  try {
    body = JSON.parse(new TextDecoder().decode(raw));
  } catch {
    return null;
  }
  if (!isRecord(body)) return null;
  const id = text(body.id);
  const type = text(body.type);
  if (!id || !type) return null;
  const optOut = optOutEvents.find((e) => e === type);
  if (!optOut) return { kind: "other", id, type };
  const data = isRecord(body.data) ? body.data : {};
  const email = field(data, "recipient", "email");
  return {
    kind: "opt_out",
    id,
    type: optOut,
    // Sequenzy documents snake_case for webhooks and camelCase for its API.
    externalId: field(data, "external_id", "externalId"),
    email: email ? email.toLowerCase() : null,
  };
}

/** Checks the signature and age, then parses the event. */
export async function verifySequenzyWebhook(input: {
  secret: string;
  headers: Headers;
  raw: Uint8Array;
  now: Date;
}): Promise<WebhookCheck> {
  const timestamp = Number(input.headers.get("x-sequenzy-timestamp"));
  const header = input.headers.get("x-sequenzy-signature") ?? "";
  if (!Number.isInteger(timestamp) || header === "") return { ok: false, reason: "signature" };
  const expected = await signSequenzyWebhook(input.secret, timestamp, input.raw);
  const expectedB64 = base64(
    new Uint8Array(expected.match(/../g)?.map((h) => parseInt(h, 16)) ?? []),
  );
  const matches = header
    .split(",")
    .map((part) => part.trim())
    .filter((part) => part.startsWith("v1="))
    .some((part) => {
      const value = part.slice(3);
      return sameText(value.toLowerCase(), expected) || sameText(value, expectedB64);
    });
  if (!matches) return { ok: false, reason: "signature" };
  if (Math.abs(input.now.getTime() / 1000 - timestamp) > toleranceSeconds)
    return { ok: false, reason: "stale" };
  const event = parseEvent(input.raw);
  return event ? { ok: true, event } : { ok: false, reason: "malformed" };
}
