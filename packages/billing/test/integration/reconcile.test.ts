// The reconciler keeps entitlements right when webhooks never arrive.

import { afterAll, beforeAll, describe, expect, test } from "bun:test";
import { sql } from "drizzle-orm";

import {
  reconcileDaily,
  reconcileFrequent,
  setPageLimitForTests,
  sweep,
} from "../../src/reconcile";
import { createHarness, type Harness } from "../../src/testing";

let h: Harness;
beforeAll(async () => {
  h = await createHarness();
});
afterAll(async () => h?.close());

const run = (only?: Parameters<typeof reconcileFrequent>[1]) =>
  h.service.withCtx((c) => reconcileFrequent(c, only));

describe("reconciler", () => {
  test("with every webhook dropped, one run issues the missing license and its email", async () => {
    const b = await h.buy("desktop", null, { email: "dropped@convt.test" });
    h.mock.takeHeld(); // dropped
    await run();
    const lic = await h.q<{ id: string }>(
      sql`select l.id from licenses l join orders o on o.id = l.order_id where o.checkout_id = ${b.checkoutId}`,
    );
    expect(lic.length).toBe(1);
    expect(h.mock.resend.sent.filter((m) => m.to[0] === "dropped@convt.test").length).toBe(1);
    const [runRow] = await h.q<{ status: string }>(
      sql`select status from reconcile_runs order by started_at desc limit 1`,
    );
    expect(runRow.status).toBe("ok");
  });

  test("a run racing the webhook issues once", async () => {
    for (let i = 0; i < 5; i++) {
      const b = await h.buy("desktop", null, { email: `race${i}@convt.test` });
      const held = h.mock.takeHeld();
      await Promise.all([run(["orders"]), ...held.map((d) => h.deliver(d)), run(["orders"])]);
      const lic = await h.q(
        sql`select 1 from licenses l join orders o on o.id = l.order_id where o.checkout_id = ${b.checkoutId}`,
      );
      expect(lic.length).toBe(1);
      const mails = await h.q(
        sql`select 1 from email_outbox where to_email = ${`race${i}@convt.test`}`,
      );
      expect(mails.length).toBe(1);
    }
  });

  test("the refund scan finds a refund of a 200-day-old order", async () => {
    const b = await h.buy("desktop", null, { email: "old@convt.test" });
    await h.deliverAll();
    h.mock.advance(200 * 86_400_000);
    const order = h.mock.state().orders.find((o) => o.checkout_id === b.providerCheckoutId)!;
    h.mock.refund(order.id);
    h.mock.takeHeld(); // the refund's webhooks are dropped
    await run(["refunds"]);
    const [lic] = await h.q<{ revoke_reason: string | null }>(
      sql`select l.revoke_reason from licenses l join orders o on o.id = l.order_id where o.provider_order_id = ${order.id}`,
    );
    expect(lic.revoke_reason).toBe("refunded");
  });

  test("the dispute scan finds a lost dispute", async () => {
    const b = await h.buy("desktop", null, { email: "lostscan@convt.test" });
    await h.deliverAll();
    const order = h.mock.state().orders.find((o) => o.checkout_id === b.providerCheckoutId)!;
    const d = h.mock.openDispute(order.id);
    await run(["disputes"]);
    let [lic] = await h.q<{ revoke_reason: string | null }>(
      sql`select l.revoke_reason from licenses l join orders o on o.id = l.order_id where o.provider_order_id = ${order.id}`,
    );
    expect(lic.revoke_reason).toBeNull();
    h.mock.closeDispute(d.id, "lost");
    // Open disputes are re-fetched each run.
    await run(["refetch"]);
    [lic] = await h.q<{ revoke_reason: string | null }>(
      sql`select l.revoke_reason from licenses l join orders o on o.id = l.order_id where o.provider_order_id = ${order.id}`,
    );
    expect(lic.revoke_reason).toBe("dispute_lost");
    expect(
      (
        await h.q(
          sql`select 1 from billing_alerts where kind = 'dispute_lost' and subject = ${d.id}`,
        )
      ).length,
    ).toBe(1);
  });

  test("a dispute that fails to ingest is retried, not skipped by the cursor", async () => {
    setPageLimitForTests(1);
    try {
      await run(["disputes"]); // the cursor reaches the current end
      // Two new disputes on orders we never heard of; fetching the first one's order fails once.
      const one = await h.buy("desktop", null, { email: "cursor1@convt.test" });
      const two = await h.buy("desktop", null, { email: "cursor2@convt.test" });
      h.mock.takeHeld();
      const o1 = h.mock.state().orders.find((o) => o.checkout_id === one.providerCheckoutId)!;
      const o2 = h.mock.state().orders.find((o) => o.checkout_id === two.providerCheckoutId)!;
      const d1 = h.mock.openDispute(o1.id);
      h.mock.closeDispute(d1.id, "lost");
      const d2 = h.mock.openDispute(o2.id);
      h.mock.failApi("GET", new RegExp(`^/v1/orders/${o1.id}$`), 500, 1);
      await run(["disputes"]);
      expect(
        (await h.q(sql`select 1 from disputes where provider_dispute_id = ${d1.id}`)).length,
      ).toBe(0);
      await run(["disputes"]);
      const [row] = await h.q<{ status: string }>(
        sql`select status from disputes where provider_dispute_id = ${d1.id}`,
      );
      expect(row.status).toBe("lost");
      expect(
        (await h.q(sql`select 1 from disputes where provider_dispute_id = ${d2.id}`)).length,
      ).toBe(1);
    } finally {
      setPageLimitForTests(100);
    }
  });

  test("the full sweep resumes from its stored page after a restart and wraps", async () => {
    for (let i = 0; i < 4; i++) await h.buy("desktop", null, { email: `sweep${i}@convt.test` });
    h.mock.takeHeld();
    setPageLimitForTests(2);
    try {
      const total = h.mock.state().orders.length;
      const maxPage = Math.ceil(total / 2);
      // One order page per run (pages = 2 covers orders and subscriptions once each).
      await h.service.withCtx((c) => sweep(c, 2));
      const [c1] = await h.q<{ page: number }>(
        sql`select page from reconcile_cursors where name = 'sweep:orders'`,
      );
      expect(Number(c1.page)).toBe(2);
      // A "restart": a fresh context reads the cursor from the database.
      await h.service.withCtx((c) => sweep(c, 2));
      const [c2] = await h.q<{ page: number }>(
        sql`select page from reconcile_cursors where name = 'sweep:orders'`,
      );
      expect(Number(c2.page)).toBe(3);
      const [p0] = await h.q<{ t: string }>(
        sql`select pass_started_at::text as t from reconcile_cursors where name = 'sweep:orders'`,
      );
      for (let i = 0; i < maxPage; i++) await h.service.withCtx((c) => sweep(c, 2));
      const [c3] = await h.q<{ page: number; t: string }>(
        sql`select page, pass_started_at::text as t from reconcile_cursors where name = 'sweep:orders'`,
      );
      expect(Number(c3.page)).toBeLessThanOrEqual(maxPage);
      expect(c3.t > p0.t).toBe(true);
      // The sweep found the four orders whose webhooks were dropped.
      const missing = await h.q(sql`select 1 from orders where email like 'sweep%@convt.test'`);
      expect(missing.length).toBe(4);
    } finally {
      setPageLimitForTests(100);
    }
  });

  test("daily: catalog drift, a fractional metered price and settings drift are reported, digested and emailed", async () => {
    h.mock.products.pro_month.amount = 1300;
    h.mock.products.api.unitAmount = "0.5";
    h.mock.settings.allowMultipleSubscriptions = false;
    h.mock.settings.emails.subscription_trial_conversion_reminder = true;
    try {
      const r = await h.service.withCtx((c) => reconcileDaily(c));
      const kinds = r.found.map((f) => `${f.kind}:${f.subject}`);
      expect(kinds).toContain("catalog_drift:pro_month");
      expect(r.found.find((f) => f.subject === "api")?.detail).toMatch(
        /not a whole number of cents/,
      );
      expect(kinds).toContain("settings:allow_multiple_subscriptions");
      expect(kinds).toContain("settings:subscription_trial_conversion_reminder");
      const digest = h.mock.resend.sent.find((m) => m.to[0] === "alerts@convt.test");
      expect(digest?.subject).toMatch(/convt billing/);
      expect(digest?.text).toContain("catalog_drift");
      // No keys or bodies in the digest.
      expect(digest?.text).not.toMatch(/eyJ[A-Za-z0-9_-]{20,}/);
    } finally {
      h.mock.products.pro_month.amount = 1200;
      h.mock.products.api.unitAmount = "1";
      h.mock.settings.allowMultipleSubscriptions = true;
      h.mock.settings.emails.subscription_trial_conversion_reminder = false;
    }
  });

  test("daily invariants hold after all of the above", async () => {
    const r = await h.service.withCtx((c) => reconcileDaily(c));
    expect(r.found.filter((f) => f.kind === "invariant")).toEqual([]);
  });
});
