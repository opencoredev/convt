import { sql } from "drizzle-orm";

import type { Db } from "../client";

/**
 * Counts one use of a send bucket and returns the count in the current window. One
 * statement, so concurrent requests cannot both read a stale count: it inserts the
 * bucket, restarts it when its window has expired, or increments it.
 */
export async function consumeSendBucket(
  db: Db,
  key: string,
  windowMs: number,
  now: Date,
): Promise<number> {
  const expires = new Date(now.getTime() + windowMs);
  const res = await db.execute<{ count: number }>(sql`
    insert into otp_send_limits (key, window_start, count, expires_at)
    values (${key}, ${now}, 1, ${expires})
    on conflict (key) do update set
      window_start = case when otp_send_limits.expires_at <= ${now} then ${now} else otp_send_limits.window_start end,
      count = case when otp_send_limits.expires_at <= ${now} then 1 else otp_send_limits.count + 1 end,
      expires_at = case when otp_send_limits.expires_at <= ${now} then ${expires} else otp_send_limits.expires_at end
    returning count`);
  return Number(res.rows[0].count);
}

/**
 * Ends a send bucket's window now, so the next consumeSendBucket starts it again at 1.
 * For a bucket counted before work that then failed.
 */
export async function releaseSendBucket(db: Db, key: string, now: Date): Promise<void> {
  await db.execute(sql`update otp_send_limits set expires_at = ${now} where key = ${key}`);
}
