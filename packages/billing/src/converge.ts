// Converge: from the stored facts of one order or subscription to its licenses,
// revocations and emails. Idempotent: every insert is guarded by a unique business
// key, so running it twice, or concurrently with the reconciler, issues once.
// See docs/p7-billing-plan.md, section 3.

import { sql } from "drizzle-orm";
import { LIFETIME_UPDATES_UNTIL, newId, sign } from "@convt/license";

import { licensePurchasedEvent, type AnalyticsEvent } from "./analytics";
import { BUNDLE_CLOUD_DAYS } from "./catalog";
import { alert, type BillingContext, fault, isoDay, lockKeys, one, type Q, rows } from "./context";
import { enqueueEmail } from "./outbox";

/** The account's verified email when the purchase has a user, else the purchase's email. */
async function licenseEmail(q: Q, userId: string | null, fallback: string): Promise<string> {
  if (!userId) return fallback;
  const u = await one<{ email: string; email_verified: boolean }>(
    q,
    sql`select email, email_verified from users where id = ${userId}`,
  );
  return u?.email_verified ? u.email : fallback;
}

async function revoke(
  q: Q,
  where: ReturnType<typeof sql>,
  reason: "refunded" | "dispute_lost",
  now: Date,
) {
  await q.execute(sql`
    update licenses set revoked_at = ${now}, revoke_reason = ${reason}, updated_at = ${now}
    where revoked_at is null and ${where}`);
}

/**
 * A one-time order: its Desktop key, and for Desktop + Cloud also the buyer's cloud
 * years. Both run in the caller's transaction, under the user lock ingest holds.
 */
export async function convergeOrder(
  ctx: BillingContext,
  tx: Q,
  orderId: string,
  now: Date,
  events: AnalyticsEvent[] = [],
) {
  const o = await one<{ product: string }>(
    tx,
    sql`select product from orders where id = ${orderId}`,
  );
  if (!o) return;
  const bundle = o.product === "bundle";
  await convergeDesktop(ctx, tx, orderId, now, events, bundle ? "bundle" : "desktop");
  if (bundle) await scheduleCloud(tx, orderId, now);
}

const cloudMs = BUNDLE_CLOUD_DAYS * 86_400_000;

export type BundleRow = {
  id: string;
  status: string;
  billed_at: Date;
  email: string;
  user_id: string | null;
  provider_customer_id: string | null;
  checkout_id: string | null;
  lost: boolean;
  sub_id: string | null;
  sub_status: string | null;
  sub_start: Date | null;
  sub_end: Date | null;
  sub_ended_at: Date | null;
};

/**
 * Lays out the cloud years of a buyer's Desktop + Cloud orders as prepaid Pro rows
 * (`subscriptions.order_id`). Orders are taken in purchase order with a running
 * end: an order in good standing (paid or partly refunded, no lost dispute) covers
 * 365 days from its purchase or from the running end, whichever is later, so a
 * repeat purchase stacks. A refunded or lost order's row is canceled at
 * least(its end, now) and keeps those dates; it adds only the time it covered, so
 * later years move forward to fill the gap. Rows that already started never move
 * back: everything before them ended before they began. A guest order (no account
 * yet) is laid out on its own; bundle checkouts on convt.app require sign-in.
 */
export async function scheduleCloud(tx: Q, orderId: string, now: Date) {
  const owner = await one<{ user_id: string | null }>(
    tx,
    sql`select user_id from orders where id = ${orderId}`,
  );
  if (!owner) return;
  await layOutCloudYears(tx, owner.user_id ? { userId: owner.user_id } : { orderId }, now);
}

/**
 * Lays out every cloud year of an account again. Run after a claim attaches guest
 * orders: each was laid out on its own when it arrived, so two bought before the
 * account existed would otherwise overlap. The caller holds the user lock.
 */
export async function scheduleCloudForUser(tx: Q, userId: string, now: Date) {
  await layOutCloudYears(tx, { userId }, now);
}

/**
 * The same in a transaction of its own, under the user lock. The site calls it
 * (BillingRpc.scheduleCloud) after its sign-up claim, which runs as convt_web and
 * cannot write coverage.
 */
export async function rescheduleCloud(ctx: BillingContext, userId: string) {
  await ctx.db.transaction(async (tx) => {
    await lockKeys(tx, [`user:${userId}`]);
    await layOutCloudYears(tx, { userId }, ctx.clock(), () => fault(ctx, "after-cloud-read"));
  });
}

/**
 * The reconciler's backstop for a claim whose rescheduling never ran (the site's
 * call failed, and later sign-ins claim nothing). Every account with two or more
 * bundle orders and a running year is planned read-only; one whose layout would
 * change (overlapping years, or days a refund freed that a later year never took)
 * is laid out again under its account lock. Returns how many were.
 */
export async function repairCloudYears(ctx: BillingContext): Promise<number> {
  const now = ctx.clock();
  const users = await rows<{ user_id: string }>(
    ctx.db,
    sql`
    select o.user_id from orders o
    where o.product = 'bundle' and o.user_id is not null
    group by o.user_id having count(*) > 1
      and exists (select 1 from subscriptions s where s.user_id = o.user_id and s.order_id is not null
        and s.status = 'active' and s.ended_at > ${now})`,
  );
  let repaired = 0;
  for (const u of users) {
    const plan = planCloudYears(await readCloudYears(ctx.db, { userId: u.user_id }), now);
    if (plan.length === 0) continue;
    await rescheduleCloud(ctx, u.user_id);
    repaired++;
  }
  return repaired;
}

/** What laying out an account's cloud years would write: nothing when it is right. */
export type CloudWrite =
  | { kind: "insert"; order: BundleRow; start: Date; stop: Date }
  | { kind: "move"; subId: string; start: Date; stop: Date }
  | { kind: "cancel"; subId: string; endedAt: Date };

const ownerFilter = (owner: { userId: string } | { orderId: string }) =>
  "userId" in owner ? sql`o.user_id = ${owner.userId}` : sql`o.id = ${owner.orderId}`;

function readCloudYears(q: Q, owner: { userId: string } | { orderId: string }) {
  return rows<BundleRow>(
    q,
    sql`
    select o.id, o.status, o.billed_at, o.email, o.user_id, o.provider_customer_id, o.checkout_id,
      exists (select 1 from disputes d where d.order_id = o.id and d.status = 'lost') as lost,
      s.id as sub_id, s.status as sub_status, s.current_period_start as sub_start,
      s.current_period_end as sub_end, s.ended_at as sub_ended_at
    from orders o left join subscriptions s on s.order_id = o.id
    where o.product = 'bundle' and ${ownerFilter(owner)}
    order by o.billed_at, o.id`,
  );
}

/**
 * The layout rule (see scheduleCloud) as a pure plan over an account's bundle
 * orders in purchase order, so the reconciler can check an account without writing.
 */
export function planCloudYears(orders: BundleRow[], now: Date): CloudWrite[] {
  const writes: CloudWrite[] = [];
  let end: number | null = null;
  for (const r of orders) {
    const good = (r.status === "paid" || r.status === "partially_refunded") && !r.lost;
    if (good) {
      // Ended early (account deletion): it keeps its dates and holds nothing back.
      if (r.sub_status === "canceled") continue;
      const billed = r.billed_at.getTime();
      const start: Date = new Date(Math.max(billed, end ?? billed));
      const stop: Date = new Date(start.getTime() + cloudMs);
      end = stop.getTime();
      if (!r.sub_id) writes.push({ kind: "insert", order: r, start, stop });
      else if (
        r.sub_start?.getTime() !== start.getTime() ||
        r.sub_end?.getTime() !== stop.getTime()
      )
        writes.push({ kind: "move", subId: r.sub_id, start, stop });
    } else if (r.sub_id) {
      // A prepaid row always has its dates (subscriptions_prepaid_check).
      const scheduledEnd = r.sub_end ?? now;
      let endedAt = r.sub_ended_at ?? scheduledEnd;
      if (r.sub_status !== "canceled") {
        endedAt = new Date(Math.min(scheduledEnd.getTime(), now.getTime()));
        writes.push({ kind: "cancel", subId: r.sub_id, endedAt });
      }
      // Only the time it actually covered holds later years back.
      if (endedAt.getTime() > (r.sub_start ?? endedAt).getTime())
        end = Math.max(end ?? 0, endedAt.getTime());
    }
  }
  return writes;
}

async function layOutCloudYears(
  tx: Q,
  owner: { userId: string } | { orderId: string },
  now: Date,
  /** Tests pause here, between reading the years and writing them. */
  afterRead?: () => Promise<void>,
) {
  // Lock the existing years first, in a fixed order, then read: the read below is a
  // new statement, so it sees whatever a writer that held these rows committed.
  // Callers also hold the account lock; this keeps a stale read from ever writing.
  await tx.execute(sql`
    select s.id from subscriptions s join orders o on o.id = s.order_id
    where o.product = 'bundle' and ${ownerFilter(owner)}
    order by s.id for update of s`);
  const orders = await readCloudYears(tx, owner);
  await afterRead?.();
  for (const w of planCloudYears(orders, now)) {
    if (w.kind === "insert") {
      const r = w.order;
      await tx.execute(sql`
        insert into subscriptions (id, provider, provider_subscription_id, provider_customer_id, user_id, email, kind,
          interval, status, current_period_start, current_period_end, cancel_at_period_end, ended_at, checkout_id,
          order_id, created_at, updated_at)
        values (${newId("sub")}, 'polar', null, ${r.provider_customer_id}, ${r.user_id}, ${r.email}, 'pro',
          'year', 'active', ${w.start}, ${w.stop}, true, ${w.stop}, ${r.checkout_id}, ${r.id}, ${now}, ${now})`);
    } else if (w.kind === "move") {
      // A refunded or charged-back year keeps its dates for good.
      await tx.execute(sql`
        update subscriptions set current_period_start = ${w.start}, current_period_end = ${w.stop},
          ended_at = ${w.stop}, updated_at = ${now}
        where id = ${w.subId} and status <> 'canceled'`);
    } else {
      await tx.execute(sql`
        update subscriptions set status = 'canceled', canceled_at = ${now}, ended_at = ${w.endedAt}, updated_at = ${now}
        where id = ${w.subId} and status <> 'canceled'`);
    }
  }
}

export async function convergeDesktop(
  ctx: BillingContext,
  tx: Q,
  orderId: string,
  now: Date,
  events: AnalyticsEvent[] = [],
  product: "desktop" | "bundle" = "desktop",
) {
  const o = await one<{
    id: string;
    status: string;
    email: string;
    user_id: string | null;
    billed_at: Date;
    lost: boolean;
  }>(
    tx,
    sql`
    select o.id, o.status, o.email, o.user_id, o.billed_at,
      exists (select 1 from disputes d where d.order_id = o.id and d.status = 'lost') as lost
    from orders o where o.id = ${orderId}`,
  );
  if (!o) return;
  const original = sql`order_id = ${o.id} and plan = 'desktop' and reissue_of is null`;
  if (o.status === "refunded") return revoke(tx, original, "refunded", now);
  if (o.lost) return revoke(tx, original, "dispute_lost", now);
  if (o.status !== "paid" && o.status !== "partially_refunded") return;

  const email = await licenseEmail(tx, o.user_id, o.email);
  const issued = isoDay(o.billed_at);
  const existing = await one<{ id: string; updates_until: string }>(
    tx,
    sql`select id, updates_until::text from licenses where order_id = ${o.id} and plan = 'desktop' and reissue_of is null`,
  );
  if (existing) {
    // Existing signed rows are immutable for the billing role. Clients treat
    // every verified Desktop key as lifetime, so no rewrite is required.
    return;
  }
  const id = newId("lic");
  const updatesUntil = LIFETIME_UPDATES_UNTIL;
  const token = await sign(
    { id, email, plan: "desktop", issued, updates_until: updatesUntil },
    await ctx.signingKey(),
  );
  const inserted = await rows<{ id: string }>(
    tx,
    sql`
    insert into licenses (id, user_id, email, plan, trial, order_id, issued_on, updates_until, token, created_at, updated_at)
    values (${id}, ${o.user_id}, ${email}, 'desktop', false, ${o.id}, ${issued}, ${updatesUntil}, ${token}, ${now}, ${now})
    on conflict (order_id) where plan = 'desktop' and reissue_of is null do nothing
    returning id`,
  );
  if (inserted.length === 0) return;
  if (o.user_id) events.push(licensePurchasedEvent(o.user_id, "desktop", o.id, product));
  await fault(ctx, "after-license-insert");
  await enqueueEmail(tx, {
    kind: "license_issued",
    dedupeKey: `license_issued:${id}`,
    to: email,
    userId: o.user_id,
    subjectId: id,
    now,
  });
}

/**
 * Pro keys come only from funded coverage: a paid invoice never refunded in full,
 * with no lost dispute, that charged money or spent credit. Let E be the latest
 * end among funded rows; if its date is past every unrevoked original key of the
 * subscription, issue one key up to it.
 */
export async function convergePro(
  ctx: BillingContext,
  tx: Q,
  subscriptionId: string,
  now: Date,
  events: AnalyticsEvent[] = [],
) {
  // Revocations first, so a refunded invoice's key never counts as covering.
  await revoke(
    tx,
    sql`subscription_id = ${subscriptionId} and plan = 'pro' and invoice_id in (select id from invoices where status = 'refunded')`,
    "refunded",
    now,
  );
  await revoke(
    tx,
    sql`subscription_id = ${subscriptionId} and plan = 'pro' and invoice_id in (select invoice_id from disputes where status = 'lost' and invoice_id is not null)`,
    "dispute_lost",
    now,
  );

  const sub = await one<{
    id: string;
    status: string;
    user_id: string | null;
    email: string;
    current_period_start: Date | null;
  }>(
    tx,
    sql`select id, status, user_id, email, current_period_start from subscriptions where id = ${subscriptionId}`,
  );
  if (!sub) return;

  if (sub.status === "past_due" && sub.current_period_start) {
    await enqueueEmail(tx, {
      kind: "renewal_failed",
      dedupeKey: `renewal_failed:${sub.id}:${sub.current_period_start.toISOString()}`,
      to: await licenseEmail(tx, sub.user_id, sub.email),
      userId: sub.user_id,
      subjectId: sub.id,
      now,
    });
  }

  // A credit line is not paid coverage, so only positive items count.
  const funded = await one<{
    invoice_id: string;
    period_start: Date;
    period_end: Date;
    billed_at: Date;
  }>(
    tx,
    sql`
    select c.invoice_id, c.period_start, c.period_end, i.billed_at
    from payment_coverage c
    join invoices i on i.id = c.invoice_id
    where c.subscription_id = ${subscriptionId}
      and c.amount_cents > 0
      and i.status in ('paid', 'partially_refunded')
      and (i.net_cents > 0 or i.applied_balance_cents > 0)
      and not exists (select 1 from disputes d where d.invoice_id = i.id and d.status = 'lost')
    order by c.period_end desc, c.period_start desc
    limit 1`,
  );
  if (!funded) return;
  const until = isoDay(funded.period_end);
  const current = await one<{ max: string | null; any: boolean }>(
    tx,
    sql`
    select max(updates_until)::text as max,
      exists (select 1 from licenses where subscription_id = ${subscriptionId} and plan = 'pro') as any
    from licenses
    where subscription_id = ${subscriptionId} and plan = 'pro' and reissue_of is null and revoked_at is null`,
  );
  if (current?.max && until <= current.max) return;

  const id = newId("lic");
  const email = await licenseEmail(tx, sub.user_id, sub.email);
  const issued = isoDay(funded.billed_at);
  const periodStart = isoDay(funded.period_start);
  const token = await sign(
    { id, email, plan: "pro", issued, updates_until: until },
    await ctx.signingKey(),
  );
  // Either unique index (one key per invoice, one per paid-through date) makes a
  // concurrent second issue a no-op.
  const inserted = await rows<{ id: string }>(
    tx,
    sql`
    insert into licenses (id, user_id, email, plan, trial, subscription_id, invoice_id, period_start, issued_on, updates_until, token, created_at, updated_at)
    values (${id}, ${sub.user_id}, ${email}, 'pro', false, ${subscriptionId}, ${funded.invoice_id}, ${periodStart}, ${issued}, ${until}, ${token}, ${now}, ${now})
    on conflict do nothing
    returning id`,
  );
  if (inserted.length === 0) {
    // One key per invoice: a later correction of an invoice that already funded a
    // key cannot issue another, so it waits for Leo instead of failing silently.
    await alert(
      tx,
      now,
      "coverage_not_issued",
      funded.invoice_id,
      `funded through ${until}, but a key for this invoice or date exists`,
    );
    return;
  }
  await fault(ctx, "after-license-insert");
  if (!current?.any && sub.user_id) events.push(licensePurchasedEvent(sub.user_id, "pro", sub.id));
  // Only a subscription's first Pro key is emailed; renewals appear on the dashboard.
  if (!current?.any) {
    await enqueueEmail(tx, {
      kind: "license_issued",
      dedupeKey: `license_issued:${id}`,
      to: email,
      userId: sub.user_id,
      subjectId: id,
      now,
    });
  }
}

export async function convergeApi(_ctx: BillingContext, tx: Q, subscriptionId: string, now: Date) {
  const sub = await one<{
    id: string;
    status: string;
    user_id: string | null;
    email: string;
    current_period_start: Date | null;
  }>(
    tx,
    sql`select id, status, user_id, email, current_period_start from subscriptions where id = ${subscriptionId}`,
  );
  if (sub?.status === "past_due" && sub.current_period_start) {
    await enqueueEmail(tx, {
      kind: "renewal_failed",
      dedupeKey: `renewal_failed:${sub.id}:${sub.current_period_start.toISOString()}`,
      to: await licenseEmail(tx, sub.user_id, sub.email),
      userId: sub.user_id,
      subjectId: sub.id,
      now,
    });
  }
}
