// Account deletion. Subscriptions end first (an immediate revoke, no refund), then
// delete_user removes the account; financial rows keep their email. A cron step
// drives every open request, so a crash anywhere resumes on the next run.
// See docs/p7-billing-plan.md, section 6.

import { sql } from "drizzle-orm";
import { newId } from "@convt/license";

import { alert, type BillingContext, fault, one, rows } from "./context";
import { ingestFacts } from "./ingest";
import { emptyFacts } from "./provider";
import { safeError } from "./outbox";

const retryMinutes = [1, 5, 15, 60, 180];

export type DeletionStatus = {
  id: string;
  status: string;
  createdAt: Date;
  lastError: string | null;
};

/** Records the request; repeating it returns the same row. */
export async function requestDeletion(
  ctx: BillingContext,
  userId: string,
): Promise<DeletionStatus> {
  const now = ctx.clock();
  await ctx.db.execute(sql`
    insert into account_deletions (id, user_id, status, next_attempt_at, created_at, updated_at)
    values (${newId("del")}, ${userId}, 'pending', ${now}, ${now}, ${now})
    on conflict (user_id) where status <> 'done' do nothing`);
  const row = (await one<{
    id: string;
    status: string;
    created_at: Date;
    last_error: string | null;
  }>(
    ctx.db,
    sql`select id, status, created_at, last_error from account_deletions where user_id = ${userId} and status <> 'done'`,
  ))!;
  return { id: row.id, status: row.status, createdAt: row.created_at, lastError: row.last_error };
}

const liveSql = (userId: string, now: Date) => sql`
  select id, provider_subscription_id, kind from subscriptions
  where user_id = ${userId} and provider = 'polar' and status not in ('canceled', 'incomplete_expired')
    and (ended_at is null or ended_at > ${now})`;

/** One step for one deletion. Returns its status afterwards. */
export async function advanceDeletion(ctx: BillingContext, deletionId: string): Promise<string> {
  const now = ctx.clock();
  const row = await one<{
    id: string;
    user_id: string | null;
    status: string;
    attempts: number;
    created_at: Date;
    alerted_at: Date | null;
  }>(
    ctx.db,
    sql`select id, user_id, status, attempts, created_at, alerted_at from account_deletions where id = ${deletionId}`,
  );
  if (!row || row.status === "done") return row?.status ?? "missing";
  if (!row.user_id) {
    await ctx.db.execute(
      sql`update account_deletions set status = 'done', finished_at = ${now}, updated_at = ${now} where id = ${row.id}`,
    );
    return "done";
  }
  const userId = row.user_id;
  try {
    // A subscription can exist at the provider before its webhook reaches us, or
    // our record of its checkout can be incomplete: ask the provider for every
    // subscription of this account, and wait while any checkout is still open.
    if ((await ctx.provider.openCheckouts(userId)) > 0)
      throw new Error("waiting for an open checkout to finish or expire");
    const remote = await ctx.provider.customerSubscriptions(userId);
    if (remote.length) {
      const facts = emptyFacts();
      facts.subscriptions.push(...remote);
      await ingestFacts(ctx, facts, "deletion");
    }
    const live = await rows<{ id: string; provider_subscription_id: string; kind: string }>(
      ctx.db,
      liveSql(userId, now),
    );
    if (live.length) {
      await ctx.db.execute(
        sql`update account_deletions set status = 'canceling', updated_at = ${now} where id = ${row.id} and status in ('pending', 'failed', 'canceling')`,
      );
      for (const s of live) {
        if (s.kind === "api") {
          // API usage must be reported before the subscription ends (P9's sender).
          const unreported = await one(
            ctx.db,
            sql`select 1 as x from usage_events where user_id = ${userId} and subscription_id = ${s.id} and kind = 'api_conversion' and reported_at is null limit 1`,
          );
          if (unreported) throw new Error("waiting for API usage to be reported");
        }
        const fact = await ctx.provider.revokeSubscription(s.provider_subscription_id);
        const facts = emptyFacts();
        facts.subscriptions.push(fact);
        await ingestFacts(ctx, facts, "deletion");
        await fault(ctx, "after-revoke");
      }
    }
    const still = await rows(ctx.db, liveSql(userId, now));
    if (still.length) throw new Error(`${still.length} subscription(s) still live after revoking`);
    await ctx.db.execute(
      sql`update account_deletions set status = 'deleting', updated_at = ${now} where id = ${row.id}`,
    );
    await fault(ctx, "before-delete-user");
    await ctx.db.transaction(async (tx) => {
      await tx.execute(sql`select delete_user(${userId}, ${row.id})`);
    });
    return "done";
  } catch (e) {
    const attempts = row.attempts + 1;
    const delay = retryMinutes[Math.min(attempts - 1, retryMinutes.length - 1)] * 60_000;
    const error = safeError(
      (e as { status?: number }).status ?? null,
      (e as Error).name ?? "Error",
      (e as Error).message ?? "",
    );
    // A crash after `deleting` stays there: the next run goes straight to delete_user.
    const updated = await one<{ status: string }>(
      ctx.db,
      sql`
      update account_deletions set status = case when status = 'deleting' then 'deleting' else 'failed' end,
        attempts = ${attempts}, last_error = ${error}, next_attempt_at = ${new Date(now.getTime() + delay)}, updated_at = ${now}
      where id = ${row.id}
      returning status`,
    );
    const status = updated?.status ?? "failed";
    if (!row.alerted_at && now.getTime() - row.created_at.getTime() > 24 * 3600_000) {
      await alert(ctx.db, now, "deletion_stuck", row.id, `still open after 24 hours: ${error}`);
      await ctx.db.execute(
        sql`update account_deletions set alerted_at = ${now} where id = ${row.id}`,
      );
    }
    ctx.log(`[billing] deletion ${row.id}: ${error}`);
    return status;
  }
}

/** The cron step: every open deletion whose next attempt is due. */
export async function runDeletions(ctx: BillingContext) {
  const due = await rows<{ id: string }>(
    ctx.db,
    sql`
    select id from account_deletions where status <> 'done' and next_attempt_at <= ${ctx.clock()} order by created_at limit 20`,
  );
  const out: Record<string, string> = {};
  for (const d of due) out[d.id] = await advanceDeletion(ctx, d.id);
  return out;
}

export async function deletionStatus(ctx: BillingContext, userId: string) {
  return one<{ id: string; status: string; created_at: Date }>(
    ctx.db,
    sql`select id, status, created_at from account_deletions where user_id = ${userId} and status <> 'done'`,
  );
}
