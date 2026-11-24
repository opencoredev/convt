import { createHash } from "node:crypto";
import { expect, test } from "bun:test";
import { Sha256 } from "../src/sha256";

test("matches node:crypto across block boundaries and chunkings", () => {
  for (const size of [0, 1, 55, 56, 63, 64, 65, 119, 120, 1000, 100_003]) {
    const data = new Uint8Array(size).map((_, i) => (i * 31 + 7) % 256);
    const expected = createHash("sha256").update(data).digest("hex");
    const whole = new Sha256().update(data).hex();
    const chunked = new Sha256();
    for (let i = 0; i < size; i += 37) chunked.update(data.subarray(i, i + 37));
    expect(whole).toBe(expected);
    expect(chunked.hex()).toBe(expected);
  }
});
