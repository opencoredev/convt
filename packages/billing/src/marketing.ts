// Campaign email consent and its push to Sequenzy. The database decides who is
// subscribed (packages/db/src/schema/marketing.ts); this module pushes each change
// to Sequenzy from the per-minute cron, signs the preferences links campaign
// footers carry, mirrors Sequenzy's unsubscribes back, and removes the contact
// when an account is deleted. Nothing here sends email.

import { sql } from "drizzle-orm";
import { base64urlEncode } from "@convt/license";
import {
  verifySequenzyWebhook,
  type Contact,
  type ContactResult,
  type ContactsClient,
} from "@convt/mail";

import { alert, type BillingContext, maskEmail, one } from "./context";
import { safeError } from "./outbox";

export type MarketingDeps = {
  /** Null until Sequenzy's marketing key is configured: consent is kept, nothing is pushed. */
  contacts: ContactsClient | null;
  /** Sequenzy list ids new contacts join; null keeps the workspace defaults. */
  lists: string[] | null;
  tags: string[];
  /** Signs preferences links. Null: links are neither made nor accepted. */
  linkSecret: string | null;
  /** Null: the webhook route answers 404. */
  webhookSecret: string | null;
};

export const marketingDisabled: MarketingDeps = {
  contacts: null,
  lists: null,
  tags: [],
  linkSecret: null,
  webhookSecret: null,
};

/** What the site shows. `subscribed: false` also covers an account never enrolled. */
export type MarketingPreference = { subscribed: boolean };
export type TokenPreference = MarketingPreference & { maskedEmail: string };

// --- preferences links -------------------------------------------------------

const tokenContext = "convt-marketing-preferences:v1:";

async function hmac(secret: string, message: string): Promise<Uint8Array> {
  const key = await crypto.subtle.importKey(
    "raw",
    new TextEncoder().encode(secret),
    { name: "HMAC", hash: "SHA-256" },
    false,
    ["sign"],
  );
  return new Uint8Array(await crypto.subtle.sign("HMAC", key, new TextEncoder().encode(message)));
}

/**
 * `<user id>.<HMAC of user id and address>`: lets whoever holds the email change
 * that account's marketing preference and nothing else. It does not expire,
 * because a link in an old email must keep working, but it stops working when
 * the account's address changes, so a previous address cannot control the new
 * one. Rotating MARKETING_LINK_SECRET revokes every link.
 */
export async function preferencesToken(
  secret: string,
  userId: string,
  email: string,
): Promise<string> {
  return `${userId}.${base64urlEncode(await hmac(secret, `${tokenContext}${userId}:${email}`))}`;
}

/** The user id a token names, before checking it. */
export function tokenUserId(token: string): string | null {
  if (token.length > 200) return null;
  const dot = token.lastIndexOf(".");
  return dot > 0 ? token.slice(0, dot) : null;
}

/** Whether `token` was made for this account and its current address. */
export async function checkPreferencesToken(
  secret: string,
  token: string,
  account: { userId: string; email: string },
): Promise<boolean> {
  const expected = await preferencesToken(secret, account.userId, account.email);
  if (expected.length !== token.length) return false;
  let diff = 0;
  for (let i = 0; i < token.length; i++) diff |= token.charCodeAt(i) ^ expected.charCodeAt(i);
  return diff === 0;
}

export function preferencesUrl(siteUrl: string, token: string): string {
  return `${siteUrl.replace(/\/$/, "")}/email/preferences?t=${encodeURIComponent(token)}`;
}

// --- consent -------------------------------------------------------------------

export async function marketingPreference(
  ctx: BillingContext,
  userId: string,
): Promise<MarketingPreference> {
  const row = await one<{ status: string }>(
    ctx.db,
    sql`select status from marketing_subscriptions where user_id = ${userId}`,
  );
  return { subscribed: row?.status === "subscribed" };
}

/** The person's own choice, from Settings or a preferences link. */
export async function setMarketingPreference(
  ctx: BillingContext,
  input: { userId: string; subscribed: boolean; source: "settings" | "email_link" },
): Promise<MarketingPreference> {
  await ctx.db.execute(
    sql`select set_marketing_consent(${input.userId}, ${input.subscribed}, ${input.source}, null, null)`,
  );
  return marketingPreference(ctx, input.userId);
}

async function tokenUser(ctx: BillingContext, token: string) {
  const secret = ctx.marketing.linkSecret;
  if (!secret) return null;
  const userId = tokenUserId(token);
  if (!userId) return null;
  const user = await one<{ id: string; email: string }>(
    ctx.db,
    sql`select id, email from users where id = ${userId}`,
  );
  if (!user) return null;
  return (await checkPreferencesToken(secret, token, { userId: user.id, email: user.email }))
    ? user
    : null;
}

export async function preferenceByToken(
  ctx: BillingContext,
  token: string,
): Promise<TokenPreference | null> {
  const user = await tokenUser(ctx, token);
  if (!user) return null;
  return { ...(await marketingPreference(ctx, user.id)), maskedEmail: maskEmail(user.email) };
}

export async function setPreferenceByToken(
  ctx: BillingContext,
  input: { token: string; subscribed: boolean },
): Promise<TokenPreference | null> {
  const user = await tokenUser(ctx, input.token);
  if (!user) return null;
  const pref = await setMarketingPreference(ctx, {
    userId: user.id,
    subscribed: input.subscribed,
    source: "email_link",
  });
  return { ...pref, maskedEmail: maskEmail(user.email) };
}

// --- push to Sequenzy ------------------------------------------------------------

/** Minutes until the next attempt after a retryable failure, by attempts made. */
const backoffMinutes = [1, 5, 15, 60, 180, 720];
/** After this many retryable failures in a row an alert goes into the daily digest. */
const alertAfterAttempts = 6;
const batchSize = 100;
/** Two Sequenzy calls of at most 10 seconds each, with room to spare. */
const leaseMs = 2 * 60_000;
/** Stop claiming after this long, well inside a Worker invocation. */
const runBudgetMs = 40_000;

type DueRow = {
  user_id: string;
  version: string;
  status: "subscribed" | "unsubscribed";
  source: string;
  reactivate: boolean;
  sync_attempts: number;
  synced_at: Date | null;
  email: string;
  email_verified: boolean;
  name: string;
  user_created_at: Date;
  desktop_buyer: boolean;
  pro_status: Contact["attributes"]["proStatus"];
};

export type SyncSummary = Record<"synced" | "held" | "retry" | "failed" | "skipped", number>;

async function contactFor(ctx: BillingContext, row: DueRow, secret: string): Promise<Contact> {
  return {
    externalId: row.user_id,
    email: row.email,
    firstName: row.name.trim().split(/\s+/)[0] ?? "",
    attributes: {
      preferencesUrl: preferencesUrl(
        ctx.config.siteUrl,
        await preferencesToken(secret, row.user_id, row.email),
      ),
      desktopBuyer: row.desktop_buyer,
      proStatus: row.pro_status,
    },
  };
}

async function push(
  ctx: BillingContext,
  contacts: ContactsClient,
  row: DueRow,
  secret: string,
): Promise<ContactResult> {
  if (row.status === "unsubscribed") {
    const result = await contacts.unsubscribe({
      externalId: row.user_id,
      email: row.email_verified ? row.email : null,
    });
    // No contact: nothing at Sequenzy to unsubscribe.
    return result.kind === "not_found" ? { kind: "ok" } : result;
  }
  const contact = await contactFor(ctx, row, secret);
  const updated = await contacts.update({ contact, reactivate: row.reactivate });
  if (updated.kind !== "not_found") return updated;
  return contacts.create({
    contact,
    tags: ctx.marketing.tags,
    lists: ctx.marketing.lists,
    // Accounts from before marketing email keep their signup date, which also
    // keeps Sequenzy from enrolling them in the welcome sequence.
    createdAt: row.source === "signup" ? null : row.user_created_at,
  });
}

/**
 * Writes the outcome unless the row changed while it was being pushed, and
 * releases the lease either way. A row that changed stays pending, so the next
 * run pushes its newest state. Returns whether the outcome was written.
 */
async function record(
  ctx: BillingContext,
  row: DueRow & { lease: Date },
  outcome:
    | { state: "synced" }
    | { state: "held" }
    | { state: "retry"; error: string; delayMs: number }
    | { state: "failed"; error: string },
) {
  const now = ctx.clock();
  const set = (() => {
    switch (outcome.state) {
      case "synced":
        return sql`sync_state = 'synced', sync_attempts = 0, synced_at = ${now}, reactivate = false, last_error = null`;
      case "held":
        return sql`sync_state = 'held', last_error = null`;
      case "retry":
        return sql`sync_attempts = sync_attempts + 1,
          next_sync_at = ${new Date(now.getTime() + outcome.delayMs)}, last_error = ${outcome.error}`;
      case "failed":
        return sql`sync_state = 'failed', sync_attempts = sync_attempts + 1, last_error = ${outcome.error}`;
      default: {
        const _exhaustive: never = outcome;
        return _exhaustive;
      }
    }
  })();
  const written = await ctx.db.execute(sql`
    update marketing_subscriptions set ${set}, sync_lease_until = null, updated_at = ${now}
    where user_id = ${row.user_id} and xmin::text = ${row.version} and sync_lease_until = ${row.lease}`);
  if ((written.rowCount ?? 0) > 0) return true;
  // Release only this run's own lease; a newer claimant keeps its own.
  await ctx.db.execute(
    sql`update marketing_subscriptions set sync_lease_until = null
      where user_id = ${row.user_id} and sync_lease_until = ${row.lease}`,
  );
  return false;
}

/**
 * Claims the next due row for this run alone and reads what the push needs. Rows
 * are claimed one at a time, just before their push, so a slow batch never holds
 * a lease it cannot use in time. Accounts being deleted are skipped: deletion
 * removes their contact.
 */
async function claimNext(ctx: BillingContext): Promise<(DueRow & { lease: Date }) | null> {
  const now = ctx.clock();
  const lease = new Date(now.getTime() + leaseMs);
  const claimed = await one<{ user_id: string }>(
    ctx.db,
    sql`
    update marketing_subscriptions set sync_lease_until = ${lease}
    where user_id = (
      select m.user_id from marketing_subscriptions m
      where m.sync_state = 'pending' and m.next_sync_at <= ${now}
        and (m.sync_lease_until is null or m.sync_lease_until <= ${now})
        and not exists (select 1 from account_deletions d where d.user_id = m.user_id and d.status <> 'done')
      order by m.next_sync_at
      limit 1
      for update skip locked)
    returning user_id`,
  );
  if (!claimed) return null;
  const row = await one<DueRow>(
    ctx.db,
    sql`
    select m.user_id, m.xmin::text as version, m.status, m.source, m.reactivate, m.sync_attempts,
      m.synced_at, u.email, u.email_verified, u.name, u.created_at as user_created_at,
      -- Paid only: a complimentary (zero-cost) Desktop order is not a purchase.
      exists (select 1 from licenses l where l.user_id = m.user_id and l.plan = 'desktop'
        and not l.trial and l.revoked_at is null
        and (l.order_id is null or exists (
          select 1 from orders o where o.id = l.order_id and o.amount_cents > 0))) as desktop_buyer,
      (select case
          when count(*) = 0 then 'none'
          when bool_or(s.status in ('active', 'past_due')) then 'active'
          when bool_or(s.status = 'trialing') then 'trialing'
          else 'ended' end
        from subscriptions s where s.user_id = m.user_id and s.kind = 'pro') as pro_status
    from marketing_subscriptions m join users u on u.id = m.user_id
    where m.user_id = ${claimed.user_id}`,
  );
  return row ? { ...row, lease } : null;
}

/** The cron step: pushes every due row. A no-op until Sequenzy is configured. */
export async function syncMarketing(ctx: BillingContext): Promise<SyncSummary> {
  const summary: SyncSummary = { synced: 0, held: 0, retry: 0, failed: 0, skipped: 0 };
  const { contacts, linkSecret } = ctx.marketing;
  if (!contacts || !linkSecret) return summary;
  const startedAt = Date.now();
  for (let n = 0; n < batchSize && Date.now() - startedAt < runBudgetMs; n++) {
    const row = await claimNext(ctx);
    if (!row) break;
    const now = ctx.clock();
    if (row.status === "subscribed" && !row.email_verified) {
      // The address is unproven; the users trigger sets the row pending again once
      // it is verified.
      await record(ctx, row, { state: "held" });
      summary.held++;
      continue;
    }
    const result = await push(ctx, contacts, row, linkSecret);
    switch (result.kind) {
      case "ok":
        await record(ctx, row, { state: "synced" });
        summary.synced++;
        break;
      case "not_found":
        // Only an unsubscribe can see this, and push() already treats it as done.
        await record(ctx, row, { state: "synced" });
        summary.synced++;
        break;
      case "retry": {
        const attempts = row.sync_attempts + 1;
        const scheduled =
          backoffMinutes[Math.min(attempts - 1, backoffMinutes.length - 1)] * 60_000;
        const error = safeError(result.status, result.code);
        const written = await record(ctx, row, {
          state: "retry",
          error,
          delayMs: Math.max(scheduled, result.retryAfterMs ?? 0),
        });
        if (written && attempts === alertAfterAttempts)
          await alert(ctx.db, now, "marketing_sync", row.user_id, `still failing: ${error}`);
        summary.retry++;
        break;
      }
      case "refused": {
        const error = safeError(result.status, result.code);
        if (await record(ctx, row, { state: "failed", error }))
          await alert(ctx.db, now, "marketing_sync", row.user_id, `refused: ${error}`);
        summary.failed++;
        break;
      }
      default: {
        const _exhaustive: never = result;
        return _exhaustive;
      }
    }
  }
  if (Object.values(summary).some((n) => n > 0))
    ctx.log(`[billing] marketing sync ${JSON.stringify(summary)}`);
  return summary;
}

// --- Sequenzy webhook ----------------------------------------------------------------

export type MarketingWebhookResult = { status: number; body: string };

/**
 * POST /webhooks/sequenzy. An unsubscribe, complaint or bounce at Sequenzy becomes
 * `unsubscribed` here and is pushed back, so a push of an older state that lands
 * late is overwritten. An event from before the person last subscribed is
 * ignored by set_marketing_consent, so a retried or delayed delivery cannot undo
 * a later resubscribe.
 */
export async function handleMarketingWebhook(
  ctx: BillingContext,
  method: string,
  raw: Uint8Array,
  headers: Headers,
): Promise<MarketingWebhookResult> {
  const secret = ctx.marketing.webhookSecret;
  if (!secret) return { status: 404, body: "not found" };
  if (method !== "POST") return { status: 405, body: "method not allowed" };
  const check = await verifySequenzyWebhook({ secret, headers, raw, now: ctx.clock() });
  if (!check.ok) {
    ctx.log(`[billing] sequenzy webhook rejected: ${check.reason}`);
    return { status: check.reason === "malformed" ? 400 : 401, body: check.reason };
  }
  const event = check.event;
  if (event.kind === "other") return { status: 200, body: "ignored" };
  const user = event.externalId
    ? await one<{ id: string }>(ctx.db, sql`select id from users where id = ${event.externalId}`)
    : event.email
      ? await one<{ id: string }>(ctx.db, sql`select id from users where email = ${event.email}`)
      : null;
  if (!user) return { status: 200, body: "no account" };
  await ctx.db.execute(
    sql`select set_marketing_consent(${user.id}, false, 'provider', ${`${event.type} ${event.id}`}, ${event.occurredAt})`,
  );
  return { status: 200, body: "ok" };
}

// --- account deletion ----------------------------------------------------------------

/** Removes the Sequenzy contact. Throws while Sequenzy cannot confirm it, so deletion retries. */
export async function removeMarketingContact(ctx: BillingContext, userId: string): Promise<void> {
  const contacts = ctx.marketing.contacts;
  if (!contacts) return;
  // A push that claimed the row before the deletion began may still be running;
  // removing the contact under it would let it create the contact again.
  const leased = await one(
    ctx.db,
    sql`select 1 as x from marketing_subscriptions where user_id = ${userId} and sync_lease_until > ${ctx.clock()}`,
  );
  if (leased) throw new Error("waiting for a marketing push to finish");
  const result = await contacts.remove(userId);
  if (result.kind === "ok" || result.kind === "not_found") return;
  const error = new Error(`Sequenzy contact not removed: ${result.code}`);
  Object.assign(error, { status: result.status });
  throw error;
}
