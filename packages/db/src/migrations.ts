// Applying and reversing migrations. Drizzle's migrator records each migration in
// drizzle.__drizzle_migrations as (sha256 of the SQL file, journal `when`) and
// applies anything newer than the last row; it never compares hashes, so rollback
// deletes the row it reverses, and convt-server checks the hashes at startup.

import { createHash } from "node:crypto";
import { existsSync, readFileSync } from "node:fs";
import { join } from "node:path";

import { migrate } from "drizzle-orm/node-postgres/migrator";
import type pg from "pg";

import { createDb } from "./client";

export const migrationsFolder = join(import.meta.dir, "..", "migrations");

export type JournalEntry = { idx: number; when: number; tag: string };

export function journal(): JournalEntry[] {
  const text = readFileSync(join(migrationsFolder, "meta", "_journal.json"), "utf8");
  return (JSON.parse(text) as { entries: JournalEntry[] }).entries;
}

/** (when, sha256) for each migration in the repo, as convt-server embeds them. */
export function migrationHashes(): Array<{ tag: string; when: number; hash: string }> {
  return journal().map((e) => ({
    tag: e.tag,
    when: e.when,
    hash: createHash("sha256")
      .update(readFileSync(join(migrationsFolder, `${e.tag}.sql`), "utf8"))
      .digest("hex"),
  }));
}

export async function migrateUp(client: pg.Client): Promise<void> {
  await migrate(createDb(client), {
    migrationsFolder,
    migrationsSchema: "drizzle",
    migrationsTable: "__drizzle_migrations",
  });
}

export async function appliedMigrations(
  client: pg.Client,
): Promise<Array<{ hash: string; when: number }>> {
  const exists = await client.query(`select to_regclass('drizzle.__drizzle_migrations') as t`);
  if (!exists.rows[0].t) return [];
  const rows = await client.query(
    `select hash, created_at from drizzle.__drizzle_migrations order by created_at`,
  );
  return rows.rows.map((r) => ({ hash: r.hash as string, when: Number(r.created_at) }));
}

/**
 * Reverses the newest applied migration with its file in migrations/down, and
 * deletes its row, in one transaction. Returns the tag, or null when nothing is
 * applied.
 */
export async function rollbackOne(client: pg.Client): Promise<string | null> {
  const applied = await appliedMigrations(client);
  const last = applied.at(-1);
  if (!last) return null;
  const entry = journal().find((e) => e.when === last.when);
  if (!entry) throw new Error(`applied migration ${last.when} is not in the journal`);
  const downFile = join(migrationsFolder, "down", `${entry.tag}.sql`);
  if (!existsSync(downFile)) throw new Error(`no down file for ${entry.tag}`);
  await client.query("begin");
  try {
    await client.query(readFileSync(downFile, "utf8"));
    await client.query(`delete from drizzle.__drizzle_migrations where created_at = $1`, [
      last.when,
    ]);
    await client.query("commit");
  } catch (e) {
    await client.query("rollback");
    throw e;
  }
  return entry.tag;
}
