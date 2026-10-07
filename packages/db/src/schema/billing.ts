// Purchases and entitlements. `user_id` is nullable because a purchase can come
// before the account; `claim_purchases` attaches rows by verified email. These rows
// are never deleted: deleting a user sets `user_id` to null and keeps the email.
// Only convt_billing writes them (see docs/p7-billing-plan.md, section 7).

import { sql } from "drizzle-orm";
import {
  type AnyPgColumn,
  boolean,
  check,
  date,
  index,
  integer,
  jsonb,
  pgTable,
  text,
  uniqueIndex,
} from "drizzle-orm/pg-core";

import { users } from "./auth";
import { checkouts } from "./billing-ops";
import { inList, timestamps, tstz, versionTs } from "./columns";

export const orderStatuses = ["pending", "paid", "partially_refunded", "refunded", "void"] as const;
/** Polar's eight subscription statuses, stored as they are. */
export const subscriptionStatuses = [
  "incomplete",
  "incomplete_expired",
  "trialing",
  "active",
  "past_due",
  "canceled",
  "unpaid",
  "paused",
] as const;
export const invoiceStatuses = [
  "draft",
  "open",
  "paid",
  "partially_refunded",
  "void",
  "uncollectible",
  "refunded",
] as const;
export const revokeReasons = ["refunded", "dispute_lost"] as const;

/** Statuses that mean money was taken, so `paid_at` is set. */
const paidStatuses = sql.raw(`'paid', 'partially_refunded', 'refunded'`);

export const orders = pgTable(
  "orders",
  {
    id: text().primaryKey(),
    /** Payment provider, `polar` by default. */
    provider: text().notNull().default("polar"),
    providerOrderId: text().notNull(),
    providerCustomerId: text(),
    userId: text().references(() => users.id, { onDelete: "set null" }),
    email: text().notNull(),
    product: text().notNull(),
    amountCents: integer().notNull(),
    currency: text().notNull(),
    status: text().notNull(),
    /** When we first stored the order as paid; set once, never moved by reconciliation. */
    paidAt: tstz(),
    refundedAt: tstz(),
    ...timestamps,
    // Added by migration 0001, so they come after the P6 columns.
    /** The provider's `created_at`, written once. Only this feeds a license date. */
    billedAt: tstz().notNull(),
    refundedCents: integer().notNull().default(0),
    checkoutId: text().references(() => checkouts.id),
    providerVersion: versionTs(),
    providerHash: text(),
  },
  (t) => [
    uniqueIndex("orders_provider_order_key").on(t.provider, t.providerOrderId),
    index("orders_user_id_idx").on(t.userId),
    index("orders_unclaimed_email_idx")
      .on(t.email)
      .where(sql`${t.userId} is null`),
    check("orders_status_check", inList(t.status, orderStatuses)),
    check("orders_product_check", inList(t.product, ["desktop"])),
    check(
      "orders_paid_at_check",
      sql`${t.status} not in (${paidStatuses}) or ${t.paidAt} is not null`,
    ),
    check("orders_refunded_cents_check", sql`${t.refundedCents} >= 0`),
    check("orders_email_normalized", sql`${t.email} = lower(btrim(${t.email}))`),
  ],
);

export const subscriptions = pgTable(
  "subscriptions",
  {
    id: text().primaryKey(),
    provider: text().notNull().default("polar"),
    providerSubscriptionId: text().notNull(),
    providerCustomerId: text(),
    userId: text().references(() => users.id, { onDelete: "set null" }),
    email: text().notNull(),
    /** `pro` or `api`. */
    kind: text().notNull(),
    /** `month` or `year`; null for API. */
    interval: text(),
    status: text().notNull(),
    trialEndsAt: tstz(),
    currentPeriodStart: tstz(),
    currentPeriodEnd: tstz(),
    cancelAtPeriodEnd: boolean().notNull().default(false),
    canceledAt: tstz(),
    endedAt: tstz(),
    /** API only, and required for API: the monthly spend cap. */
    spendCapCents: integer(),
    ...timestamps,
    // Added by migration 0001.
    /** The checkout we created that started this subscription. */
    checkoutId: text().references(() => checkouts.id),
    /** The provider's `modified_at` (or `created_at`) of the state stored here. */
    providerVersion: versionTs(),
    providerHash: text(),
    pendingUpdate: jsonb(),
    /** API: when ingest first saw the subscription active with a card on file. */
    cardSeenAt: tstz(),
  },
  (t) => [
    uniqueIndex("subscriptions_provider_subscription_key").on(t.provider, t.providerSubscriptionId),
    index("subscriptions_user_id_idx").on(t.userId),
    index("subscriptions_unclaimed_email_idx")
      .on(t.email)
      .where(sql`${t.userId} is null`),
    check("subscriptions_kind_check", inList(t.kind, ["pro", "api"])),
    check("subscriptions_status_check", inList(t.status, subscriptionStatuses)),
    check(
      "subscriptions_interval_check",
      // `in` alone passes a NULL interval (CHECK treats NULL as true), hence the explicit test.
      sql`(${t.kind} = 'pro' and ${t.interval} is not null and ${t.interval} in ('month', 'year')) or (${t.kind} = 'api' and ${t.interval} is null)`,
    ),
    check("subscriptions_email_normalized", sql`${t.email} = lower(btrim(${t.email}))`),
    check(
      "subscriptions_spend_cap_check",
      sql`(${t.kind} = 'pro' and ${t.spendCapCents} is null) or (${t.kind} = 'api' and ${t.spendCapCents} is not null and ${t.spendCapCents} > 0)`,
    ),
    index("subscriptions_status_idx").on(t.status),
  ],
);

export const licenses = pgTable(
  "licenses",
  {
    /** Also the `id` inside the signed token. */
    id: text().primaryKey(),
    userId: text().references(() => users.id, { onDelete: "set null" }),
    /** The email as signed into the token. */
    email: text().notNull(),
    plan: text().notNull(),
    trial: boolean().notNull().default(false),
    orderId: text().references(() => orders.id),
    subscriptionId: text().references(() => subscriptions.id),
    /** Pro: the start of the billing period this key covers. */
    periodStart: date({ mode: "string" }),
    issuedOn: date({ mode: "string" }).notNull(),
    updatesUntil: date({ mode: "string" }).notNull(),
    token: text().notNull(),
    reissueOf: text().references((): AnyPgColumn => licenses.id),
    revokedAt: tstz(),
    revokeReason: text(),
    ...timestamps,
    // Added by migration 0001.
    /** Pro: the invoice whose paid coverage funded this key. */
    invoiceId: text().references((): AnyPgColumn => invoices.id),
  },
  (t) => [
    index("licenses_user_id_idx").on(t.userId),
    index("licenses_unclaimed_email_idx")
      .on(t.email)
      .where(sql`${t.userId} is null`),
    // The business facts issuance relies on, whatever the webhook event: one Desktop
    // key per order; for Pro, one key per invoice and one per paid-through date. A
    // support reissue sets `reissue_of`.
    uniqueIndex("licenses_desktop_order_key")
      .on(t.orderId)
      .where(sql`${t.plan} = 'desktop' and ${t.reissueOf} is null`),
    uniqueIndex("licenses_pro_period_until_key")
      .on(t.subscriptionId, t.periodStart, t.updatesUntil)
      .where(sql`${t.plan} = 'pro' and ${t.reissueOf} is null`),
    uniqueIndex("licenses_pro_invoice_key")
      .on(t.invoiceId)
      .where(sql`${t.plan} = 'pro' and ${t.reissueOf} is null`),
    check(
      "licenses_revoke_check",
      sql`(${t.revokedAt} is null and ${t.revokeReason} is null) or (${t.revokedAt} is not null and ${t.revokeReason} in ('refunded', 'dispute_lost'))`,
    ),
    check("licenses_plan_check", inList(t.plan, ["desktop", "pro"])),
    check("licenses_email_normalized", sql`${t.email} = lower(btrim(${t.email}))`),
    check(
      "licenses_source_check",
      sql`(${t.plan} = 'desktop' and ${t.orderId} is not null and ${t.subscriptionId} is null) or (${t.plan} = 'pro' and ${t.subscriptionId} is not null and ${t.periodStart} is not null)`,
    ),
  ],
);

export const invoices = pgTable(
  "invoices",
  {
    id: text().primaryKey(),
    provider: text().notNull().default("polar"),
    providerInvoiceId: text().notNull(),
    userId: text().references(() => users.id, { onDelete: "set null" }),
    email: text().notNull(),
    subscriptionId: text().references(() => subscriptions.id),
    orderId: text().references(() => orders.id),
    description: text().notNull(),
    amountCents: integer().notNull(),
    currency: text().notNull(),
    status: text().notNull(),
    issuedAt: tstz().notNull(),
    receiptUrl: text(),
    ...timestamps,
    // Added by migration 0001.
    /** The provider's billing reason, such as `subscription_cycle`. */
    reason: text(),
    /** The provider's `created_at`, written once. */
    billedAt: tstz().notNull(),
    /** When we first stored it as paid; set once. */
    paidAt: tstz(),
    netCents: integer().notNull().default(0),
    appliedBalanceCents: integer().notNull().default(0),
    refundedCents: integer().notNull().default(0),
    providerVersion: versionTs(),
    providerHash: text(),
  },
  (t) => [
    uniqueIndex("invoices_provider_invoice_key").on(t.provider, t.providerInvoiceId),
    index("invoices_user_issued_idx").on(t.userId, t.issuedAt.desc()),
    index("invoices_unclaimed_email_idx")
      .on(t.email)
      .where(sql`${t.userId} is null`),
    check("invoices_status_check", inList(t.status, invoiceStatuses)),
    check(
      "invoices_paid_at_check",
      sql`${t.status} not in (${paidStatuses}) or ${t.paidAt} is not null`,
    ),
    check("invoices_refunded_cents_check", sql`${t.refundedCents} >= 0`),
    index("invoices_subscription_idx").on(t.subscriptionId),
    check("invoices_email_normalized", sql`${t.email} = lower(btrim(${t.email}))`),
  ],
);

/**
 * The free desktop trial (CNV-56): at most one per account and one per computer.
 * convt-billing creates it and signs a trial token from it on every request, so no
 * token or email is stored. Deleting the account sets `user_id` to null and keeps
 * the row, so the computer cannot start a second trial under a new account.
 */
export const trials = pgTable(
  "trials",
  {
    /** Also the `id` inside the signed trial token. */
    id: text().primaryKey(),
    userId: text().references(() => users.id, { onDelete: "set null" }),
    /** HMAC-SHA256 (hex) of the app's device hash under the site's DEVICE_HASH_SECRET. */
    deviceHash: text(),
    startedAt: tstz().notNull(),
    /** Midnight UTC after the trial's last day: the start day plus seven days. */
    endsAt: tstz().notNull(),
    ...timestamps,
  },
  (t) => [
    uniqueIndex("trials_user_id_key").on(t.userId),
    uniqueIndex("trials_device_hash_key").on(t.deviceHash),
    check("trials_ends_after_start", sql`${t.endsAt} > ${t.startedAt}`),
    check("trials_device_hash_format", sql`${t.deviceHash} ~ '^[0-9a-f]{64}$'`),
  ],
);
