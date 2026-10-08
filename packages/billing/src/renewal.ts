// Pro renewal for the desktop app (P8). The site authenticates the device token and
// asks for the account's current Pro key through BillingRpc.currentProKey. The key
// is the newest unrevoked paid Pro key, whatever the subscription's status now: a
// lapsed subscription's last key stays valid for every build it covers, and the app
// keeps whichever stored key covers more.

import { and, desc, eq, isNull, sql } from "drizzle-orm";

import { schema as t, type Db } from "@convt/db";
import { rows } from "./context";
import type { BillingContext } from "./context";
import { canStartProTrial } from "./checkout";

export type CurrentProKey = { key: string; updatesUntil: string } | null;

export async function currentProKey(db: Db, userId: string): Promise<CurrentProKey> {
  const [row] = await db
    .select({ key: t.licenses.token, updatesUntil: t.licenses.updatesUntil })
    .from(t.licenses)
    .where(
      and(
        eq(t.licenses.userId, userId),
        eq(t.licenses.plan, "pro"),
        eq(t.licenses.trial, false),
        isNull(t.licenses.revokedAt),
      ),
    )
    .orderBy(desc(t.licenses.updatesUntil), desc(t.licenses.createdAt))
    .limit(1);
  return row ?? null;
}

export type CurrentProAccess =
  | { kind: "pro" }
  | { kind: "trial"; endsOn: string }
  | { kind: "can_start_trial"; checkoutUrl: string }
  | { kind: "lapsed" };

export async function proSubscription(db: Db, userId: string, now: Date) {
  const [live] = await rows<{ status: string; trial_ends_at: Date | null }>(
    db,
    sql`
    select status, trial_ends_at from subscriptions
    where user_id = ${userId} and kind = 'pro'
      and status in ('trialing', 'active', 'past_due', 'unpaid', 'incomplete')
      and (ended_at is null or ended_at > ${now})
    order by created_at desc limit 1`,
  );
  return live ?? null;
}

export async function hasProSubscription(db: Db, userId: string): Promise<boolean> {
  const [row] = await rows<{ x: number }>(
    db,
    sql`
    select 1 as x from subscriptions where user_id = ${userId} and kind = 'pro' limit 1`,
  );
  return !!row;
}

export async function currentProAccess(
  ctx: BillingContext,
  userId: string,
): Promise<CurrentProAccess> {
  const live = await proSubscription(ctx.db, userId, ctx.clock());
  if (live?.status === "trialing" && live.trial_ends_at && live.trial_ends_at > ctx.clock())
    return { kind: "trial", endsOn: live.trial_ends_at.toISOString().slice(0, 10) };
  if (live?.status === "unpaid" || live?.status === "incomplete") return { kind: "lapsed" };
  if (live) return { kind: "pro" };
  const deleting = await rows<{ x: number }>(
    ctx.db,
    sql`select 1 as x from account_deletions where user_id = ${userId} and status <> 'done' limit 1`,
  );
  if (deleting.length) return { kind: "lapsed" };
  if (await hasProSubscription(ctx.db, userId)) return { kind: "lapsed" };
  const eligible = await canStartProTrial(ctx.db, userId, ctx.clock());
  return eligible
    ? { kind: "can_start_trial", checkoutUrl: `${ctx.config.siteUrl}/checkout/pro` }
    : { kind: "lapsed" };
}
