// bun run db:seed. Writes the fixture accounts (see src/seed.ts) into this
// checkout's own database. Safe to run again: it replaces the fixtures.
import { withDb } from "../client";
import { requireSetting } from "../env";
import { assertOwnedDatabase } from "../guard";
import { runSeed } from "../seed";

const url = requireSetting("OWNER_DATABASE_URL");
assertOwnedDatabase(url);
await withDb(url, async (db) => {
  console.log(`db:seed: ${await runSeed(db, new Date())}`);
});
