// Converge: from the stored facts of one order or subscription to its licenses,
// revocations and emails. Idempotent: every insert is guarded by a unique business
// key, so running it twice, or concurrently with the reconciler, issues once.
// See docs/p7-billing-plan.md, section 3.

import { sql } from "drizzle-orm";
import { LIFETIME_UPDATES_UNTIL, newId, sign } from "@convt/license";

import { licensePurchasedEvent, type AnalyticsEvent } from "./analytics";
import { alert, type BillingContext, fault, isoDay, one, type Q, rows } from "./context";
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

export async function convergeDesktop(
  ctx: BillingContext,
  tx: Q,
  orderId: string,
  now: Date,
  events: AnalyticsEvent[] = [],
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
    if (existing.updates_until !== LIFETIME_UPDATES_UNTIL) {
      const token = await sign(
        { id: existing.id, email, plan: "desktop", issued, updates_until: LIFETIME_UPDATES_UNTIL },
        await ctx.signingKey(),
      );
      await tx.execute(sql`
        update licenses
        set email = ${email}, updates_until = ${LIFETIME_UPDATES_UNTIL}, token = ${token}, updated_at = ${now}
        where id = ${existing.id}`);
    }
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
  if (o.user_id) events.push(licensePurchasedEvent(o.user_id, "desktop", o.id));
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
