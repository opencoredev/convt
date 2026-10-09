// The marketing backfill (bun run marketing:backfill). Accounts created before
// marketing email existed have no subscription row; this enrolls them through
// enroll_marketing, which never touches an account that already has a row, so an
// unsubscribe always survives. It writes rows only: convt-billing's cron pushes
// them to Sequenzy. Runs as convt_owner.

import { sql } from "drizzle-orm";

import type { Db } from "../client";

export type MarketingBackfillPlan = {
  /** Accounts with no row, oldest first. Ids only: the plan is printed. */
  missing: Array<{ userId: string; createdAt: Date; verified: boolean }>;
  /** Existing rows by status and sync state, e.g. `subscribed/synced`. */
  existing: Record<string, number>;
};

export async function planMarketingBackfill(db: Db): Promise<MarketingBackfillPlan> {
  const missing = await db.execute<{
    id: string;
    created_at: Date | string;
    email_verified: boolean;
  }>(sql`
    select u.id, u.created_at, u.email_verified from users u
    where not exists (select 1 from marketing_subscriptions m where m.user_id = u.id)
    order by u.created_at, u.id`);
  const existing = await db.execute<{ key: string; n: string | number }>(sql`
    select status || '/' || sync_state as key, count(*) as n from marketing_subscriptions
    group by 1 order by 1`);
  return {
    missing: missing.rows.map((r) => ({
      userId: r.id,
      createdAt: new Date(r.created_at),
      verified: r.email_verified,
    })),
    existing: Object.fromEntries(existing.rows.map((r) => [r.key, Number(r.n)])),
  };
}

/**
 * Enrolls every account without a row. With `resync`, also queues every
 * subscribed row to be pushed again, which refreshes the attributes campaigns
 * are segmented on. Returns how many rows each step touched.
 */
export async function applyMarketingBackfill(
  db: Db,
  options: { resync: boolean },
): Promise<{ enrolled: number; resynced: number }> {
  return db.transaction(async (tx) => {
    const enrolled = await tx.execute<{ n: string | number }>(sql`
      select count(*) filter (where enroll_marketing(u.id, 'backfill')) as n from users u
      where not exists (select 1 from marketing_subscriptions m where m.user_id = u.id)`);
    let resynced = 0;
    if (options.resync) {
      const r = await tx.execute(sql`
        update marketing_subscriptions
          set sync_state = 'pending', sync_attempts = 0, next_sync_at = now(), last_error = null,
            updated_at = now()
          where status = 'subscribed' and sync_state <> 'pending'`);
      resynced = r.rowCount ?? 0;
    }
    return { enrolled: Number(enrolled.rows[0]?.n ?? 0), resynced };
  });
}
