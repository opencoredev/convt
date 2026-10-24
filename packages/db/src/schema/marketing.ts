// Marketing email consent. One row per account says whether it receives campaign
// email and whether Sequenzy has caught up; the event log records every change
// and where it came from. Only set_marketing_consent and enroll_marketing (see
// sql/privileges.sql) change consent, so the two tables never disagree.
// convt-billing pushes rows to Sequenzy; the site reaches them through its RPC.

import { sql } from "drizzle-orm";
import { bigint, boolean, check, index, integer, pgTable, text } from "drizzle-orm/pg-core";

import { users } from "./auth";
import { inList, timestamps, tstz } from "./columns";

export const marketingStatuses = ["subscribed", "unsubscribed"] as const;
/**
 * `signup` and `backfill` enroll an account that has no row. `settings` and
 * `email_link` are the person's own choice. `provider` mirrors an unsubscribe,
 * complaint or bounce that Sequenzy reported.
 */
export const marketingSources = [
  "signup",
  "backfill",
  "settings",
  "email_link",
  "provider",
] as const;
/**
 * `pending`: waiting to be pushed. `held`: subscribed, but the email is not
 * verified yet. `synced`: Sequenzy matches. `failed`: Sequenzy refused it for
 * good; an alert says why.
 */
export const marketingSyncStates = ["pending", "held", "synced", "failed"] as const;

export const marketingSubscriptions = pgTable(
  "marketing_subscriptions",
  {
    userId: text()
      .primaryKey()
      .references(() => users.id, { onDelete: "cascade" }),
    status: text().notNull(),
    /** Where the current status came from. */
    source: text().notNull(),
    statusChangedAt: tstz().notNull().defaultNow(),
    /**
     * Set when the person subscribes again themselves. Only then may the push set
     * Sequenzy's status back to active; nothing else ever resubscribes anyone.
     */
    reactivate: boolean().notNull().default(false),
    syncState: text().notNull().default("pending"),
    syncAttempts: integer().notNull().default(0),
    nextSyncAt: tstz().notNull().defaultNow(),
    /**
     * Set while one push holds the row, so two pushes never race and land at
     * Sequenzy out of order. A change during the push leaves the row pending for
     * the next one.
     */
    syncLeaseUntil: tstz(),
    syncedAt: tstz(),
    /** Status code and error code only, never a response body. */
    lastError: text(),
    ...timestamps,
  },
  (t) => [
    check("marketing_subscriptions_status", inList(t.status, marketingStatuses)),
    check("marketing_subscriptions_source", inList(t.source, marketingSources)),
    check("marketing_subscriptions_sync_state", inList(t.syncState, marketingSyncStates)),
    index("marketing_subscriptions_due_idx")
      .on(t.nextSyncAt)
      .where(sql`${t.syncState} = 'pending'`),
  ],
);

/** Append-only. Removed with the account it belongs to. */
export const marketingConsentEvents = pgTable(
  "marketing_consent_events",
  {
    id: bigint({ mode: "number" }).primaryKey().generatedAlwaysAsIdentity(),
    userId: text()
      .notNull()
      .references(() => users.id, { onDelete: "cascade" }),
    status: text().notNull(),
    source: text().notNull(),
    /** For `provider`: the Sequenzy event type and id. Never an email address. */
    detail: text(),
    createdAt: tstz().notNull().defaultNow(),
  },
  (t) => [
    check("marketing_consent_events_status", inList(t.status, marketingStatuses)),
    check("marketing_consent_events_source", inList(t.source, marketingSources)),
    index("marketing_consent_events_user_id_idx").on(t.userId, t.createdAt),
  ],
);
