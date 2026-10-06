// bun run db:rollback. Reverses one migration on this checkout's own database.
import { withDb } from "../client";
import { requireSetting } from "../env";
import { assertOwnedDatabase } from "../guard";
import { rollbackOne } from "../migrations";

const url = requireSetting("OWNER_DATABASE_URL");
assertOwnedDatabase(url);
await withDb(url, async (_db, client) => {
  const tag = await rollbackOne(client);
  console.log(tag ? `db:rollback: reversed ${tag}` : "db:rollback: nothing to reverse");
});
