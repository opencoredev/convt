// bun run db:migrate. Applies migrations as convt_owner: MIGRATE_DATABASE_URL if
// set (deploys), else this checkout's OWNER_DATABASE_URL from services.env.
import { withDb } from "../client";
import { requireSetting, servicesEnv } from "../env";
import { appliedMigrations, migrateUp } from "../migrations";

const url = servicesEnv().MIGRATE_DATABASE_URL ?? requireSetting("OWNER_DATABASE_URL");
await withDb(url, async (_db, client) => {
  const before = (await appliedMigrations(client)).length;
  await migrateUp(client);
  const after = (await appliedMigrations(client)).length;
  console.log(`db:migrate: ${after - before} applied, ${after} total`);
});
