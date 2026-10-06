// Ingest: one verified delivery (or one reconciler fetch) in one transaction as
// convt_billing. Facts are checked against our records, applied under the version
// rules, and converged into licenses, revocations and outbox rows, each insert
// guarded by a unique business key. See docs/p7-billing-plan.md, section 2.

import { sql } from "drizzle-orm";

import { isPro, type CatalogProduct } from "./catalog";
import { alert, type BillingContext, lockKeys, one, type Q, rows } from "./context";
import { convergeDesktop, convergePro, convergeApi } from "./converge";
import { emptyFacts } from "./provider";
import type {
  CheckoutFact,
  CustomerFact,
  DisputeFact,
  OrderFact,
  ProviderFacts,
  SubscriptionFact,
} from "./provider";
import { newId } from "@convt/license";

export type IngestOutcome =
  | { rejected: string }
  | { rejected: null; notes: string[]; userIds: string[] };

export class BudgetExceeded extends Error {
  constructor() {
    super("the ingest budget was exceeded");
    this.name = "BudgetExceeded";
  }
}

const paidStatuses = new Set(["paid", "partially_refunded", "refunded"]);
const terminalOrder = new Set(["refunded", "void"]);
const endedSubscription = (s: { status: string; ended_at: Date | null }) =>
  s.status === "canceled" || s.status === "incomplete_expired" || s.ended_at !== null;

const invoiceStatus: Record<string, string> = {
  draft: "draft",
  pending: "open",
  paid: "paid",
  partially_refunded: "partially_refunded",
  refunded: "refunded",
  void: "void",
};

/**
 * Fetches what the facts only hint at (a refund's order, a dispute's order) and
 * whether an active API subscription has a card. Runs before the locks.
 */
export async function hydrate(ctx: BillingContext, facts: ProviderFacts, deadline: number) {
  const seenOrders = new Set(facts.orders.map((o) => o.providerOrderId));
  const fetchOrder = async (id: string) => {
    if (seenOrders.has(id)) return;
    seenOrders.add(id);
    facts.orders.push(await ctx.provider.getOrder(id));
  };
  for (const h of facts.hints) {
    if (h.kind === "order") await fetchOrder(h.id);
    else if (h.kind === "subscription")
      facts.subscriptions.push(await ctx.provider.getSubscription(h.id));
    else if (h.kind === "dispute") facts.disputes.push(await ctx.provider.getDispute(h.id));
    checkDeadline(deadline);
  }
  facts.hints = facts.hints.filter((h) => h.kind === "settings");
  for (const d of facts.disputes) {
    const known = await one(
      ctx.db,
      sql`
      select 1 as x from orders where provider = 'polar' and provider_order_id = ${d.providerOrderId}
      union all select 1 from invoices where provider = 'polar' and provider_invoice_id = ${d.providerOrderId}`,
    );
    if (!known) await fetchOrder(d.providerOrderId);
  }
  const cards = new Map<string, boolean>();
  for (const s of allSubscriptions(facts)) {
    if (s.product !== "api" || s.status !== "active" || !s.userId || cards.has(s.userId)) continue;
    cards.set(s.userId, (await ctx.provider.paymentMethods(s.userId)).length > 0);
    checkDeadline(deadline);
  }
  return { cards };
}

export function checkDeadline(deadline: number) {
  if (Date.now() > deadline) throw new BudgetExceeded();
}

/** Standalone subscription facts, then the ones embedded in orders. */
function allSubscriptions(facts: ProviderFacts): SubscriptionFact[] {
  const out = [...facts.subscriptions];
  for (const o of facts.orders) if (o.subscription) out.push(o.subscription);
  return out;
}

type CheckoutRow = {
  id: string;
  provider_checkout_id: string | null;
  user_id: string | null;
  product: string;
  spend_cap_cents: number | null;
  status: string;
};

async function resolveCheckout(q: Q, providerCheckoutId: string | null, ref: string | null) {
  if (!providerCheckoutId && !ref) return null;
  return one<CheckoutRow>(
    q,
    sql`select id, provider_checkout_id, user_id, product, spend_cap_cents, status from checkouts
        where (provider = 'polar' and provider_checkout_id = ${providerCheckoutId})
           or (id = ${ref} and (provider_checkout_id is null or provider_checkout_id = ${providerCheckoutId}))
        order by (provider_checkout_id is not null) desc limit 1`,
  );
}

async function existingUser(q: Q, userId: string | null): Promise<string | null> {
  if (!userId) return null;
  const row = await one<{ id: string }>(q, sql`select id from users where id = ${userId}`);
  return row?.id ?? null;
}

// ------------------------------------------------------------------ checks

/** The business checks of section 2. Returns the first failure, or null. */
export async function checkFacts(
  ctx: BillingContext,
  tx: Q,
  facts: ProviderFacts,
): Promise<string | null> {
  const { catalog } = ctx;
  const prices = new Map(
    Object.entries(catalog.products).map(([k, v]) => [v.priceId, k as CatalogProduct]),
  );
  const yearly = catalog.products.pro_year.amountCents!;

  const checkCheckout = async (
    kind: string,
    product: CatalogProduct,
    providerCheckoutId: string | null,
    ref: string | null,
    userId: string | null,
  ) => {
    const co = await resolveCheckout(tx, providerCheckoutId, ref);
    if (!co) return `foreign_checkout: ${kind} names no checkout we created`;
    if (co.user_id && co.user_id !== userId)
      return `checkout_user: ${kind} belongs to another user's checkout`;
    // A switch can change a subscription between Pro products before its first
    // snapshot arrives, so a subscription only needs the same kind of checkout.
    const sameKind =
      kind === "subscription" && isPro(product) && isPro(co.product as CatalogProduct);
    if (co.product !== product && !sameKind)
      return `checkout_product: ${kind} is ${product}, the checkout was ${co.product}`;
    if (product === "api" && co.spend_cap_cents === null)
      return "checkout_cap: API checkout without a cap";
    return null;
  };

  const customerPairs: Array<[string | null, string]> = [];

  for (const s of allSubscriptions(facts)) {
    if (!s.product) return `unknown_product: subscription ${s.providerSubscriptionId}`;
    if (s.currency !== catalog.currency) return `currency: ${s.currency}`;
    if (s.discountId) return "discount: a subscription with a discount";
    const stored = await one<{
      user_id: string | null;
      provider_customer_id: string | null;
      kind: string;
    }>(
      tx,
      sql`select user_id, provider_customer_id, kind from subscriptions where provider = 'polar' and provider_subscription_id = ${s.providerSubscriptionId}`,
    );
    if (stored) {
      if (stored.user_id && s.userId && stored.user_id !== s.userId)
        return `subscription_user_changed: ${s.providerSubscriptionId}`;
      if (stored.provider_customer_id && stored.provider_customer_id !== s.providerCustomerId)
        return `customer_changed: subscription ${s.providerSubscriptionId}`;
      if (stored.kind !== (s.product === "api" ? "api" : "pro"))
        return `kind_changed: ${s.providerSubscriptionId}`;
    } else {
      const r = await checkCheckout(
        "subscription",
        s.product,
        s.providerCheckoutId,
        s.checkoutRef,
        s.userId,
      );
      if (r) return r;
    }
    customerPairs.push([s.userId, s.providerCustomerId]);
  }

  for (const o of facts.orders) {
    if (o.currency !== catalog.currency) return `currency: ${o.currency}`;
    if (o.discountId || o.discountCents !== 0) return "discount: an order with a discount";
    if (!o.product) return `unknown_product: order ${o.providerOrderId}`;
    for (const i of o.items) {
      if (i.priceId !== null && !prices.has(i.priceId)) return `unknown_price: ${i.priceId}`;
    }
    if (!o.email) return `no_email: order ${o.providerOrderId}`;
    if (o.product === "desktop") {
      const price = catalog.products.desktop;
      if (o.reason !== "purchase") return `amount: a Desktop order with reason ${o.reason}`;
      if (
        o.items.length !== 1 ||
        o.items[0].priceId !== price.priceId ||
        o.items[0].amountCents !== price.amountCents ||
        o.subtotalCents !== price.amountCents
      )
        return `amount: Desktop is ${price.amountCents}, the order is ${o.subtotalCents}`;
      const r = await checkCheckout(
        "order",
        "desktop",
        o.providerCheckoutId,
        o.checkoutRef,
        o.userId,
      );
      if (r) return r;
    } else if (isPro(o.product)) {
      if (o.reason === "subscription_create" || o.reason === "subscription_cycle") {
        for (const i of o.items) {
          if (!i.product || i.proration) continue;
          const full = catalog.products[i.product].amountCents;
          const trialStart = o.reason === "subscription_create" && i.amountCents === 0;
          if (!isPro(i.product) || (i.amountCents !== full && !trialStart))
            return `amount: ${i.product} item is ${i.amountCents}, the price is ${full}`;
        }
      } else if (o.reason === "subscription_update") {
        for (const i of o.items) {
          if (!isPro(i.product)) return `unknown_price: a switch item without a Pro price`;
          if (Math.abs(i.amountCents) > yearly) return `amount: a switch item of ${i.amountCents}`;
        }
      } else if (o.reason !== "subscription_meter_cycle") {
        return `amount: a Pro order with reason ${o.reason}`;
      }
      if (!o.providerSubscriptionId) return `amount: a Pro order without a subscription`;
    } else if (o.product === "api") {
      if (o.reason === "purchase") return "amount: an API order with reason purchase";
      for (const i of o.items)
        if (i.product && i.product !== "api") return `unknown_price: ${i.priceId} on an API order`;
      if (!o.providerSubscriptionId) return "amount: an API order without a subscription";
    }
    if (o.providerSubscriptionId && !o.subscription) {
      const known = await one(
        tx,
        sql`select 1 as x from subscriptions where provider = 'polar' and provider_subscription_id = ${o.providerSubscriptionId}`,
      );
      if (!known) return `unknown_subscription: ${o.providerSubscriptionId}`;
    }
    const stored = await one<{ user_id: string | null; provider_customer_id: string | null }>(
      tx,
      sql`select user_id, provider_customer_id from orders where provider = 'polar' and provider_order_id = ${o.providerOrderId}
          union all
          select user_id, null from invoices where provider = 'polar' and provider_invoice_id = ${o.providerOrderId}`,
    );
    if (stored?.provider_customer_id && stored.provider_customer_id !== o.providerCustomerId)
      return `customer_changed: order ${o.providerOrderId}`;
    if (stored?.user_id && o.userId && stored.user_id !== o.userId)
      return `user_changed: order ${o.providerOrderId}`;
    customerPairs.push([o.userId, o.providerCustomerId]);
  }

  for (const c of facts.customers) customerPairs.push([c.userId, c.providerCustomerId]);
  for (const [userId, pcid] of customerPairs) {
    if (!userId) continue;
    const byUser = await one<{ provider_customer_id: string }>(
      tx,
      sql`select provider_customer_id from billing_customers where provider = 'polar' and user_id = ${userId}`,
    );
    if (byUser && byUser.provider_customer_id !== pcid) return `customer_changed: user ${userId}`;
    const byCustomer = await one<{ user_id: string | null }>(
      tx,
      sql`select user_id from billing_customers where provider = 'polar' and provider_customer_id = ${pcid}`,
    );
    if (byCustomer?.user_id && byCustomer.user_id !== userId)
      return `customer_changed: customer ${pcid}`;
  }
  return null;
}

// ------------------------------------------------------------------ apply

type Version = { newer: boolean; same: boolean; same_hash: boolean };

async function versionOf(
  q: Q,
  table: "orders" | "invoices" | "subscriptions" | "disputes",
  idColumn: string,
  providerId: string,
  version: string,
  hash: string,
) {
  return one<Version & { id: string }>(
    q,
    sql`select id,
          (provider_version is null or ${version}::timestamptz > provider_version) as newer,
          (provider_version = ${version}::timestamptz) as same,
          (provider_hash = ${hash}) as same_hash
        from ${sql.identifier(table)} where provider = 'polar' and ${sql.identifier(idColumn)} = ${providerId}`,
  );
}

type Decision = "insert" | "apply" | "skip" | "conflict";
function decide(v: Version | null): Decision {
  if (!v) return "insert";
  if (v.newer) return "apply";
  if (v.same && v.same_hash) return "skip";
  if (v.same) return "conflict";
  return "skip";
}

/** On a conflict, the provider's current object wins if it is at least as new as ours. */
async function atLeastStored(
  q: Q,
  table: string,
  idColumn: string,
  providerId: string,
  version: string,
) {
  const r = await one<{ ok: boolean }>(
    q,
    sql`select (provider_version is null or ${version}::timestamptz >= provider_version) as ok
        from ${sql.identifier(table)} where provider = 'polar' and ${sql.identifier(idColumn)} = ${providerId}`,
  );
  return r?.ok ?? true;
}

type Touched = {
  orders: Set<string>;
  subscriptions: Set<string>;
  notes: string[];
  userIds: Set<string>;
};

async function applyCustomer(ctx: BillingContext, tx: Q, c: CustomerFact, now: Date) {
  if (!c.email) return;
  const userId = await existingUser(tx, c.userId);
  await tx.execute(sql`
    insert into billing_customers (id, user_id, provider, provider_customer_id, email, deleted_at, provider_version, created_at, updated_at)
    values (${newId("cus")}, ${userId}, 'polar', ${c.providerCustomerId}, ${c.email}, ${c.deleted ? now : null}, ${c.version}::timestamptz, ${now}, ${now})
    on conflict (provider, provider_customer_id) do update set
      user_id = coalesce(billing_customers.user_id, excluded.user_id),
      email = case when billing_customers.provider_version is null or excluded.provider_version >= billing_customers.provider_version then excluded.email else billing_customers.email end,
      deleted_at = coalesce(billing_customers.deleted_at, excluded.deleted_at),
      provider_version = greatest(billing_customers.provider_version, excluded.provider_version),
      updated_at = excluded.updated_at`);
  void ctx;
}

const checkoutRank: Record<string, number> = {
  created: 0,
  open: 1,
  confirmed: 2,
  expired: 3,
  failed: 3,
  succeeded: 4,
};

async function applyCheckout(tx: Q, c: CheckoutFact, now: Date) {
  const row = await resolveCheckout(tx, c.providerCheckoutId, c.checkoutRef);
  if (!row) return;
  const status =
    (checkoutRank[c.status] ?? 0) >= (checkoutRank[row.status] ?? 0) ? c.status : row.status;
  await tx.execute(sql`
    update checkouts set status = ${status}, provider_checkout_id = coalesce(provider_checkout_id, ${c.providerCheckoutId}), updated_at = ${now}
    where id = ${row.id}`);
}

async function applySubscription(
  ctx: BillingContext,
  tx: Q,
  fact: SubscriptionFact,
  cards: Map<string, boolean>,
  touched: Touched,
  now: Date,
) {
  let s = fact;
  const v = await versionOf(
    tx,
    "subscriptions",
    "provider_subscription_id",
    s.providerSubscriptionId,
    s.version,
    s.hash,
  );
  let d = decide(v);
  if (d === "conflict") {
    s = await ctx.provider.getSubscription(s.providerSubscriptionId);
    // The fetched object passes the same checks as a delivered one.
    const bad = await checkFacts(ctx, tx, { ...emptyFacts(), subscriptions: [s] });
    if (bad) {
      touched.notes.push(
        `conflict on subscription ${s.providerSubscriptionId}: fetched fact rejected (${bad})`,
      );
      await alert(tx, now, "rejected_fact", `fetch:${s.providerSubscriptionId}`, bad);
      return;
    }
    d = (await atLeastStored(
      tx,
      "subscriptions",
      "provider_subscription_id",
      s.providerSubscriptionId,
      s.version,
    ))
      ? "apply"
      : "skip";
    touched.notes.push(`conflict on subscription ${s.providerSubscriptionId}: fetched`);
  }
  const product = s.product!;
  const kind = product === "api" ? "api" : "pro";
  const interval = product === "pro_month" ? "month" : product === "pro_year" ? "year" : null;
  const userId = await existingUser(tx, s.userId);
  const cardSeen = s.userId ? cards.get(s.userId) === true : false;
  if (d === "insert") {
    const co = await resolveCheckout(tx, s.providerCheckoutId, s.checkoutRef);
    if (co && !co.provider_checkout_id && s.providerCheckoutId)
      await tx.execute(
        sql`update checkouts set provider_checkout_id = ${s.providerCheckoutId}, updated_at = ${now} where id = ${co.id}`,
      );
    // The cap set at enrollment reaches the subscription in the same transaction.
    const cap = kind === "api" ? (co?.spend_cap_cents ?? null) : null;
    const email =
      s.email ??
      (co?.user_id
        ? (await one<{ email: string }>(tx, sql`select email from users where id = ${co.user_id}`))
            ?.email
        : null);
    if (!email) throw new Error(`subscription ${s.providerSubscriptionId} has no email`);
    // The money is real, so it is stored, but an account that no longer exists
    // (deleted while its checkout was in flight) needs Leo to end and refund it.
    if (s.userId && !userId)
      await alert(
        tx,
        now,
        "subscription_without_account",
        s.providerSubscriptionId,
        `external id ${s.userId} has no account`,
      );
    const inserted = await one<{ id: string }>(
      tx,
      sql`
      insert into subscriptions (id, provider, provider_subscription_id, provider_customer_id, user_id, email, kind, interval, status,
        trial_ends_at, current_period_start, current_period_end, cancel_at_period_end, canceled_at, ended_at, spend_cap_cents,
        checkout_id, provider_version, provider_hash, pending_update, card_seen_at, created_at, updated_at)
      values (${newId("sub")}, 'polar', ${s.providerSubscriptionId}, ${s.providerCustomerId}, ${userId}, ${email}, ${kind}, ${interval}, ${s.status},
        ${s.trialEndsAt}::timestamptz, ${s.currentPeriodStart}::timestamptz, ${s.currentPeriodEnd}::timestamptz, ${s.cancelAtPeriodEnd},
        ${s.canceledAt}::timestamptz, ${s.endedAt}::timestamptz, ${cap}, ${co?.id ?? null}, ${s.version}::timestamptz, ${s.hash},
        ${s.pendingUpdate === undefined || s.pendingUpdate === null ? null : JSON.stringify(s.pendingUpdate)}::jsonb,
        ${kind === "api" && s.status === "active" && cardSeen ? now : null}, ${now}, ${now})
      returning id`,
    );
    touched.subscriptions.add(inserted!.id);
    if (userId) touched.userIds.add(userId);
    await duplicateCheck(tx, userId, kind, now);
    return;
  }
  if (d === "skip") {
    // A card can appear without the subscription changing: record it anyway.
    if (kind === "api" && s.status === "active" && cardSeen) {
      const r = await tx.execute(sql`
        update subscriptions set card_seen_at = ${now}, updated_at = ${now}
        where provider = 'polar' and provider_subscription_id = ${s.providerSubscriptionId} and card_seen_at is null and status = 'active'`);
      if ((r.rowCount ?? 0) > 0) touched.notes.push(`card seen for ${s.providerSubscriptionId}`);
    }
    return;
  }
  const stored = (await one<{ id: string; status: string; ended_at: Date | null }>(
    tx,
    sql`select id, status, ended_at from subscriptions where provider = 'polar' and provider_subscription_id = ${s.providerSubscriptionId}`,
  ))!;
  const nowEnded =
    s.status === "canceled" || s.status === "incomplete_expired" || s.endedAt !== null;
  if (endedSubscription(stored) && !nowEnded) {
    touched.notes.push(
      `contradiction: subscription ${s.providerSubscriptionId} ended, reported ${s.status}`,
    );
    await alert(
      tx,
      now,
      "contradiction",
      `subscription:${s.providerSubscriptionId}`,
      `ended, then reported ${s.status}`,
    );
    return;
  }
  await tx.execute(sql`
    update subscriptions set
      status = ${s.status}, interval = ${interval}, trial_ends_at = ${s.trialEndsAt}::timestamptz,
      current_period_start = ${s.currentPeriodStart}::timestamptz, current_period_end = ${s.currentPeriodEnd}::timestamptz,
      cancel_at_period_end = ${s.cancelAtPeriodEnd}, canceled_at = ${s.canceledAt}::timestamptz,
      ended_at = coalesce(ended_at, ${s.endedAt}::timestamptz),
      pending_update = case when ${s.pendingUpdate === undefined} then pending_update else ${s.pendingUpdate === undefined || s.pendingUpdate === null ? null : JSON.stringify(s.pendingUpdate)}::jsonb end,
      provider_customer_id = ${s.providerCustomerId}, user_id = coalesce(user_id, ${userId}),
      card_seen_at = case when card_seen_at is null and ${kind === "api" && s.status === "active" && cardSeen} then ${now}::timestamptz else card_seen_at end,
      provider_version = ${s.version}::timestamptz, provider_hash = ${s.hash}, updated_at = ${now}
    where id = ${stored.id}`);
  touched.subscriptions.add(stored.id);
  if (userId) touched.userIds.add(userId);
  if (["paused"].includes(s.status))
    await alert(tx, now, "subscription_paused", s.providerSubscriptionId, "paused at the provider");
  await duplicateCheck(tx, userId, kind, now);
}

/** A second live Pro or API subscription is stored (the money is real) and alerted. */
async function duplicateCheck(tx: Q, userId: string | null, kind: string, now: Date) {
  if (!userId) return;
  const live = await rows<{ provider_subscription_id: string }>(
    tx,
    sql`
    select provider_subscription_id from subscriptions
    where user_id = ${userId} and kind = ${kind}
      and status in ('trialing', 'active', 'past_due') and (ended_at is null or ended_at > ${now})`,
  );
  if (live.length > 1)
    await alert(
      tx,
      now,
      "duplicate_subscription",
      `${kind}:${userId}`,
      `${live.length} live ${kind} subscriptions: ${live.map((l) => l.provider_subscription_id).join(", ")}; refund by hand`,
    );
}

function describeOrder(o: OrderFact): string {
  if (o.product === "desktop") return "Desktop License, 12 months of updates";
  if (o.product === "api")
    return o.reason === "subscription_create" ? "API enrollment" : "API usage";
  const interval = o.product === "pro_year" ? "yearly" : "monthly";
  if (o.reason === "subscription_update") return `Pro, switch to ${interval}`;
  if (o.reason === "subscription_create" && o.netCents === 0 && o.appliedBalanceCents === 0)
    return `Pro, ${interval} (free trial)`;
  return `Pro, ${interval}`;
}

async function applyOrder(
  ctx: BillingContext,
  tx: Q,
  fact: OrderFact,
  touched: Touched,
  now: Date,
) {
  let o = fact;
  const desktop = o.product === "desktop";
  const table = desktop ? "orders" : "invoices";
  const idColumn = desktop ? "provider_order_id" : "provider_invoice_id";
  const v = await versionOf(tx, table, idColumn, o.providerOrderId, o.version, o.hash);
  let d = decide(v);
  if (d === "conflict") {
    o = await ctx.provider.getOrder(o.providerOrderId);
    // The fetched object passes the same checks as a delivered one.
    const bad = await checkFacts(ctx, tx, { ...emptyFacts(), orders: [o] });
    if (bad) {
      touched.notes.push(`conflict on order ${o.providerOrderId}: fetched fact rejected (${bad})`);
      await alert(tx, now, "rejected_fact", `fetch:${o.providerOrderId}`, bad);
      return;
    }
    d = (await atLeastStored(tx, table, idColumn, o.providerOrderId, o.version)) ? "apply" : "skip";
    touched.notes.push(`conflict on order ${o.providerOrderId}: fetched`);
  }
  if (d === "skip") return;
  const userId = await existingUser(tx, o.userId);
  if (userId) touched.userIds.add(userId);
  const paid = paidStatuses.has(o.status);
  const refunding = o.status === "refunded" || o.status === "partially_refunded";
  const istatus = invoiceStatus[o.status];

  if (desktop) {
    let orderId: string;
    if (d === "insert") {
      const co = await resolveCheckout(tx, o.providerCheckoutId, o.checkoutRef);
      if (co && !co.provider_checkout_id && o.providerCheckoutId)
        await tx.execute(
          sql`update checkouts set provider_checkout_id = ${o.providerCheckoutId}, updated_at = ${now} where id = ${co.id}`,
        );
      const r = await one<{ id: string }>(
        tx,
        sql`
        insert into orders (id, provider, provider_order_id, provider_customer_id, user_id, email, product, amount_cents, currency, status,
          paid_at, billed_at, refunded_cents, refunded_at, checkout_id, provider_version, provider_hash, created_at, updated_at)
        values (${newId("ord")}, 'polar', ${o.providerOrderId}, ${o.providerCustomerId}, ${userId}, ${o.email}, 'desktop', ${o.netCents}, ${o.currency}, ${o.status},
          ${paid ? now : null}, ${o.billedAt}::timestamptz, ${o.refundedCents}, ${refunding ? now : null}, ${co?.id ?? null},
          ${o.version}::timestamptz, ${o.hash}, ${now}, ${now})
        returning id`,
      );
      orderId = r!.id;
    } else {
      const stored = (await one<{ id: string; status: string }>(
        tx,
        sql`select id, status from orders where provider = 'polar' and provider_order_id = ${o.providerOrderId}`,
      ))!;
      orderId = stored.id;
      if (terminalOrder.has(stored.status) && stored.status !== o.status) {
        touched.notes.push(
          `contradiction: order ${o.providerOrderId} is ${stored.status}, reported ${o.status}`,
        );
        await alert(
          tx,
          now,
          "contradiction",
          `order:${o.providerOrderId}`,
          `${stored.status}, then reported ${o.status}`,
        );
        touched.orders.add(orderId);
        return;
      }
      await tx.execute(sql`
        update orders set status = ${o.status},
          paid_at = coalesce(paid_at, ${paid ? now : null}::timestamptz),
          refunded_cents = greatest(refunded_cents, ${o.refundedCents}),
          refunded_at = coalesce(refunded_at, ${refunding ? now : null}::timestamptz),
          provider_customer_id = ${o.providerCustomerId}, user_id = coalesce(user_id, ${userId}),
          provider_version = ${o.version}::timestamptz, provider_hash = ${o.hash}, updated_at = ${now}
        where id = ${orderId}`);
    }
    // The invoice row the billing page lists, mirroring the order.
    await upsertInvoice(tx, o, {
      orderId,
      subscriptionId: null,
      userId,
      status: istatus,
      paid,
      now,
    });
    touched.orders.add(orderId);
    return;
  }

  // A subscription order: its invoice and payment coverage.
  const sub = await one<{ id: string }>(
    tx,
    sql`select id from subscriptions where provider = 'polar' and provider_subscription_id = ${o.providerSubscriptionId}`,
  );
  if (!sub) throw new Error(`order ${o.providerOrderId} arrived before its subscription`);
  if (d === "apply") {
    const stored = (await one<{ status: string }>(
      tx,
      sql`select status from invoices where provider = 'polar' and provider_invoice_id = ${o.providerOrderId}`,
    ))!;
    if (terminalOrder.has(stored.status) && stored.status !== istatus) {
      touched.notes.push(
        `contradiction: invoice ${o.providerOrderId} is ${stored.status}, reported ${istatus}`,
      );
      await alert(
        tx,
        now,
        "contradiction",
        `invoice:${o.providerOrderId}`,
        `${stored.status}, then reported ${istatus}`,
      );
      touched.subscriptions.add(sub.id);
      return;
    }
  }
  const invoiceId = await upsertInvoice(tx, o, {
    orderId: null,
    subscriptionId: sub.id,
    userId,
    status: istatus,
    paid,
    now,
  });
  for (const i of o.items) {
    if (!isPro(i.product) || !i.periodStart || !i.periodEnd || i.periodEnd <= i.periodStart)
      continue;
    await tx.execute(sql`
      insert into payment_coverage (id, invoice_id, provider_item_id, subscription_id, product, price_id, period_start, period_end, amount_cents, kind, created_at)
      values (${newId("cov")}, ${invoiceId}, ${i.providerItemId}, ${sub.id}, ${i.product}, ${i.priceId}, ${i.periodStart}::timestamptz,
        ${i.periodEnd}::timestamptz, ${i.amountCents}, ${i.proration ? "proration" : "period"}, ${now})
      on conflict (provider_item_id) do update set
        period_start = excluded.period_start, period_end = excluded.period_end,
        amount_cents = excluded.amount_cents, product = excluded.product,
        price_id = excluded.price_id, kind = excluded.kind`);
  }
  touched.subscriptions.add(sub.id);
}

async function upsertInvoice(
  tx: Q,
  o: OrderFact,
  x: {
    orderId: string | null;
    subscriptionId: string | null;
    userId: string | null;
    status: string;
    paid: boolean;
    now: Date;
  },
): Promise<string> {
  const r = await one<{ id: string }>(
    tx,
    sql`
    insert into invoices (id, provider, provider_invoice_id, user_id, email, subscription_id, order_id, description, amount_cents, currency,
      status, issued_at, reason, billed_at, paid_at, net_cents, applied_balance_cents, refunded_cents, provider_version, provider_hash, created_at, updated_at)
    values (${newId("inv")}, 'polar', ${o.providerOrderId}, ${x.userId}, ${o.email}, ${x.subscriptionId}, ${x.orderId}, ${describeOrder(o)},
      ${Math.max(0, o.netCents)}, ${o.currency}, ${x.status}, ${o.billedAt}::timestamptz, ${o.reason}, ${o.billedAt}::timestamptz,
      ${x.paid ? x.now : null}, ${o.netCents}, ${o.appliedBalanceCents}, ${o.refundedCents}, ${o.version}::timestamptz, ${o.hash}, ${x.now}, ${x.now})
    on conflict (provider, provider_invoice_id) do update set
      status = excluded.status,
      paid_at = coalesce(invoices.paid_at, excluded.paid_at),
      refunded_cents = greatest(invoices.refunded_cents, excluded.refunded_cents),
      net_cents = excluded.net_cents, applied_balance_cents = excluded.applied_balance_cents,
      user_id = coalesce(invoices.user_id, excluded.user_id),
      provider_version = excluded.provider_version, provider_hash = excluded.provider_hash, updated_at = excluded.updated_at
    returning id`,
  );
  return r!.id;
}

async function applyDispute(
  ctx: BillingContext,
  tx: Q,
  fact: DisputeFact,
  touched: Touched,
  now: Date,
) {
  let d0 = fact;
  const target = await one<{
    order_id: string | null;
    invoice_id: string | null;
    subscription_id: string | null;
  }>(
    tx,
    sql`
    select o.id as order_id, null::text as invoice_id, null::text as subscription_id from orders o where o.provider = 'polar' and o.provider_order_id = ${d0.providerOrderId}
    union all
    select null, i.id, i.subscription_id from invoices i where i.provider = 'polar' and i.provider_invoice_id = ${d0.providerOrderId} and i.order_id is null`,
  );
  if (!target) {
    touched.notes.push(`dispute ${d0.providerDisputeId}: unknown order ${d0.providerOrderId}`);
    await alert(
      tx,
      now,
      "dispute_unknown_order",
      d0.providerDisputeId,
      `order ${d0.providerOrderId} is not ours`,
    );
    return;
  }
  const v = await versionOf(
    tx,
    "disputes",
    "provider_dispute_id",
    d0.providerDisputeId,
    d0.version,
    d0.hash,
  );
  let d = decide(v);
  if (d === "conflict") {
    d0 = await ctx.provider.getDispute(d0.providerDisputeId);
    d = (await atLeastStored(
      tx,
      "disputes",
      "provider_dispute_id",
      d0.providerDisputeId,
      d0.version,
    ))
      ? "apply"
      : "skip";
  }
  if (d === "skip") return;
  if (d === "insert") {
    await tx.execute(sql`
      insert into disputes (id, provider, provider_dispute_id, order_id, invoice_id, status, amount_cents, closed, provider_version, provider_hash, created_at, updated_at)
      values (${newId("dsp")}, 'polar', ${d0.providerDisputeId}, ${target.order_id}, ${target.invoice_id}, ${d0.status}, ${d0.amountCents}, ${d0.closed},
        ${d0.version}::timestamptz, ${d0.hash}, ${now}, ${now})`);
  } else {
    const stored = (await one<{ status: string }>(
      tx,
      sql`select status from disputes where provider = 'polar' and provider_dispute_id = ${d0.providerDisputeId}`,
    ))!;
    if ((stored.status === "lost" || stored.status === "won") && stored.status !== d0.status) {
      touched.notes.push(
        `contradiction: dispute ${d0.providerDisputeId} is ${stored.status}, reported ${d0.status}`,
      );
      await alert(
        tx,
        now,
        "contradiction",
        `dispute:${d0.providerDisputeId}`,
        `${stored.status}, then reported ${d0.status}`,
      );
      return;
    }
    await tx.execute(sql`
      update disputes set status = ${d0.status}, amount_cents = ${d0.amountCents}, closed = ${d0.closed},
        provider_version = ${d0.version}::timestamptz, provider_hash = ${d0.hash}, updated_at = ${now}
      where provider = 'polar' and provider_dispute_id = ${d0.providerDisputeId}`);
  }
  if (d0.status === "lost")
    await alert(
      tx,
      now,
      "dispute_lost",
      d0.providerDisputeId,
      `lost on order ${d0.providerOrderId}`,
    );
  if (target.order_id) touched.orders.add(target.order_id);
  if (target.subscription_id) touched.subscriptions.add(target.subscription_id);
}

/**
 * Locks, checks, applies and converges one set of facts inside `tx`. Callers own
 * the transaction and the webhook_events row.
 */
export async function applyFacts(
  ctx: BillingContext,
  tx: Q,
  facts: ProviderFacts,
  cards: Map<string, boolean>,
): Promise<IngestOutcome> {
  const now = ctx.clock();
  const subs = allSubscriptions(facts);
  const users = new Set<string>();
  for (const x of [...facts.orders, ...subs, ...facts.customers]) if (x.userId) users.add(x.userId);
  // A dispute on a subscription invoice converges that subscription, so it takes
  // the subscription's lock like every other fact about it.
  const disputeSubs: string[] = [];
  for (const d of facts.disputes) {
    const r = await one<{ id: string }>(
      tx,
      sql`select s.provider_subscription_id as id from invoices i join subscriptions s on s.id = i.subscription_id
          where i.provider = 'polar' and i.provider_invoice_id = ${d.providerOrderId}`,
    );
    if (r) disputeSubs.push(r.id);
  }
  // The user lock first, like claim_purchases, then every subject in a fixed order.
  for (const u of [...users].sort()) await lockKeys(tx, [`user:${u}`]);
  await lockKeys(tx, [
    ...facts.orders.map((o) => `polar:order:${o.providerOrderId}`),
    ...subs.map((s) => `polar:subscription:${s.providerSubscriptionId}`),
    ...facts.orders.flatMap((o) =>
      o.providerSubscriptionId ? [`polar:subscription:${o.providerSubscriptionId}`] : [],
    ),
    ...facts.disputes.map((d) => `polar:dispute:${d.providerDisputeId}`),
    ...facts.disputes.map((d) => `polar:order:${d.providerOrderId}`),
    ...disputeSubs.map((id) => `polar:subscription:${id}`),
    ...facts.checkouts.map((c) => `polar:checkout:${c.providerCheckoutId}`),
    ...facts.customers.map((c) => `polar:customer:${c.providerCustomerId}`),
  ]);

  const rejected = await checkFacts(ctx, tx, facts);
  if (rejected) return { rejected };

  const touched: Touched = {
    orders: new Set(),
    subscriptions: new Set(),
    notes: [],
    userIds: new Set(),
  };
  for (const a of facts.alerts) await alert(tx, now, a.kind, a.subject, a.detail);
  for (const c of facts.customers) await applyCustomer(ctx, tx, c, now);
  for (const c of facts.checkouts) await applyCheckout(tx, c, now);
  for (const s of facts.subscriptions) await applySubscription(ctx, tx, s, cards, touched, now);
  for (const o of facts.orders) {
    if (o.subscription) await applySubscription(ctx, tx, o.subscription, cards, touched, now);
    await applyOrder(ctx, tx, o, touched, now);
  }
  for (const d of facts.disputes) await applyDispute(ctx, tx, d, touched, now);

  for (const orderId of touched.orders) await convergeDesktop(ctx, tx, orderId, now);
  for (const subId of touched.subscriptions) {
    const s = await one<{ kind: string }>(
      tx,
      sql`select kind from subscriptions where id = ${subId}`,
    );
    if (s?.kind === "pro") await convergePro(ctx, tx, subId, now);
    else if (s?.kind === "api") await convergeApi(ctx, tx, subId, now);
  }
  for (const u of touched.userIds) await tx.execute(sql`select * from claim_purchases(${u})`);
  return { rejected: null, notes: touched.notes, userIds: [...touched.userIds] };
}

/**
 * Ingests facts outside a webhook (a reconciler fetch, the success page's sync, a
 * subscription change's answer). One transaction; a check failure is alerted and
 * nothing is written for those facts.
 */
export async function ingestFacts(
  ctx: BillingContext,
  facts: ProviderFacts,
  source: string,
): Promise<IngestOutcome> {
  const deadline = Date.now() + ctx.config.budgetMs * 4;
  const { cards } = await hydrate(ctx, facts, deadline);
  const outcome = await ctx.db.transaction(async (tx) => applyFacts(ctx, tx, facts, cards));
  if (outcome.rejected) {
    const subject =
      facts.orders[0]?.providerOrderId ??
      facts.subscriptions[0]?.providerSubscriptionId ??
      facts.disputes[0]?.providerDisputeId ??
      "facts";
    await alert(ctx.db, ctx.clock(), "rejected_fact", `${source}:${subject}`, outcome.rejected);
    ctx.log(`[billing] ${source}: rejected ${subject}: ${outcome.rejected}`);
  }
  return outcome;
}
