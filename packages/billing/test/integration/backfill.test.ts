// Manual Polar backfill: missing $0 Desktop orders, claim by email, idempotent.

import { afterAll, beforeAll, describe, expect, test } from "bun:test";
import { sql } from "drizzle-orm";

import { backfillPolarOrders } from "../../src/backfill";
import { createHarness, type Harness } from "../../src/testing";
import { testMailbox } from "../mailbox";

let h: Harness;
beforeAll(async () => {
  h = await createHarness();
});
afterAll(async () => h?.close());

const run = (dryRun: boolean) => h.service.withCtx((c) => backfillPolarOrders(c, { dryRun }));

describe("Polar order backfill", () => {
  test("dry-run lists a dropped $0 Desktop order; apply creates it and is idempotent", async () => {
    const email = testMailbox("backfill-zero");
    const u = await h.user(email);
    const raw = h.mock.craftOrder({
      externalCustomerId: null,
      email,
      product: "desktop",
      reason: "purchase",
      checkoutId: null,
      items: [{ amount: 2900, priceId: "price_local_desktop" }],
      discountAmount: 2900,
    });
    h.mock.takeHeld();
    const dry = await run(true);
    expect(dry.dryRun).toBe(true);
    expect(dry.missing.some((m) => m.providerOrderId === raw.id && m.netCents === 0)).toBe(true);
    const before = await h.q(sql`select 1 from orders where provider_order_id = ${raw.id}`);
    expect(before.length).toBe(0);

    const first = await run(false);
    expect(first.dryRun).toBe(false);
    expect(first.created).toBeGreaterThanOrEqual(1);
    const [ord] = await h.q<{ user_id: string; amount_cents: number }>(
      sql`select user_id, amount_cents from orders where provider_order_id = ${raw.id}`,
    );
    expect(ord).toEqual({ user_id: u.id, amount_cents: 0 });
    const lics = await h.q(
      sql`select 1 from licenses l join orders o on o.id = l.order_id where o.provider_order_id = ${raw.id} and l.user_id = ${u.id}`,
    );
    expect(lics.length).toBe(1);

    const second = await run(false);
    expect(second.created).toBe(0);
    expect(second.alreadyPresent).toBeGreaterThanOrEqual(1);
    const again = await h.q(
      sql`select 1 from licenses l join orders o on o.id = l.order_id where o.provider_order_id = ${raw.id}`,
    );
    expect(again.length).toBe(1);
  });

  test("skips unpaid, refunded, and disputed orders so they do not get a license", async () => {
    const unpaid = h.mock.craftOrder({
      externalCustomerId: null,
      email: testMailbox("backfill-unpaid"),
      product: "desktop",
      reason: "purchase",
      checkoutId: null,
      items: [{ amount: 2900, priceId: "price_local_desktop" }],
      status: "pending",
    });
    const refunded = h.mock.craftOrder({
      externalCustomerId: null,
      email: testMailbox("backfill-refunded"),
      product: "desktop",
      reason: "purchase",
      checkoutId: null,
      items: [{ amount: 2900, priceId: "price_local_desktop" }],
      status: "refunded",
    });
    const disputed = h.mock.craftOrder({
      externalCustomerId: null,
      email: testMailbox("backfill-disputed"),
      product: "desktop",
      reason: "purchase",
      checkoutId: null,
      items: [{ amount: 2900, priceId: "price_local_desktop" }],
    });
    const d = h.mock.openDispute(disputed.id);
    h.mock.closeDispute(d.id, "lost");
    h.mock.takeHeld();

    const dry = await run(true);
    expect(dry.missing.some((m) => m.providerOrderId === unpaid.id)).toBe(false);
    expect(dry.missing.some((m) => m.providerOrderId === refunded.id)).toBe(false);
    expect(dry.missing.some((m) => m.providerOrderId === disputed.id)).toBe(false);
    expect(dry.skipped).toBeGreaterThanOrEqual(3);

    const applied = await run(false);
    expect(applied.skipped).toBeGreaterThanOrEqual(3);
    const lics = await h.q(
      sql`select o.provider_order_id from licenses l join orders o on o.id = l.order_id
          where o.provider_order_id = ${unpaid.id}
             or o.provider_order_id = ${refunded.id}
             or o.provider_order_id = ${disputed.id}`,
    );
    expect(lics.length).toBe(0);
  });

  test("an invoice row counts as already present so Pro orders are not re-created", async () => {
    const email = testMailbox("backfill-invoice");
    const u = await h.user(email);
    const bought = await h.buy("pro_month", u);
    await h.deliverAll();
    const [inv] = await h.q<{ provider_invoice_id: string }>(
      sql`select provider_invoice_id from invoices where user_id = ${u.id} limit 1`,
    );
    expect(inv?.provider_invoice_id).toBeTruthy();
    const dry = await run(true);
    expect(dry.missing.some((m) => m.providerOrderId === inv.provider_invoice_id)).toBe(false);
    expect(dry.alreadyPresent).toBeGreaterThanOrEqual(1);
    void bought;
  });

  test("a claimed guest Desktop order opens the portal with the Polar customer id", async () => {
    const email = testMailbox("portal-guest");
    const u = await h.user(email);
    const bought = await h.buy("desktop", null, { email });
    await h.deliverAll();
    await h.q(sql`select * from claim_purchases(${u.id})`);
    const url = await h.service.portalUrl(u.id);
    expect(url).toBeTruthy();
    expect(new URL(url!).pathname).toContain("/portal/");
    const [ord] = await h.q<{ provider_customer_id: string }>(
      sql`select provider_customer_id from orders where user_id = ${u.id} limit 1`,
    );
    expect(ord?.provider_customer_id).toBeTruthy();
    void bought;
  });
});
