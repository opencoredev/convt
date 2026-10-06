import { join } from "node:path";

import { repoRoot } from "./env";

/**
 * Refuses unless `url` is this checkout's own local database: the published port of
 * the container named for this checkout, the container id and label recorded by
 * `db.sh up`, and the convt_dev.owner marker inside the database. One
 * implementation, in scripts/db.sh, serves the shell and Bun tools.
 */
export function assertOwnedDatabase(url: string): void {
  const result = Bun.spawnSync(["bash", join(repoRoot, "scripts", "db.sh"), "guard", url], {
    stdout: "pipe",
    stderr: "pipe",
  });
  if (result.exitCode !== 0) {
    throw new Error(result.stderr.toString().trim() || "db.sh guard refused this database");
  }
}
