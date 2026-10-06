import { and, eq, isNull, sql } from "drizzle-orm";
import { apiKeyPrefix, generateApiKey, hashApiKey, newId } from "@convt/license";
import type { Db } from "../client";
import { apiKeys } from "../schema";

export async function createApiKey(
  db: Db,
  userId: string,
  name: string,
): Promise<{ id: string; key: string }> {
  return db.transaction(async (tx) => {
    const enrolled = await tx.execute(
      sql`select id from subscriptions where user_id=${userId} and kind='api' and status='active' and card_seen_at is not null and (current_period_start is null or current_period_start<=now()) and (current_period_end>now() or (kind='api' and current_period_end is null)) and (ended_at is null or ended_at>now()) order by created_at desc limit 1 for update`,
    );
    if (!enrolled.rows.length) throw new Error("Add a payment method under API billing first.");
    const count = await tx.execute<{ n: string }>(
      sql`select count(*)::text as n from api_keys where user_id=${userId} and revoked_at is null`,
    );
    if (Number(count.rows[0].n) >= 20)
      throw new Error("Revoke a key before creating another. You can keep 20 active keys.");
    const key = generateApiKey();
    const id = newId("key");
    await tx
      .insert(apiKeys)
      .values({ id, userId, name, prefix: apiKeyPrefix(key), secretHash: await hashApiKey(key) });
    return { id, key };
  });
}
export async function revokeApiKey(db: Db, userId: string, id: string): Promise<boolean> {
  const rows = await db
    .update(apiKeys)
    .set({ revokedAt: new Date(), updatedAt: new Date() })
    .where(and(eq(apiKeys.id, id), eq(apiKeys.userId, userId), isNull(apiKeys.revokedAt)))
    .returning({ id: apiKeys.id });
  return rows.length === 1;
}
export async function cloudAllowance(db: Db, userId: string, kind: "api" | "pro") {
  const sub = await db.execute<{
    id: string;
    status: string;
    usable: boolean;
    cap: number;
    period: string;
  }>(
    sql`select id,status,coalesce((status='active' and (ended_at is null or ended_at>now()) and (current_period_start is null or current_period_start<=now()) and (current_period_end>now() or (kind='api' and current_period_end is null)) and (kind='pro' or card_seen_at is not null)),false) as usable,coalesce(spend_cap_cents,0) as cap,case when kind='pro' then (date_trunc('month',now() at time zone 'UTC') at time zone 'UTC') else coalesce(current_period_start,(date_trunc('month',now() at time zone 'UTC') at time zone 'UTC')) end as period from subscriptions where user_id=${userId} and kind=${kind} order by created_at desc limit 1`,
  );
  const row = sub.rows[0];
  if (!row)
    return {
      allowed: false,
      state: "none",
      used: 0,
      reserved: 0,
      limit: kind === "pro" ? 50_000_000_000 : 0,
      period: null,
    };
  const usage = await db.execute<{ used: string }>(
    sql`select coalesce(sum(case when ${kind}='api' then e.amount_cents else e.quantity end),0)::text as used from usage_events e left join usage_events original on original.id=e.corrects where e.subscription_id=${row.id} and e.occurred_at>=${row.period}::timestamptz and (e.kind=${kind === "api" ? "api_conversion" : "pro_bytes"} or (e.kind='correction' and original.kind=${kind === "api" ? "api_conversion" : "pro_bytes"}))`,
  );
  const reservations = await db.execute<{ reserved: string }>(
    sql`select coalesce(sum(case when ${kind}='api' then reserved_cents else reserved_bytes end),0)::text as reserved from cloud_jobs where subscription_id=${row.id} and reservation='open'`,
  );
  return {
    allowed: row.usable,
    state: row.status,
    used: Number(usage.rows[0].used),
    reserved: Number(reservations.rows[0].reserved),
    limit: kind === "pro" ? 50_000_000_000 : row.cap,
    period: row.period,
  };
}
