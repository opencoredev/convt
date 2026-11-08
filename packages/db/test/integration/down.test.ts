// Down files must reverse a database that has data, not only an empty one:
// db:reset runs them on a developer's database after real local purchases.

import { afterAll, expect, test } from "bun:test";
import { importSigningKey } from "@convt/license";
import { sql } from "drizzle-orm";

import { migrateUp, rollbackOne } from "../../src/migrations";
import { fixtureId, runSeed } from "../../src/seed";
import { freshDatabase, type TestDatabase } from "../../src/testing";

let tdb: TestDatabase | undefined;
afterAll(async () => tdb?.drop());

test("0008 reverses with a Desktop + Cloud order, its cloud year and usage on it", async () => {
  tdb = await freshDatabase();
  const { db, client } = await tdb.open("owner");
  // The seed's bundle@ fixture: a bundle order and its prepaid cloud year.
  await runSeed(db, new Date(), await importSigningKey(crypto.getRandomValues(new Uint8Array(32))));
  const cloudYear = fixtureId("sub", "bundle-cloud");
  // Usage is append-only and references the year, so the rows cannot be deleted.
  await db.execute(sql`
    insert into usage_events (id, user_id, subscription_id, job_id, kind, quantity, amount_cents, occurred_at)
    values ('use_down', ${fixtureId("usr", "bundle")}, ${cloudYear}, 'job_down', 'pro_bytes', 1, 0, now())`);
  expect(await rollbackOne(client)).toBe("0008_prepaid_cloud");
  const [row] = (
    await db.execute<{ provider_subscription_id: string }>(
      sql`select provider_subscription_id from subscriptions where id = ${cloudYear}`,
    )
  ).rows;
  expect(row.provider_subscription_id).toStartWith("prepaid:");
  // And forward again.
  await migrateUp(client);
});
