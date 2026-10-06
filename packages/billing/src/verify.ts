// Webhook signature verification, before anything parses the body. Our own code,
// tested against the `standardwebhooks` library, Polar's legacy scheme and the
// SDK's `validateEvent`, so the mock and the verifier cannot share a bug.
//
// Polar signs `${webhook-id}.${webhook-timestamp}.${body}` with HMAC-SHA256. Secrets
// created before 8 September 2026 use the UTF-8 bytes of the whole `whsec_...`
// string as the key; newer ones follow Standard Webhooks (base64 of the part after
// `whsec_`). Like the SDK, we try both.

export const maxBodyBytes = 256 * 1024;
export const toleranceSeconds = 5 * 60;

export type Delivery = {
  id: string;
  timestamp: number;
  /** The verified bytes, decoded as strict UTF-8. */
  body: string;
};

export type Rejected = {
  rejected: true;
  status: 401 | 413 | 405;
  reason:
    | "method"
    | "too_large"
    | "missing_headers"
    | "bad_timestamp"
    | "stale"
    | "future"
    | "no_match"
    | "bad_utf8";
};

function base64Decode(text: string): Uint8Array | null {
  try {
    const raw = atob(text);
    const out = new Uint8Array(raw.length);
    for (let i = 0; i < raw.length; i++) out[i] = raw.charCodeAt(i);
    return out;
  } catch {
    return null;
  }
}

/** The HMAC keys Polar documents for one secret: Standard Webhooks first, then legacy. */
export function signingKeys(secret: string): Uint8Array[] {
  const keys: Uint8Array[] = [];
  if (secret.startsWith("whsec_")) {
    const decoded = base64Decode(secret.slice(6));
    if (decoded && decoded.length > 0) keys.push(decoded);
  }
  keys.push(new TextEncoder().encode(secret));
  return keys;
}

function constantTimeEqual(a: Uint8Array, b: Uint8Array): boolean {
  if (a.length !== b.length) return false;
  let diff = 0;
  for (let i = 0; i < a.length; i++) diff |= a[i] ^ b[i];
  return diff === 0;
}

const reject = (status: Rejected["status"], reason: Rejected["reason"]): Rejected => ({
  rejected: true,
  status,
  reason,
});

export async function verifyWebhook(
  raw: Uint8Array,
  headers: Headers,
  secret: string,
  now: Date,
): Promise<Delivery | Rejected> {
  if (raw.byteLength > maxBodyBytes) return reject(413, "too_large");
  const id = headers.get("webhook-id");
  const timestampText = headers.get("webhook-timestamp");
  const signatureHeader = headers.get("webhook-signature");
  if (!id || !timestampText || !signatureHeader) return reject(401, "missing_headers");
  if (!/^\d{1,12}$/.test(timestampText)) return reject(401, "bad_timestamp");
  const timestamp = Number(timestampText);
  const nowSeconds = Math.floor(now.getTime() / 1000);
  if (timestamp < nowSeconds - toleranceSeconds) return reject(401, "stale");
  if (timestamp > nowSeconds + toleranceSeconds) return reject(401, "future");

  const prefix = new TextEncoder().encode(`${id}.${timestamp}.`);
  const signed = new Uint8Array(prefix.length + raw.byteLength);
  signed.set(prefix);
  signed.set(raw, prefix.length);

  const candidates: Uint8Array[] = [];
  for (const entry of signatureHeader.split(" ")) {
    const [version, value] = entry.split(",", 2);
    if (version !== "v1" || !value) continue;
    const decoded = base64Decode(value);
    if (decoded && decoded.length === 32) candidates.push(decoded);
  }
  let matched = false;
  for (const keyBytes of signingKeys(secret)) {
    const key = await crypto.subtle.importKey(
      "raw",
      keyBytes as BufferSource,
      { name: "HMAC", hash: "SHA-256" },
      false,
      ["sign"],
    );
    const expected = new Uint8Array(await crypto.subtle.sign("HMAC", key, signed));
    // Compare against every candidate so timing does not reveal which one matched.
    for (const candidate of candidates) matched = constantTimeEqual(expected, candidate) || matched;
  }
  if (!matched) return reject(401, "no_match");
  let body: string;
  try {
    body = new TextDecoder("utf-8", { fatal: true, ignoreBOM: true }).decode(raw);
  } catch {
    return reject(401, "bad_utf8");
  }
  return { id, timestamp, body };
}

export function isRejected(result: Delivery | Rejected): result is Rejected {
  return "rejected" in result;
}
