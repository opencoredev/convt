import { afterAll, beforeAll, expect, test } from "bun:test";
import { importSigningKey, hashApiKey } from "@convt/license";
import { freshDatabase, type TestDatabase } from "../../src/testing";
import { runSeed, fixtureId } from "../../src/seed";
import { createApiKey, revokeApiKey, cloudAllowance } from "../../src/queries/cloud";
let tdb: TestDatabase;
beforeAll(async () => {
  tdb = await freshDatabase();
  const { db } = await tdb.open("owner");
  await runSeed(db, new Date(), await importSigningKey(crypto.getRandomValues(new Uint8Array(32))));
});
afterAll(async () => {
  await tdb?.drop();
});
test("keys are hashed, owner checked, revocable and enrollment required", async () => {
  const { db } = await tdb.open("web");
  const { client } = await tdb.open("owner");
  const created = await createApiKey(db, fixtureId("usr", "pro"), "integration key");
  expect(created.key).toStartWith("cvt_live_");
  const row = (await client.query("select secret_hash from api_keys where id=$1", [created.id]))
    .rows[0];
  expect(new Uint8Array(row.secret_hash)).toEqual(await hashApiKey(created.key));
  expect(await revokeApiKey(db, fixtureId("usr", "api"), created.id)).toBe(false);
  expect(await revokeApiKey(db, fixtureId("usr", "pro"), created.id)).toBe(true);
  expect(await revokeApiKey(db, fixtureId("usr", "pro"), created.id)).toBe(false);
  await expect(createApiKey(db, fixtureId("usr", "new"), "refused")).rejects.toThrow(
    "payment method",
  );
});
test("annual Pro allowance is monthly and API cap exposes settled plus reserved", async () => {
  const { db } = await tdb.open("web");
  expect((await cloudAllowance(db, fixtureId("usr", "pro"), "pro")).allowed).toBe(true);
  expect((await cloudAllowance(db, fixtureId("usr", "pro"), "pro")).limit).toBe(50_000_000_000);
  expect((await cloudAllowance(db, fixtureId("usr", "trial"), "pro")).allowed).toBe(true);
  expect((await cloudAllowance(db, fixtureId("usr", "new"), "pro")).allowed).toBe(false);
  const api = await cloudAllowance(db, fixtureId("usr", "pro"), "api");
  expect(api.allowed).toBe(true);
  expect(api.limit).toBeGreaterThan(0);
  expect(api.used).toBeGreaterThan(0);
});
