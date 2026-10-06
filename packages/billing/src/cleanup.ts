// Bounded sign-in housekeeping without convt-server. Each statement commits on
// its own: code issuance locks the marker before the code, so cleanup must too.
import { sql } from "drizzle-orm";

import type { BillingContext } from "./context";

const batchSize = 1000;
const rateLimitKeepMs = 86_400_000;

export async function cleanupAuth(c: BillingContext) {
  const now = c.clock();
  const markers = await c.db.execute(sql`
    delete from verifications where id in (
      select id from verifications where identifier like 'otp-issued:%' and expires_at < ${now}
      order by expires_at, id limit ${batchSize}
    )`);
  const codes = await c.db.execute(sql`
    delete from verifications where id in (
      select id from verifications where identifier not like 'otp-issued:%' and expires_at < ${now}
      order by expires_at, id limit ${batchSize}
    )`);
  const rates = await c.db.execute(sql`
    delete from rate_limits where id in (
      select id from rate_limits where last_request < ${now.getTime() - rateLimitKeepMs}
      order by last_request, id limit ${batchSize}
    )`);
  const sends = await c.db.execute(sql`
    delete from otp_send_limits where key in (
      select key from otp_send_limits where expires_at < ${now}
      order by expires_at, key limit ${batchSize}
    )`);
  return {
    verifications: (markers.rowCount ?? 0) + (codes.rowCount ?? 0),
    rateLimits: rates.rowCount ?? 0,
    otpSendLimits: sends.rowCount ?? 0,
  };
}
