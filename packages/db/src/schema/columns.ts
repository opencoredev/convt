import { sql } from "drizzle-orm";
import { type AnyPgColumn, timestamp } from "drizzle-orm/pg-core";

/** `timestamptz` read and written as a Date. */
export const tstz = () => timestamp({ withTimezone: true, mode: "date" });

export const timestamps = {
  createdAt: tstz().notNull().defaultNow(),
  updatedAt: tstz().notNull().defaultNow(),
};

/**
 * A provider's version timestamp, kept as text in and out so its microseconds
 * survive (a JS Date has milliseconds). Compare versions in SQL.
 */
export const versionTs = () => timestamp({ withTimezone: true, mode: "string" });

/** A check that `column` is one of `values` (constant strings only). */
export const inList = (column: AnyPgColumn, values: readonly string[]) =>
  sql`${column} in (${sql.raw(values.map((v) => `'${v}'`).join(", "))})`;
