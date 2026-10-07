// Manual Polar order backfill. Pulls every order Polar knows and runs each
// through the same ingest as a webhook, then claim_purchases for matching
// verified emails. Idempotent: a second run writes nothing new.
//
// Default is a dry run that only reports missing rows. Do not point this at
// production from CI or a Worker; Leo runs it by hand after a missed webhook.

import { sql } from "drizzle-orm";

import { type BillingContext, one } from "./context";
import { ingestFacts } from "./ingest";
import { emptyFacts } from "./provider";

export type BackfillMissing = {
  providerOrderId: string;
  email: string | null;
  product: string | null;
  netCents: number;
  status: string;
};

export type BackfillResult = {
  dryRun: boolean;
  scanned: number;
  missing: BackfillMissing[];
  created: number;
  alreadyPresent: number;
  rejected: number;
  claimedUsers: number;
  claimedOrders: number;
  claimedLicenses: number;
  errors: number;
};

const pageSize = 100;

/**
 * Pages Polar orders and creates any Desktop (or other catalog) rows ingest
 * accepts. Dry-run (`dryRun: true`, the default) writes nothing.
 */
export async function backfillPolarOrders(
  ctx: BillingContext,
  opts: { dryRun?: boolean } = {},
): Promise<BackfillResult> {
  const dryRun = opts.dryRun !== false;
  const out: BackfillResult = {
    dryRun,
    scanned: 0,
    missing: [],
    created: 0,
    alreadyPresent: 0,
    rejected: 0,
    claimedUsers: 0,
    claimedOrders: 0,
    claimedLicenses: 0,
    errors: 0,
  };
  const emails = new Set<string>();
  for (let page = 1; page <= 200; page++) {
    const r = await ctx.provider.scan("orders", { page, limit: pageSize });
    for (const o of r.facts.orders) {
      out.scanned++;
      if (o.email) emails.add(o.email);
      const stored = await one<{ id: string }>(
        ctx.db,
        sql`select id from orders where provider = 'polar' and provider_order_id = ${o.providerOrderId}`,
      );
      if (stored) {
        out.alreadyPresent++;
        if (!dryRun) {
          try {
            const again = await ingestFacts(ctx, { ...emptyFacts(), orders: [o] }, "backfill");
            if (again.rejected) out.rejected++;
          } catch (e) {
            ctx.log(`[billing] backfill ${o.providerOrderId}: ${(e as Error).message}`);
            out.errors++;
          }
        }
        continue;
      }
      out.missing.push({
        providerOrderId: o.providerOrderId,
        email: o.email,
        product: o.product,
        netCents: o.netCents,
        status: o.status,
      });
      if (dryRun) continue;
      try {
        const applied = await ingestFacts(ctx, { ...emptyFacts(), orders: [o] }, "backfill");
        if (applied.rejected) {
          out.rejected++;
          ctx.log(`[billing] backfill rejected ${o.providerOrderId}: ${applied.rejected}`);
        } else {
          out.created++;
        }
      } catch (e) {
        ctx.log(`[billing] backfill ${o.providerOrderId}: ${(e as Error).message}`);
        out.errors++;
      }
    }
    if (page >= r.maxPage) break;
  }
  if (!dryRun) {
    for (const email of [...emails].sort()) {
      const u = await one<{ id: string }>(
        ctx.db,
        sql`select id from users where email = ${email} and email_verified`,
      );
      if (!u) continue;
      const claimed = await one<{
        claimed_orders: number;
        claimed_licenses: number;
      }>(ctx.db, sql`select * from claim_purchases(${u.id})`);
      out.claimedUsers++;
      out.claimedOrders += Number(claimed?.claimed_orders ?? 0);
      out.claimedLicenses += Number(claimed?.claimed_licenses ?? 0);
    }
  }
  return out;
}
