import { describe, expect, test } from "bun:test";

import { backfillPolarOrders, backfillSkipReason, maxBackfillPages } from "../../src/backfill";
import { emptyFacts, type OrderFact, type ScanKind } from "../../src/provider";

describe("backfill skip reasons", () => {
  test("skips unpaid, refunded, void, and open or lost disputes", () => {
    expect(backfillSkipReason("draft", [])).toBe("unpaid");
    expect(backfillSkipReason("pending", [])).toBe("unpaid");
    expect(backfillSkipReason("refunded", [])).toBe("refunded");
    expect(backfillSkipReason("void", [])).toBe("void");
    expect(backfillSkipReason("paid", [{ status: "lost" }])).toBe("disputed");
    expect(backfillSkipReason("paid", [{ status: "needs_response" }])).toBe("disputed");
    expect(backfillSkipReason("paid", [{ status: "under_review" }])).toBe("disputed");
    expect(backfillSkipReason("paid", [])).toBeNull();
    expect(backfillSkipReason("paid", [{ status: "won" }])).toBeNull();
    expect(backfillSkipReason("partially_refunded", [{ status: "prevented" }])).toBeNull();
  });
});

describe("backfill paging", () => {
  const order = (id: string): OrderFact =>
    ({
      kind: "order",
      providerOrderId: id,
      providerCustomerId: "cus_x",
      providerCheckoutId: null,
      checkoutRef: null,
      providerSubscriptionId: null,
      userId: null,
      email: null,
      product: "desktop",
      providerProductId: "prod_desktop",
      reason: "purchase",
      status: "paid",
      subtotalCents: 2900,
      discountCents: 0,
      discountId: null,
      netCents: 2900,
      appliedBalanceCents: 0,
      refundedCents: 0,
      currency: "usd",
      billedAt: "2026-10-07T00:00:00Z",
      description: "Desktop",
      items: [],
      subscription: null,
      version: "2026-10-07T00:00:00Z",
      hash: id,
    }) as OrderFact;

  test("continues past 200 pages when Polar still has more", async () => {
    const last = 201;
    const scans: Array<{ kind: ScanKind; page: number }> = [];
    const ctx = {
      db: { execute: async () => ({ rows: [] }) },
      provider: {
        scan: async (kind: ScanKind, from: { page: number }) => {
          scans.push({ kind, page: from.page });
          if (kind === "disputes") {
            return { facts: emptyFacts(), page: from.page, maxPage: 1 };
          }
          return {
            facts: { ...emptyFacts(), orders: [order(`ord_${from.page}`)] },
            page: from.page,
            maxPage: last,
          };
        },
      },
      log: () => undefined,
    };
    const result = await backfillPolarOrders(ctx as never, { dryRun: true });
    expect(result.scanned).toBe(last);
    expect(result.missing).toHaveLength(last);
    expect(scans.filter((s) => s.kind === "orders")).toHaveLength(last);
    expect(last).toBeGreaterThan(200);
  });

  test("fails loudly instead of stopping when Polar exceeds the hard page cap", async () => {
    const ctx = {
      db: { execute: async () => ({ rows: [] }) },
      provider: {
        scan: async (kind: ScanKind, from: { page: number }) => {
          if (kind === "disputes") {
            return { facts: emptyFacts(), page: from.page, maxPage: 1 };
          }
          return {
            facts: { ...emptyFacts(), orders: [order(`ord_${from.page}`)] },
            page: from.page,
            maxPage: maxBackfillPages + 1,
          };
        },
      },
      log: () => undefined,
    };
    await expect(backfillPolarOrders(ctx as never, { dryRun: true })).rejects.toThrow(
      /refusing to stop silently/,
    );
  });
});
