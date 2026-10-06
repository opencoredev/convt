// API keys: `cvt_live_` and 32 Crockford base32 characters (160 random bits). The
// database keeps only the SHA-256 of the whole key and a display prefix
// (`cvt_live_` and the first 8 characters). P9 creates keys; P6 hashes and masks.

import { randomBytes } from "./ids";

const alphabet = "0123456789abcdefghjkmnpqrstvwxyz";
export const apiKeyScheme = "cvt_live_";
const prefixChars = 8;

function base32(bytes: Uint8Array): string {
  let bits = 0;
  let value = 0;
  let out = "";
  for (const b of bytes) {
    value = (value << 8) | b;
    bits += 8;
    while (bits >= 5) {
      out += alphabet[(value >>> (bits - 5)) & 31];
      bits -= 5;
    }
  }
  if (bits > 0) out += alphabet[(value << (5 - bits)) & 31];
  return out;
}

export function generateApiKey(random: (n: number) => Uint8Array = randomBytes): string {
  return apiKeyScheme + base32(random(20));
}

export function apiKeyPrefix(key: string): string {
  return key.slice(0, apiKeyScheme.length + prefixChars);
}

export async function hashApiKey(key: string): Promise<Uint8Array> {
  return new Uint8Array(await crypto.subtle.digest("SHA-256", new TextEncoder().encode(key)));
}

/** "cvt_live_8f3a2b1c••••" */
export function maskApiKey(prefix: string): string {
  return `${prefix}${"•".repeat(4)}`;
}
