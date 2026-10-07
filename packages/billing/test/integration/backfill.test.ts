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
});
