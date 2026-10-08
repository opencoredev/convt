// Manual Polar order backfill. Pulls every order Polar knows and runs each
// through the same ingest as a webhook, then claim_purchases for matching
// verified emails. Idempotent: a second run writes nothing new.
//
// Default is a dry run that only reports missing rows. Do not point this at
// production from CI or a Worker; Leo runs it by hand after a missed webhook.

import { sql } from "drizzle-orm";

import { type BillingContext, one } from "./context";
import { ingestFacts } from "./ingest";
import { emptyFacts, type DisputeFact, type OrderFact, type ScanKind } from "./provider";

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
  skipped: number;
  rejected: number;
  claimedUsers: number;
  claimedOrders: number;
  claimedLicenses: number;
  errors: number;
};

const pageSize = 100;
export const maxBackfillPages = 10_000;

/** Unpaid, refunded, void, or a dispute that is not won/prevented. */
export function backfillSkipReason(
  status: string,
  disputes: Array<{ status: string }>,
): "unpaid" | "refunded" | "void" | "disputed" | null {
  if (status === "draft" || status === "pending") return "unpaid";
  if (status === "refunded") return "refunded";
  if (status === "void") return "void";
  if (disputes.some((d) => d.status !== "won" && d.status !== "prevented")) return "disputed";
  return null;
}

async function scanKind(ctx: BillingContext, kind: ScanKind) {
  const pages = [];
  for (let page = 1; ; page++) {
    if (page > maxBackfillPages) {
      throw new Error(
        `billing backfill: ${kind} still has pages after ${maxBackfillPages}; refusing to stop silently`,
      );
    }
    const r = await ctx.provider.scan(kind, { page, limit: pageSize });
    pages.push(r);
    if (page >= r.maxPage) break;
  }
  return pages;
}

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
    skipped: 0,
    rejected: 0,
    claimedUsers: 0,
    claimedOrders: 0,
    claimedLicenses: 0,
    errors: 0,
  };
  const emails = new Set<string>();
  const disputesByOrder = new Map<string, DisputeFact[]>();
  for (const page of await scanKind(ctx, "disputes")) {
    for (const d of page.facts.disputes) {
      const list = disputesByOrder.get(d.providerOrderId) ?? [];
      list.push(d);
      disputesByOrder.set(d.providerOrderId, list);
    }
  }
  for (const page of await scanKind(ctx, "orders")) {
    for (const o of page.facts.orders) {
      out.scanned++;
      if (o.email) emails.add(o.email);
      const disputes = disputesByOrder.get(o.providerOrderId) ?? [];
      const stored = await one<{ id: string }>(
        ctx.db,
        sql`
          select id from orders where provider = 'polar' and provider_order_id = ${o.providerOrderId}
          union all
          select id from invoices where provider = 'polar' and provider_invoice_id = ${o.providerOrderId}
          limit 1`,
      );
      if (stored) {
        out.alreadyPresent++;
        if (!dryRun) await applyOrder(ctx, out, o, disputes, false);
        continue;
      }
      if (backfillSkipReason(o.status, disputes)) {
        out.skipped++;
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
      await applyOrder(ctx, out, o, disputes, true);
    }
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

async function applyOrder(
  ctx: BillingContext,
  out: BackfillResult,
  o: OrderFact,
  disputes: DisputeFact[],
  countCreate: boolean,
) {
  try {
    const applied = await ingestFacts(ctx, { ...emptyFacts(), orders: [o], disputes }, "backfill");
    if (applied.rejected) {
      out.rejected++;
      ctx.log(`[billing] backfill rejected ${o.providerOrderId}: ${applied.rejected}`);
    } else if (countCreate) {
      out.created++;
    }
  } catch (e) {
    ctx.log(`[billing] backfill ${o.providerOrderId}: ${(e as Error).message}`);
    out.errors++;
  }
}
