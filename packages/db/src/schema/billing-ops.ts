// The tables convt-billing keeps around purchases: checkouts we created, the
// provider's customers, paid coverage and disputes, webhook deliveries, the email
// outbox, account deletions, reconciler bookkeeping and alerts. convt_web reads a
// few of them and writes none (see sql/privileges.sql).

import { sql } from "drizzle-orm";
import {
  bigint,
  boolean,
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
import { invoices, orders, subscriptions } from "./billing";
import { inList, timestamps, tstz, versionTs } from "./columns";

const bytea = customType<{ data: Uint8Array; driverData: Buffer }>({
  dataType: () => "bytea",
  toDriver: (value) => Buffer.from(value),
  fromDriver: (value) => new Uint8Array(value),
});

export const catalogProducts = ["desktop", "pro_month", "pro_year", "api"] as const;
export const checkoutStatuses = [
  "created",
  "open",
  "expired",
  "confirmed",
  "succeeded",
  "failed",
] as const;
export const disputeStatuses = [
  "prevented",
  "early_warning",
  "needs_response",
  "under_review",
  "lost",
  "won",
] as const;
export const webhookStatuses = ["processed", "ignored", "rejected", "failed", "dead"] as const;
export const emailKinds = [
  "license_issued",
  "trial_ending",
  "renewal_failed",
  "alert_digest",
] as const;
export const emailStatuses = [
  "pending",
  "sending",
  "sent",
  "skipped",
  "dead",
  "ambiguous",
] as const;
export const deletionStatuses = ["pending", "canceling", "deleting", "done", "failed"] as const;

export const checkouts = pgTable(
  "checkouts",
  {
    id: text().primaryKey(),
    provider: text().notNull().default("polar"),
    /** Null until the provider has answered; the provider's checkout carries our id in metadata. */
    providerCheckoutId: text(),
    userId: text().references(() => users.id, { onDelete: "set null" }),
    product: text().notNull(),
    allowTrial: boolean().notNull().default(false),
    /** API only: the cap the subscription gets when it is first ingested. */
    spendCapCents: integer(),
    /** SHA-256 of the nonce in the buyer's checkout cookie. */
    nonceHash: bytea().notNull(),
    nonceExpiresAt: tstz().notNull(),
    keyDisclosedAt: tstz(),
    status: text().notNull().default("created"),
    /** When the success page last asked us to sync this checkout from the provider. */
    syncedAt: tstz(),
    ...timestamps,
  },
  (t) => [
    uniqueIndex("checkouts_provider_checkout_key").on(t.provider, t.providerCheckoutId),
    index("checkouts_user_idx").on(t.userId),
    index("checkouts_open_idx")
      .on(t.createdAt)
      .where(sql`${t.status} in ('created', 'open')`),
    check("checkouts_product_check", inList(t.product, catalogProducts)),
    check("checkouts_status_check", inList(t.status, checkoutStatuses)),
    check(
      "checkouts_spend_cap_check",
      sql`(${t.product} = 'api' and ${t.spendCapCents} is not null and ${t.spendCapCents} > 0) or (${t.product} <> 'api' and ${t.spendCapCents} is null)`,
    ),
  ],
);

export const billingCustomers = pgTable(
  "billing_customers",
  {
    id: text().primaryKey(),
    userId: text().references(() => users.id, { onDelete: "set null" }),
    provider: text().notNull().default("polar"),
    providerCustomerId: text().notNull(),
    email: text().notNull(),
    deletedAt: tstz(),
    providerVersion: versionTs(),
    ...timestamps,
  },
  (t) => [
    uniqueIndex("billing_customers_provider_customer_key").on(t.provider, t.providerCustomerId),
    uniqueIndex("billing_customers_provider_user_key").on(t.provider, t.userId),
  ],
);

export const paymentCoverage = pgTable(
  "payment_coverage",
  {
    id: text().primaryKey(),
    invoiceId: text()
      .notNull()
      .references(() => invoices.id),
    providerItemId: text().notNull(),
    subscriptionId: text()
      .notNull()
      .references(() => subscriptions.id),
    /** `pro_month` or `pro_year`. */
    product: text().notNull(),
    priceId: text().notNull(),
    periodStart: tstz().notNull(),
    periodEnd: tstz().notNull(),
    /** Negative for a credit. */
    amountCents: integer().notNull(),
    kind: text().notNull(),
    createdAt: tstz().notNull().defaultNow(),
  },
  (t) => [
    uniqueIndex("payment_coverage_item_key").on(t.providerItemId),
    index("payment_coverage_subscription_idx").on(t.subscriptionId),
    index("payment_coverage_invoice_idx").on(t.invoiceId),
    check("payment_coverage_kind_check", inList(t.kind, ["period", "proration"])),
    check("payment_coverage_product_check", inList(t.product, ["pro_month", "pro_year"])),
    check("payment_coverage_period_check", sql`${t.periodEnd} > ${t.periodStart}`),
  ],
);

export const disputes = pgTable(
  "disputes",
  {
    id: text().primaryKey(),
    provider: text().notNull().default("polar"),
    providerDisputeId: text().notNull(),
    orderId: text().references(() => orders.id),
    invoiceId: text().references(() => invoices.id),
    status: text().notNull(),
    amountCents: integer().notNull(),
    closed: boolean().notNull().default(false),
    providerVersion: versionTs(),
    providerHash: text(),
    ...timestamps,
  },
  (t) => [
    uniqueIndex("disputes_provider_dispute_key").on(t.provider, t.providerDisputeId),
    index("disputes_order_idx").on(t.orderId),
    index("disputes_invoice_idx").on(t.invoiceId),
    index("disputes_open_idx")
      .on(t.updatedAt)
      .where(sql`${t.status} not in ('lost', 'won')`),
    check("disputes_status_check", inList(t.status, disputeStatuses)),
    check("disputes_subject_check", sql`num_nonnulls(${t.orderId}, ${t.invoiceId}) = 1`),
  ],
);

export const webhookEvents = pgTable(
  "webhook_events",
  {
    id: text().primaryKey(),
    provider: text().notNull().default("polar"),
    providerEventId: text().notNull(),
    type: text().notNull(),
    receivedAt: tstz().notNull().defaultNow(),
    /** The verified body. Nulled after 30 days. */
    body: text(),
    status: text().notNull(),
    reason: text(),
    attempts: integer().notNull().default(0),
    processedAt: tstz(),
    updatedAt: tstz().notNull().defaultNow(),
  },
  (t) => [
    uniqueIndex("webhook_events_provider_event_key").on(t.provider, t.providerEventId),
    index("webhook_events_status_idx")
      .on(t.status)
      .where(sql`${t.status} in ('failed', 'rejected', 'dead')`),
    index("webhook_events_received_idx").on(t.receivedAt),
    check("webhook_events_status_check", inList(t.status, webhookStatuses)),
  ],
);

export const emailOutbox = pgTable(
  "email_outbox",
  {
    id: text().primaryKey(),
    kind: text().notNull(),
    dedupeKey: text().notNull(),
    toEmail: text().notNull(),
    userId: text().references(() => users.id, { onDelete: "set null" }),
    /** The license, subscription or date the email is about. */
    subjectId: text().notNull(),
    status: text().notNull().default("pending"),
    claimGeneration: integer().notNull().default(0),
    lockedUntil: tstz(),
    nextAttemptAt: tstz().notNull().defaultNow(),
    attempts: integer().notNull().default(0),
    firstAttemptAt: tstz(),
    lastAttemptAt: tstz(),
    /** When an attempt last ended without knowing whether Resend accepted it. */
    unknownOutcomeAt: tstz(),
    templateVersion: integer(),
    /** The exact request, frozen on the first claim; nulled 25 hours after a final status. */
    payload: jsonb(),
    payloadSha256: text(),
    providerMessageId: text(),
    lastError: text(),
    sentAt: tstz(),
    finishedAt: tstz(),
    ...timestamps,
  },
  (t) => [
    uniqueIndex("email_outbox_dedupe_key").on(t.dedupeKey),
    index("email_outbox_due_idx")
      .on(t.nextAttemptAt)
      .where(sql`${t.status} in ('pending', 'sending')`),
    index("email_outbox_user_idx").on(t.userId),
    check("email_outbox_kind_check", inList(t.kind, emailKinds)),
    check("email_outbox_status_check", inList(t.status, emailStatuses)),
  ],
);

export const accountDeletions = pgTable(
  "account_deletions",
  {
    id: text().primaryKey(),
    userId: text().references(() => users.id, { onDelete: "set null" }),
    status: text().notNull().default("pending"),
    attempts: integer().notNull().default(0),
    nextAttemptAt: tstz().notNull().defaultNow(),
    lastError: text(),
    alertedAt: tstz(),
    finishedAt: tstz(),
    ...timestamps,
  },
  (t) => [
    uniqueIndex("account_deletions_open_key")
      .on(t.userId)
      .where(sql`${t.status} <> 'done'`),
    index("account_deletions_due_idx")
      .on(t.nextAttemptAt)
      .where(sql`${t.status} not in ('done')`),
    check("account_deletions_status_check", inList(t.status, deletionStatuses)),
  ],
);

export const reconcileRuns = pgTable(
  "reconcile_runs",
  {
    id: text().primaryKey(),
    kind: text().notNull(),
    startedAt: tstz().notNull(),
    finishedAt: tstz(),
    status: text().notNull().default("running"),
    summary: jsonb().notNull().default({}),
  },
  (t) => [
    index("reconcile_runs_kind_started_idx").on(t.kind, t.startedAt.desc()),
    check("reconcile_runs_kind_check", inList(t.kind, ["frequent", "daily"])),
    check("reconcile_runs_status_check", inList(t.status, ["running", "ok", "failed"])),
  ],
);

export const reconcileCursors = pgTable("reconcile_cursors", {
  name: text().primaryKey(),
  page: bigint({ mode: "number" }).notNull().default(1),
  passStartedAt: tstz().notNull().defaultNow(),
  updatedAt: tstz().notNull().defaultNow(),
});

/** Things Leo should look at; the daily digest sends the undigested ones. */
export const billingAlerts = pgTable(
  "billing_alerts",
  {
    id: text().primaryKey(),
    kind: text().notNull(),
    /** The order, subscription, event or outbox id it is about. No keys or bodies. */
    subject: text().notNull(),
    detail: text().notNull(),
    createdAt: tstz().notNull().defaultNow(),
    digestedAt: tstz(),
  },
  (t) => [
    uniqueIndex("billing_alerts_kind_subject_key").on(t.kind, t.subject),
    index("billing_alerts_undigested_idx")
      .on(t.createdAt)
      .where(sql`${t.digestedAt} is null`),
  ],
);
