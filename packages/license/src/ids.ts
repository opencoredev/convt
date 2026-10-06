// Primary keys: a type prefix and 128 random bits in lowercase Crockford base32,
// 26 characters, most significant bits first. convt-server implements the same
// spec; both check vectors/ids.json.

const alphabet = "0123456789abcdefghjkmnpqrstvwxyz";

export const idPrefixes = [
  "usr",
  "ses",
  "acc",
  "ver",
  "rl",
  "dev",
  "ord",
  "sub",
  "inv",
  "lic",
  "key",
  "job",
  "use",
  "chk",
  "cus",
  "whe",
  "eml",
  "rcn",
  "cov",
  "dsp",
  "del",
  "alr",
] as const;

export type IdPrefix = (typeof idPrefixes)[number];

/** Crockford base32 of 16 bytes, read as one 128-bit big-endian number. */
export function encodeId128(bytes: Uint8Array): string {
  if (bytes.length !== 16) throw new Error("an id is 16 bytes");
  let n = 0n;
  for (const b of bytes) n = (n << 8n) | BigInt(b);
  let out = "";
  for (let i = 0; i < 26; i++) {
    out = alphabet[Number(n & 31n)] + out;
    n >>= 5n;
  }
  return out;
}

export function newId(prefix: IdPrefix, random: (n: number) => Uint8Array = randomBytes): string {
  return `${prefix}_${encodeId128(random(16))}`;
}

export function randomBytes(n: number): Uint8Array {
  return crypto.getRandomValues(new Uint8Array(n));
}

const idPattern = /^[a-z]+_[0-9a-hjkmnp-tv-z]{26}$/;

export function isId(text: string, prefix?: IdPrefix): boolean {
  return idPattern.test(text) && (!prefix || text.startsWith(`${prefix}_`));
}
