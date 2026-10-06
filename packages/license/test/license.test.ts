import { describe, expect, test } from "bun:test";

import vectors from "../../../crates/convt-license/tests/vectors.json";
import ids from "../vectors/ids.json";
import {
  apiKeyPrefix,
  base64urlDecode,
  base64urlEncode,
  encodeId128,
  generateApiKey,
  hashApiKey,
  importSigningKey,
  importVerifyKey,
  isId,
  maskApiKey,
  newId,
  parseSeed,
  publicKeyOf,
  sign,
  verify,
  type License,
} from "../src";

const hex = (h: string) => Uint8Array.from(h.match(/../g)!.map((b) => parseInt(b, 16)));
const toHex = (b: Uint8Array) => [...b].map((x) => x.toString(16).padStart(2, "0")).join("");

describe("P0 vectors", async () => {
  const key = await importSigningKey(hex(vectors.seed_hex));

  test("public key matches the seed", async () => {
    expect(await publicKeyOf(key)).toBe(vectors.public_key_b64url);
  });

  for (const c of vectors.cases) {
    test(`sign reproduces and verify accepts: ${c.name}`, async () => {
      expect(await sign(c.license as License, key)).toBe(c.token);
      const vk = await importVerifyKey(vectors.public_key_b64url);
      expect(await verify(c.token, vk)).toEqual({ ok: true, license: c.license as License });
    });
  }

  test("a flipped signature byte is rejected", async () => {
    const [payload, sig] = vectors.cases[0].token.split(".");
    const bytes = base64urlDecode(sig)!;
    bytes[0] ^= 1;
    const vk = await importVerifyKey(vectors.public_key_b64url);
    expect(await verify(`${payload}.${base64urlEncode(bytes)}`, vk)).toEqual({
      ok: false,
      error: "bad_signature",
    });
  });

  test("a tampered payload and a wrong key are rejected", async () => {
    const vk = await importVerifyKey(vectors.public_key_b64url);
    const forged = base64urlEncode(
      new TextEncoder().encode(JSON.stringify({ ...vectors.cases[0].license, plan: "pro" })),
    );
    const sig = vectors.cases[0].token.split(".")[1];
    expect((await verify(`${forged}.${sig}`, vk)).ok).toBe(false);
    const other = await importSigningKey(new Uint8Array(32).fill(9));
    const otherVk = await importVerifyKey(await publicKeyOf(other));
    expect(await verify(vectors.cases[0].token, otherVk)).toEqual({
      ok: false,
      error: "bad_signature",
    });
  });

  test("malformed tokens and dates", async () => {
    const vk = await importVerifyKey(vectors.public_key_b64url);
    for (const bad of ["", "abc", "a.b.c", "!!.??"]) {
      expect((await verify(bad, vk)).ok).toBe(false);
    }
    await expect(
      sign({ ...(vectors.cases[0].license as License), updates_until: "2027-02-30" }, key),
    ).rejects.toThrow();
  });

  test("parseSeed reads the dev-keys file format", () => {
    const seed = hex(vectors.seed_hex);
    expect(parseSeed(`${base64urlEncode(seed)}\n`)).toEqual(seed);
    expect(() => parseSeed("short")).toThrow();
  });
});

describe("ids", () => {
  for (const c of ids.ids) {
    test(`vector ${c.name}`, () => {
      expect(encodeId128(hex(c.bytes_hex))).toBe(c.encoded);
    });
  }

  test("newId has the prefix and 26 characters", () => {
    const id = newId("usr");
    expect(id).toMatch(/^usr_[0-9a-z]{26}$/);
    expect(isId(id, "usr")).toBe(true);
    expect(isId(id, "lic")).toBe(false);
    expect(newId("usr")).not.toBe(id);
  });
});

describe("API keys", () => {
  test("hash vector", async () => {
    expect(toHex(await hashApiKey(ids.api_key.key))).toBe(ids.api_key.sha256_hex);
    expect(apiKeyPrefix(ids.api_key.key)).toBe(ids.api_key.prefix);
  });

  test("generated keys have the scheme and length", () => {
    const key = generateApiKey();
    expect(key).toMatch(/^cvt_live_[0-9a-z]{32}$/);
    expect(maskApiKey(apiKeyPrefix(key))).toBe(`${key.slice(0, 17)}••••`);
  });
});

test("the shared API key vector is a well-formed key", () => {
  expect(ids.api_key.key).toMatch(/^cvt_live_[0-9a-z]{32}$/);
});
