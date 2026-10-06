// bun run db:reset. Reverses every migration, applies them again and seeds, on this
// checkout's own database only.
import { withDb } from "../client";
import { requireSetting } from "../env";
import { assertOwnedDatabase } from "../guard";
import { migrateUp, rollbackOne } from "../migrations";
import { runSeed } from "../seed";

const url = requireSetting("OWNER_DATABASE_URL");
assertOwnedDatabase(url);
await withDb(url, async (db, client) => {
  let tag: string | null;
  while ((tag = await rollbackOne(client))) console.log(`db:reset: reversed ${tag}`);
  await migrateUp(client);
  console.log("db:reset: migrated");
  const summary = await runSeed(db, new Date());
  console.log(`db:reset: seeded ${summary}`);
});
