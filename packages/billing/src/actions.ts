// The billing page's actions. Each takes the user id from the site's verified
// session (through the RPC), acts on that user's own rows, calls the provider and
// ingests what it answers. See docs/p7-billing-plan.md, sections 1 and 4.

import { sql } from "drizzle-orm";

import { type BillingContext, one } from "./context";
import { ingestFacts } from "./ingest";
import { emptyFacts, type SubscriptionFact } from "./provider";
import { maxCapCents, minCapCents } from "./checkout";

const liveSub = (kind: "pro" | "api", userId: string, now: Date) => sql`
  select id, provider_subscription_id, status, interval, cancel_at_period_end from subscriptions
  where user_id = ${userId} and kind = ${kind}
    and status in ('trialing', 'active', 'past_due', 'unpaid', 'incomplete')
    and (ended_at is null or ended_at > ${now})
  order by created_at desc limit 1`;

async function ingestSubscription(ctx: BillingContext, fact: SubscriptionFact) {
  const facts = emptyFacts();
  facts.subscriptions.push(fact);
  await ingestFacts(ctx, facts, "action");
}

export type ActionResult =
  | { ok: true }
  | { ok: false; reason: "not_found" | "declined" | "provider_error" | "bad_cap" | "trial" };

export async function switchInterval(
  ctx: BillingContext,
  userId: string,
  to: "month" | "year",
): Promise<ActionResult> {
  const sub = await one<{ provider_subscription_id: string; interval: string }>(
    ctx.db,
    liveSub("pro", userId, ctx.clock()),
  );
  if (!sub) return { ok: false, reason: "not_found" };
  if (sub.interval === to) return { ok: true };
  const product = to === "year" ? "pro_year" : "pro_month";
  try {
    const r = await ctx.provider.changeProduct(
      sub.provider_subscription_id,
      product,
      ctx.catalog.switchPolicy[product],
    );
    if ("paymentFailed" in r) return { ok: false, reason: "declined" };
    await ingestSubscription(ctx, r);
    return { ok: true };
  } catch (e) {
    ctx.log(`[billing] switch for ${userId}: ${(e as Error).message}`);
    return { ok: false, reason: "provider_error" };
  }
}

export async function setCancel(
  ctx: BillingContext,
  userId: string,
  kind: "pro" | "api",
  cancel: boolean,
): Promise<ActionResult> {
  const sub = await one<{ provider_subscription_id: string }>(
    ctx.db,
    liveSub(kind, userId, ctx.clock()),
  );
  if (!sub) return { ok: false, reason: "not_found" };
  try {
    await ingestSubscription(
      ctx,
      await ctx.provider.setCancelAtPeriodEnd(sub.provider_subscription_id, cancel),
    );
    return { ok: true };
  } catch (e) {
    ctx.log(`[billing] cancel for ${userId}: ${(e as Error).message}`);
    return { ok: false, reason: "provider_error" };
  }
}

/** Polar's customer portal, only if the URL's origin is the provider's. */
export async function portalUrl(ctx: BillingContext, userId: string): Promise<string | null> {
  try {
    return await ctx.provider.portalUrl(userId, `${ctx.config.siteUrl}/dashboard/billing`);
  } catch (e) {
    ctx.log(`[billing] portal for ${userId}: ${(e as Error).message}`);
    return null;
  }
}

export async function receiptUrl(
  ctx: BillingContext,
  userId: string,
  invoiceId: string,
): Promise<string | null> {
  const inv = await one<{ provider_invoice_id: string }>(
    ctx.db,
    sql`select provider_invoice_id from invoices where id = ${invoiceId} and user_id = ${userId}`,
  );
  if (!inv) return null;
  try {
    return await ctx.provider.receiptUrl(inv.provider_invoice_id);
  } catch {
    return null;
  }
}

export async function card(ctx: BillingContext, userId: string) {
  try {
    const [c] = await ctx.provider.paymentMethods(userId);
    if (!c) return null;
    return {
      brand: c.brand.toUpperCase(),
      last4: c.last4,
      expires: `${String(c.expMonth).padStart(2, "0")}/${String(c.expYear).slice(-2)}`,
    };
  } catch {
    return null;
  }
}

/**
 * Sets the API spend cap under the subscription row lock that P9's reservations
 * take, so a cap change and a reservation serialize. Lowering the cap keeps
 * reservations already made; it only refuses new ones above it.
 */
export async function setSpendCap(
  ctx: BillingContext,
  userId: string,
  cents: number,
): Promise<ActionResult> {
  if (!Number.isInteger(cents) || cents < minCapCents || cents > maxCapCents)
    return { ok: false, reason: "bad_cap" };
  const now = ctx.clock();
  return ctx.db.transaction(async (tx) => {
    const sub = await one<{ id: string }>(
      tx,
      sql`
      select id from subscriptions
      where user_id = ${userId} and kind = 'api' and status not in ('canceled', 'incomplete_expired')
        and (ended_at is null or ended_at > ${now})
      order by created_at desc limit 1
      for update`,
    );
    if (!sub) return { ok: false, reason: "not_found" } as const;
    await tx.execute(
      sql`update subscriptions set spend_cap_cents = ${cents}, updated_at = ${now} where id = ${sub.id}`,
    );
    return { ok: true } as const;
  });
}
