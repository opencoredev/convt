// Checkouts. Our server creates every checkout, so every order can be matched to a
// row we recorded. The success page learns the result through checkoutResult,
// which releases a key only to the session user who owns the checkout or to the
// browser holding the checkout's nonce, and rotates the nonce on first disclosure.
// See docs/p7-billing-plan.md, sections 1 and 3.

import { createHash, timingSafeEqual } from "node:crypto";

import { sql } from "drizzle-orm";
import { base64urlDecode, base64urlEncode, newId, randomBytes } from "@convt/license";

import type { CatalogProduct } from "./catalog";
import { type BillingContext, fault, maskEmail, one, rows } from "./context";
import { ingestFacts } from "./ingest";
import { emptyFacts } from "./provider";

export const nonceTtlMs = 2 * 3600_000;
export const disclosedTtlMs = 10 * 60_000;
export const minCapCents = 100;
export const maxCapCents = 1_000_000;

export type CheckoutRefusal =
  | "sign_in_required"
  | "already_pro"
  | "already_enrolled"
  | "needs_multiple_subscriptions"
  | "bad_cap"
  | "provider_error"
  | "deleting";

export type CreatedCheckout =
  | { ok: true; url: string; checkoutId: string; cookieValue: string }
  | { ok: false; refusal: CheckoutRefusal };

const sha256 = (bytes: Uint8Array) => new Uint8Array(createHash("sha256").update(bytes).digest());

/** Live means the user cannot start another: trialing, active, past due, or a checkout still incomplete. */
const liveSql = (kind: "pro" | "api", userId: string, now: Date) => sql`
  select id from subscriptions
  where user_id = ${userId} and kind = ${kind}
    and status in ('incomplete', 'trialing', 'active', 'past_due', 'unpaid')
    and (ended_at is null or ended_at > ${now})`;

export async function createCheckout(
  ctx: BillingContext,
  input: {
    product: CatalogProduct;
    user: { id: string; email: string } | null;
    spendCapCents?: number | null;
  },
): Promise<CreatedCheckout> {
  const now = ctx.clock();
  const { product, user } = input;
  if (product !== "desktop" && !user) return { ok: false, refusal: "sign_in_required" };
  // No new purchases while the account is being deleted.
  if (
    user &&
    (await one(
      ctx.db,
      sql`select 1 as x from account_deletions where user_id = ${user.id} and status <> 'done'`,
    ))
  )
    return { ok: false, refusal: "deleting" };
  let allowTrial = false;
  let cap: number | null = null;
  if (product === "pro_month" || product === "pro_year") {
    if ((await rows(ctx.db, liveSql("pro", user!.id, now))).length)
      return { ok: false, refusal: "already_pro" };
    // A trial only for an account that never had a Pro subscription.
    const ever = await one(
      ctx.db,
      sql`select 1 as x from subscriptions where user_id = ${user!.id} and kind = 'pro' limit 1`,
    );
    allowTrial = !ever;
  }
  if (product === "api") {
    cap = input.spendCapCents ?? null;
    if (cap === null || !Number.isInteger(cap) || cap < minCapCents || cap > maxCapCents)
      return { ok: false, refusal: "bad_cap" };
    if ((await rows(ctx.db, liveSql("api", user!.id, now))).length)
      return { ok: false, refusal: "already_enrolled" };
    let settings;
    try {
      settings = await ctx.provider.settings();
    } catch {
      return { ok: false, refusal: "provider_error" };
    }
    if (!settings.allowMultipleSubscriptions)
      return { ok: false, refusal: "needs_multiple_subscriptions" };
  }
  const nonce = randomBytes(32);
  const id = newId("chk");
  await ctx.db.execute(sql`
    insert into checkouts (id, provider, user_id, product, allow_trial, spend_cap_cents, nonce_hash, nonce_expires_at, status, created_at, updated_at)
    values (${id}, 'polar', ${user?.id ?? null}, ${product}, ${allowTrial}, ${cap}, ${Buffer.from(sha256(nonce))},
      ${new Date(now.getTime() + nonceTtlMs)}, 'created', ${now}, ${now})`);
  await fault(ctx, "after-checkout-row");
  let created;
  try {
    created = await ctx.provider.createCheckout({
      product,
      checkoutRef: id,
      successUrl: `${ctx.config.siteUrl}/checkout/success?checkout_id={CHECKOUT_ID}`,
      allowTrial,
      externalCustomerId: user?.id ?? null,
      email: user?.email ?? null,
    });
  } catch (e) {
    ctx.log(`[billing] checkout ${id}: ${(e as Error).message}`);
    return { ok: false, refusal: "provider_error" };
  }
  await fault(ctx, "before-checkout-store");
  await ctx.db.execute(sql`
    update checkouts set provider_checkout_id = coalesce(provider_checkout_id, ${created.providerCheckoutId}),
      status = case when status = 'created' then 'open' else status end, updated_at = ${now}
    where id = ${id}`);
  return {
    ok: true,
    url: created.url,
    checkoutId: id,
    cookieValue: `${id}.${base64urlEncode(nonce)}`,
  };
}

export type CheckoutResult =
  | {
      state: "pending" | "trial" | "api_enrolled" | "failed" | "shown" | "not_found";
      product?: CatalogProduct;
    }
  | {
      state: "ready";
      product: CatalogProduct;
      maskedEmail: string;
      updatesUntil: string;
      token: string;
      licenseId: string;
    };

type CheckoutRow = {
  id: string;
  provider_checkout_id: string;
  user_id: string | null;
  product: CatalogProduct;
  nonce_hash: Uint8Array;
  nonce_expires_at: Date;
  key_disclosed_at: Date | null;
  status: string;
  synced_at: Date | null;
};

function parseCookie(value: string | null): { id: string; nonce: Uint8Array } | null {
  if (!value) return null;
  const m = value.match(/^(chk_[0-9a-z]{26})\.([A-Za-z0-9_-]{43})$/);
  if (!m) return null;
  const nonce = base64urlDecode(m[2]);
  return nonce && nonce.length === 32 ? { id: m[1], nonce } : null;
}

async function stateOf(ctx: BillingContext, row: CheckoutRow) {
  if (row.product === "desktop") {
    const lic = await one<{
      id: string;
      token: string;
      email: string;
      updates_until: string;
      revoked_at: Date | null;
      status: string;
    }>(
      ctx.db,
      sql`
      select l.id, l.token, l.email, l.updates_until::text, l.revoked_at, o.status
      from orders o left join licenses l on l.order_id = o.id and l.plan = 'desktop' and l.reissue_of is null
      where o.checkout_id = ${row.id} order by o.created_at limit 1`,
    );
    if (!lic)
      return {
        state: row.status === "failed" || row.status === "expired" ? "failed" : "pending",
      } as const;
    if (!lic.id) return { state: lic.status === "pending" ? "pending" : "failed" } as const;
    if (lic.revoked_at) return { state: "failed" } as const;
    return { state: "ready", lic } as const;
  }
  const sub = await one<{ id: string; status: string; card_seen_at: Date | null }>(
    ctx.db,
    sql`select id, status, card_seen_at from subscriptions where checkout_id = ${row.id} order by created_at limit 1`,
  );
  if (!sub)
    return {
      state: row.status === "failed" || row.status === "expired" ? "failed" : "pending",
    } as const;
  if (row.product === "api")
    return {
      state:
        sub.status === "active" && sub.card_seen_at
          ? "api_enrolled"
          : sub.status === "incomplete_expired"
            ? "failed"
            : "pending",
    } as const;
  if (sub.status === "trialing") return { state: "trial" } as const;
  const lic = await one<{
    id: string;
    token: string;
    email: string;
    updates_until: string;
    revoked_at: Date | null;
  }>(
    ctx.db,
    sql`
    select id, token, email, updates_until::text, revoked_at from licenses
    where subscription_id = ${sub.id} and plan = 'pro' and revoked_at is null
    order by updates_until desc limit 1`,
  );
  if (lic) return { state: "ready", lic } as const;
  if (sub.status === "canceled" || sub.status === "incomplete_expired")
    return { state: "failed" } as const;
  return { state: "pending" } as const;
}

/**
 * The success page's poll. `cookie` is the raw checkout cookie value;
 * `sessionUserId` is the verified session's user, if any. Returns the state and,
 * when the nonce rotates, the new cookie value to set.
 */
export async function checkoutResult(
  ctx: BillingContext,
  input: {
    providerCheckoutId: string;
    cookie: string | null;
    sessionUserId: string | null;
    sync: boolean;
  },
): Promise<{ result: CheckoutResult; setCookie: string | null }> {
  const now = ctx.clock();
  const row = await one<CheckoutRow>(
    ctx.db,
    sql`
    select id, provider_checkout_id, user_id, product, nonce_hash, nonce_expires_at, key_disclosed_at, status, synced_at
    from checkouts where provider = 'polar' and provider_checkout_id = ${input.providerCheckoutId}`,
  );
  const none = { result: { state: "not_found" } as CheckoutResult, setCookie: null };
  if (!row) return none;
  const owner = input.sessionUserId !== null && row.user_id === input.sessionUserId;
  const cookie = parseCookie(input.cookie);
  // The cookie's checkout id must be this row's, and its nonce must hash to the
  // stored one, compared in constant time; expiry is ours, not the cookie's.
  const cookieMatchesRow = cookie !== null && cookie.id === row.id;
  const nonceOk =
    cookieMatchesRow &&
    timingSafeEqual(Buffer.from(sha256(cookie!.nonce)), Buffer.from(row.nonce_hash)) &&
    row.nonce_expires_at.getTime() > now.getTime();
  if (!owner && !nonceOk) {
    // The old nonce after rotation, or an expired one: the key was shown already.
    if (cookieMatchesRow && row.key_disclosed_at)
      return { result: { state: "shown", product: row.product }, setCookie: null };
    return none;
  }

  let s = await stateOf(ctx, row);
  if (s.state === "pending" && input.sync && !row.synced_at) {
    // Once per checkout: pull it from the provider through ingest.
    await ctx.db.execute(
      sql`update checkouts set synced_at = ${now}, updated_at = ${now} where id = ${row.id} and synced_at is null`,
    );
    await syncCheckout(ctx, row.provider_checkout_id);
    s = await stateOf(ctx, row);
  }
  if (s.state !== "ready")
    return { result: { state: s.state, product: row.product }, setCookie: null };

  let setCookie: string | null = null;
  if (!owner) {
    if (!row.key_disclosed_at) {
      // First disclosure: a new nonce for 10 minutes, set in the same response.
      const fresh = randomBytes(32);
      const updated = await ctx.db.execute(sql`
        update checkouts set nonce_hash = ${Buffer.from(sha256(fresh))}, nonce_expires_at = ${new Date(now.getTime() + disclosedTtlMs)},
          key_disclosed_at = ${now}, updated_at = ${now}
        where id = ${row.id} and key_disclosed_at is null and nonce_hash = ${Buffer.from(row.nonce_hash)}`);
      // A concurrent first request rotated it already: this one is a replay.
      if ((updated.rowCount ?? 0) === 0)
        return { result: { state: "shown", product: row.product }, setCookie: null };
      setCookie = `${row.id}.${base64urlEncode(fresh)}`;
    }
  }
  return {
    result: {
      state: "ready",
      product: row.product,
      maskedEmail: maskEmail(s.lic.email),
      updatesUntil: s.lic.updates_until,
      token: s.lic.token,
      licenseId: s.lic.id,
    },
    setCookie,
  };
}

/** Pulls one checkout's orders and subscription from the provider through ingest. */
export async function syncCheckout(ctx: BillingContext, providerCheckoutId: string) {
  try {
    const co = await ctx.provider.getCheckout(providerCheckoutId);
    const facts = emptyFacts();
    facts.checkouts.push(co);
    if (co.providerSubscriptionId)
      facts.subscriptions.push(await ctx.provider.getSubscription(co.providerSubscriptionId));
    facts.orders.push(...(await ctx.provider.checkoutOrders(providerCheckoutId)));
    await ingestFacts(ctx, facts, "sync");
  } catch (e) {
    ctx.log(`[billing] sync ${providerCheckoutId}: ${(e as Error).message}`);
  }
}
