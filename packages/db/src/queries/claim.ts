import { sql } from "drizzle-orm";

import type { Db } from "../client";

export type ClaimResult = {
  orders: number;
  subscriptions: number;
  licenses: number;
  invoices: number;
};

/**
 * Attaches purchases made before the account existed, through the database's
 * `claim_purchases` function (sql/privileges.sql): it locks the user row, reads the
 * current email and whether convt verified it, and claims unclaimed rows with that
 * email only when it is verified. convt_web has no UPDATE on purchases, so this is
 * the only way the site attaches them. Better Auth's user create and update hooks
 * call it, and convt-billing calls the same function after ingest.
 */
export async function claimPurchases(db: Db, userId: string): Promise<ClaimResult> {
  const res = await db.execute<{
    claimed_orders: number;
    claimed_subscriptions: number;
    claimed_licenses: number;
    claimed_invoices: number;
  }>(sql`select * from claim_purchases(${userId})`);
  const row = res.rows[0];
  return {
    orders: Number(row?.claimed_orders ?? 0),
    subscriptions: Number(row?.claimed_subscriptions ?? 0),
    licenses: Number(row?.claimed_licenses ?? 0),
    invoices: Number(row?.claimed_invoices ?? 0),
  };
}
