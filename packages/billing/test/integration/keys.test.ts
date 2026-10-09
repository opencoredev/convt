// Keys issued on 29 February verify with the Rust verifier and end on 28 February.

import { afterAll, beforeAll, expect, test } from "bun:test";
import { sql } from "drizzle-orm";

import { createHarness, type Harness } from "../../src/testing";
import { verifyWithCli } from "../cli";

let h: Harness;
beforeAll(async () => {
  h = await createHarness({ startMs: Date.UTC(2028, 1, 29, 15, 30) });
});
afterAll(async () => h?.close());

test("a Desktop key bought on 29 February 2028 is lifetime", async () => {
  await h.buy("desktop", null, { email: "leap@convt.test" });
  await h.deliverAll();
  const [lic] = await h.q<{ token: string; issued_on: string; updates_until: string }>(sql`
    select token, issued_on::text, updates_until::text from licenses where email = 'leap@convt.test'`);
  expect(lic.issued_on).toBe("2028-02-29");
  expect(lic.updates_until).toBe("9999-12-31");
  const out = await verifyWithCli(lic.token, h.publicKey);
  expect(out.status).toBe(0);
  expect(out.text).toMatch(/9999-12-31|Dec 31, 9999/);
}, 600_000);
