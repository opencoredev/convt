// License tokens, byte-compatible with crates/convt-license: base64url of the JSON
// payload, a dot, and the base64url Ed25519 signature over the encoded payload.
// The key is the 32-byte seed in the `.convt-dev/license.key` format (base64url).

import { decode, encode } from "./base64url";

export type Plan = "desktop" | "pro";

/** Field order matters: it must match the Rust struct so tokens are identical. */
export type License = {
  id: string;
  email: string;
  plan: Plan;
  /** Issue date, `YYYY-MM-DD`. */
  issued: string;
  /** Builds dated on or before this day are covered, `YYYY-MM-DD`. */
  updates_until: string;
};

// PKCS#8 wrapper for a raw Ed25519 seed (RFC 8410).
const pkcs8Prefix = Uint8Array.from([
  0x30, 0x2e, 0x02, 0x01, 0x00, 0x30, 0x05, 0x06, 0x03, 0x2b, 0x65, 0x70, 0x04, 0x22, 0x04, 0x20,
]);

const datePattern = /^\d{4}-\d{2}-\d{2}$/;

export function isDate(text: string): boolean {
  if (!datePattern.test(text)) return false;
  const [y, m, d] = text.split("-").map(Number);
  const date = new Date(Date.UTC(y, m - 1, d));
  return date.getUTCFullYear() === y && date.getUTCMonth() === m - 1 && date.getUTCDate() === d;
}

function payloadJson(license: License): string {
  const { id, email, plan, issued, updates_until } = license;
  return JSON.stringify({ id, email, plan, issued, updates_until });
}

export async function importSigningKey(seed: Uint8Array): Promise<CryptoKey> {
  if (seed.length !== 32) throw new Error("an Ed25519 seed is 32 bytes");
  const der = new Uint8Array(pkcs8Prefix.length + 32);
  der.set(pkcs8Prefix);
  der.set(seed, pkcs8Prefix.length);
  return crypto.subtle.importKey("pkcs8", der, { name: "Ed25519" }, true, ["sign"]);
}

/** The seed from a `license.key` file's text. */
export function parseSeed(text: string): Uint8Array {
  const seed = decode(text.trim());
  if (!seed || seed.length !== 32) throw new Error("the signing key is malformed");
  return seed;
}

/** The base64url public key for a signing key, as `CONVT_LICENSE_PUBKEY` holds it. */
export async function publicKeyOf(key: CryptoKey): Promise<string> {
  const jwk = await crypto.subtle.exportKey("jwk", key);
  if (!jwk.x) throw new Error("not an Ed25519 key");
  return jwk.x;
}

export async function importVerifyKey(publicKey: string): Promise<CryptoKey> {
  const raw = decode(publicKey.trim());
  if (!raw || raw.length !== 32) throw new Error("the public key is malformed");
  return crypto.subtle.importKey("raw", raw, { name: "Ed25519" }, true, ["verify"]);
}

export async function sign(license: License, key: CryptoKey): Promise<string> {
  if (!isDate(license.issued) || !isDate(license.updates_until))
    throw new Error("dates are YYYY-MM-DD");
  const payload = encode(new TextEncoder().encode(payloadJson(license)));
  const signature = await crypto.subtle.sign("Ed25519", key, new TextEncoder().encode(payload));
  return `${payload}.${encode(new Uint8Array(signature))}`;
}

export type VerifyError = "malformed" | "bad_signature";

export async function verify(
  token: string,
  key: CryptoKey,
): Promise<{ ok: true; license: License } | { ok: false; error: VerifyError }> {
  const [payload, sig, extra] = token.trim().split(".");
  if (!payload || !sig || extra !== undefined) return { ok: false, error: "malformed" };
  const signature = decode(sig);
  if (!signature || signature.length !== 64) return { ok: false, error: "malformed" };
  const valid = await crypto.subtle.verify(
    "Ed25519",
    key,
    signature,
    new TextEncoder().encode(payload),
  );
  if (!valid) return { ok: false, error: "bad_signature" };
  const json = decode(payload);
  if (!json) return { ok: false, error: "malformed" };
  try {
    const value = JSON.parse(new TextDecoder("utf-8", { fatal: true }).decode(json)) as License;
    if (
      typeof value.id !== "string" ||
      typeof value.email !== "string" ||
      (value.plan !== "desktop" && value.plan !== "pro") ||
      !isDate(value.issued) ||
      !isDate(value.updates_until)
    )
      return { ok: false, error: "malformed" };
    return { ok: true, license: value };
  } catch {
    return { ok: false, error: "malformed" };
  }
}
