// API keys, cloud jobs (P9's queue) and usage facts. `usage_events` is append-only:
// grants and triggers in the migration refuse edits and deletes (see sql/privileges.sql).

import { sql } from "drizzle-orm";
import {
  type AnyPgColumn,
  bigint,
  check,
  customType,
  index,
  integer,
  jsonb,
  pgTable,
  text,
  uniqueIndex,
} from "drizzle-orm/pg-core";

import { users } from "./auth";
import { subscriptions } from "./billing";
import { timestamps, tstz } from "./columns";

const bytea = customType<{ data: Uint8Array; driverData: Buffer }>({
  dataType: () => "bytea",
  toDriver: (value) => Buffer.from(value),
  fromDriver: (value) => new Uint8Array(value),
});

export const jobStatuses = [
  "created",
  "uploaded",
  "queued",
  "running",
  "succeeded",
  "failed",
  "cancelled",
] as const;

export const apiKeys = pgTable(
  "api_keys",
  {
    id: text().primaryKey(),
    userId: text()
      .notNull()
      .references(() => users.id, { onDelete: "cascade" }),
    name: text().notNull(),
    /** Display prefix, such as `cvt_live_8f3a2b1c`. */
    prefix: text().notNull(),
    /** SHA-256 of the whole key. */
    secretHash: bytea().notNull(),
    lastUsedAt: tstz(),
    revokedAt: tstz(),
    ...timestamps,
  },
  (t) => [
    uniqueIndex("api_keys_secret_hash_key").on(t.secretHash),
    uniqueIndex("api_keys_prefix_key").on(t.prefix),
    index("api_keys_user_id_active_idx")
      .on(t.userId)
      .where(sql`${t.revokedAt} is null`),
  ],
);

export const cloudJobs = pgTable(
  "cloud_jobs",
  {
    id: text().primaryKey(),
    userId: text()
      .notNull()
      .references(() => users.id, { onDelete: "cascade" }),
    source: text().notNull(),
    apiKeyId: text().references(() => apiKeys.id, { onDelete: "set null" }),
    status: text().notNull().default("created"),
    inputFormat: text().notNull(),
    targetFormat: text().notNull(),
    options: jsonb().notNull().default({}),
    inputKey: text(),
    inputBytes: bigint({ mode: "number" }),
    outputKeys: jsonb().notNull().default([]),
    /** Fencing token for P9's leases. */
    attempt: integer().notNull().default(0),
    maxAttempts: integer().notNull().default(3),
    leaseOwner: text(),
    leaseExpiresAt: tstz(),
    reservedBytes: bigint({ mode: "number" }).notNull().default(0),
    reservedCents: integer().notNull().default(0),
    reservation: text().notNull().default("open"),
    errorCode: text(),
    errorDetail: text(),
    queuedAt: tstz(),
    startedAt: tstz(),
    finishedAt: tstz(),
    expiresAt: tstz().notNull(),
    ...timestamps,
    subscriptionId: text().references(() => subscriptions.id, { onDelete: "set null" }),
    quotaPeriodStart: tstz(),
  },
  (t) => [
    index("cloud_jobs_queued_idx")
      .on(t.queuedAt)
      .where(sql`${t.status} = 'queued'`),
    index("cloud_jobs_lease_idx")
      .on(t.leaseExpiresAt)
      .where(sql`${t.status} = 'running'`),
    index("cloud_jobs_user_created_idx").on(t.userId, t.createdAt.desc()),
    index("cloud_jobs_expires_at_idx").on(t.expiresAt),
    index("cloud_jobs_subscription_open_idx")
      .on(t.subscriptionId)
      .where(sql`${t.reservation} = 'open'`),
    check("cloud_jobs_source_check", sql`${t.source} in ('api', 'web', 'desktop')`),
    check(
      "cloud_jobs_status_check",
      sql`${t.status} in (${sql.raw(jobStatuses.map((s) => `'${s}'`).join(", "))})`,
    ),
    check("cloud_jobs_reservation_check", sql`${t.reservation} in ('open', 'settled', 'released')`),
  ],
);

export const usageEvents = pgTable(
  "usage_events",
  {
    id: text().primaryKey(),
    userId: text().references(() => users.id, { onDelete: "set null" }),
    subscriptionId: text().references(() => subscriptions.id, { onDelete: "set null" }),
    apiKeyId: text().references(() => apiKeys.id, { onDelete: "set null" }),
    /** No foreign key: the billing record outlives the job row. */
    jobId: text(),
    kind: text().notNull(),
    quantity: bigint({ mode: "number" }).notNull(),
    amountCents: integer().notNull(),
    corrects: text().references((): AnyPgColumn => usageEvents.id),
    occurredAt: tstz().notNull(),
    reportedAt: tstz(),
    providerEventId: text(),
    createdAt: tstz().notNull().defaultNow(),
  },
  (t) => [
    uniqueIndex("usage_events_job_kind_key")
      .on(t.jobId, t.kind)
      .where(sql`${t.corrects} is null`),
    index("usage_events_user_occurred_idx").on(t.userId, t.occurredAt),
    index("usage_events_subscription_occurred_idx").on(t.subscriptionId, t.occurredAt),
    index("usage_events_unreported_idx")
      .on(t.occurredAt)
      .where(sql`${t.reportedAt} is null`),
    check(
      "usage_events_kind_check",
      sql`${t.kind} in ('api_conversion', 'pro_bytes', 'correction')`,
    ),
    check(
      "usage_events_correction_check",
      sql`(${t.kind} = 'correction') = (${t.corrects} is not null)`,
    ),
  ],
);
