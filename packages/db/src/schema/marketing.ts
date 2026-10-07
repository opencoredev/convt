// The launch mailing list: addresses a visitor gave on convt.app (the phone
// download-link card) with consent to launch email. One row per address. Every
// email's unsubscribe link carries the address's token (stable per address); its
// SHA-256 is stored here, and unsubscribing deletes the row.

import { sql } from "drizzle-orm";
import { check, pgTable, text, uniqueIndex } from "drizzle-orm/pg-core";

import { inList, tstz } from "./columns";

export const launchListSources = ["landing", "download", "checkout_success"] as const;

export const launchList = pgTable(
  "launch_list",
  {
    email: text().primaryKey(),
    /** Where the address was last given. */
    source: text().notNull(),
    /** SHA-256 hex of the address's unsubscribe token. */
    unsubscribeTokenHash: text().notNull(),
    consentedAt: tstz().notNull().defaultNow(),
    lastRequestedAt: tstz().notNull().defaultNow(),
  },
  (t) => [
    check("launch_list_email_normalized", sql`${t.email} = lower(btrim(${t.email}))`),
    check("launch_list_source_check", inList(t.source, launchListSources)),
    uniqueIndex("launch_list_unsubscribe_token_hash_key").on(t.unsubscribeTokenHash),
  ],
);
