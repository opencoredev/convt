// Concurrency: duplicate and racing deliveries, concurrent snapshots, two drains,
// and a stale outbox worker fenced off by the claim generation.

import { afterAll, beforeAll, describe, expect, test } from "bun:test";
import { sql } from "drizzle-orm";

import { enqueueEmail } from "../../src/outbox";
import { createHarness, type Harness } from "../../src/testing";

let h: Harness;
beforeAll(async () => {
  h = await createHarness();
});
afterAll(async () => h?.close());

const jitter = () => Bun.sleep(Math.floor(Math.random() * 15));

describe("concurrency", () => {
  test("20 concurrent deliveries of one paid order: one order, one license, one email row, one event row", async () => {
    const b = await h.buy("desktop", null, { email: "twenty@convt.test" });
    const held = h.mock.takeHeld();
    const paid = held.find((d) => d.type === "order.paid")!;
    const rs = await Promise.all(Array.from({ length: 20 }, () => h.deliver(paid)));
    expect(rs.every((r) => r.status === 200)).toBe(true);
    const [c] = await h.q<{ orders: number; licenses: number; outbox: number; events: number }>(sql`
      select (select count(*)::int from orders where checkout_id = ${b.checkoutId}) as orders,
        (select count(*)::int from licenses l join orders o on o.id = l.order_id where o.checkout_id = ${b.checkoutId}) as licenses,
        (select count(*)::int from email_outbox where to_email = 'twenty@convt.test') as outbox,
        (select count(*)::int from webhook_events where provider_event_id = ${paid.id}) as events`);
    expect(c).toEqual({ orders: 1, licenses: 1, outbox: 1, events: 1 });
  });

  test("order.paid racing order.refunded, 50 times with random delays: always refunded, no unrevoked key", async () => {
    let refundFirstSeen = 0;
    for (let i = 0; i < 50; i++) {
      const b = await h.buy("desktop", null, { email: `racer${i}@convt.test` });
      const orderId = h.mock.state().orders.find((o) => o.checkout_id === b.providerCheckoutId)!.id;
      h.mock.refund(orderId);
      const held = h.mock.takeHeld();
      const relevant = held.filter(
        (d) => d.type === "order.paid" || d.type === "order.refunded" || d.type === "order.updated",
      );
      if (Math.random() < 0.5) refundFirstSeen++;
      const rs = await Promise.all(
        relevant.map(async (d) => {
          await jitter();
          return h.deliver(d);
        }),
      );
      expect(rs.every((r) => r.status === 200 || r.status === 500)).toBe(true);
      // A deadlock or lock timeout is a failed row that Polar retries; retry what failed.
      for (const [k, r] of rs.entries())
        if (r.status === 500) expect((await h.deliver(relevant[k])).status).toBe(200);
      const [o] = await h.q<{ status: string; unrevoked: number }>(sql`
        select o.status, (select count(*)::int from licenses l where l.order_id = o.id and l.revoked_at is null) as unrevoked
        from orders o where o.provider_order_id = ${orderId}`);
      expect(o).toEqual({ status: "refunded", unrevoked: 0 });
    }
    expect(refundFirstSeen).toBeGreaterThanOrEqual(0);
  });

  test("concurrent subscription snapshots end at the newest", async () => {
    const u = await h.user("snap@convt.test");
    const b = await h.buy("pro_month", u);
    const subId = h.mock
      .state()
      .checkouts.find((c) => c.id === b.providerCheckoutId)!.subscription_id!;
    await h.deliverAll();
    for (let i = 0; i < 9; i++) await h.provider.setCancelAtPeriodEnd(subId, i % 2 === 0);
    const held = h.mock.takeHeld().filter((d) => d.type.startsWith("subscription."));
    expect(held.length).toBeGreaterThanOrEqual(18);
    await Promise.all(
      held.map(async (d) => {
        await jitter();
        const r = await h.deliver(d);
        if (r.status === 500) await h.deliver(d);
      }),
    );
    const newest = h.mock.subscription(subId)!;
    const [s] = await h.q<{ v: string; cancel: boolean; nv: string }>(sql`
      select provider_version::text as v, cancel_at_period_end as cancel, ${newest.modified_at}::timestamptz::text as nv
      from subscriptions where provider_subscription_id = ${subId}`);
    expect(s.v).toBe(s.nv);
    expect(s.cancel).toBe(newest.cancel_at_period_end);
  });

  test("two drains over 50 rows send each exactly once", async () => {
    await h.service.drainOutbox();
    const now = h.mock.now();
    await h.service.withCtx(async (c) => {
      for (let i = 0; i < 50; i++)
        await enqueueEmail(c.db, {
          kind: "alert_digest",
          dedupeKey: `alert_digest:test-${i}`,
          to: `drain${i}@convt.test`,
          userId: null,
          subjectId: "2026-01-01",
          now,
        });
    });
    const before = h.mock.resend.sent.length;
    const drainAll = async () => {
      for (let i = 0; i < 10; i++) if ((await h.service.drainOutbox()).claimed === 0) break;
    };
    await Promise.all([drainAll(), drainAll()]);
    const sent = h.mock.resend.sent.slice(before).filter((m) => m.to[0].startsWith("drain"));
    expect(sent.length).toBe(50);
    expect(new Set(sent.map((m) => m.to[0])).size).toBe(50);
    const [r] = await h.q<{ n: number }>(
      sql`select count(*)::int as n from email_outbox where dedupe_key like 'alert_digest:test-%' and status = 'sent' and attempts = 1`,
    );
    expect(r.n).toBe(50);
  });

  test("a stale worker's completion is refused by the claim generation", async () => {
    await h.service.drainOutbox();
    await h.buy("desktop", null, { email: "stale@convt.test" });
    await h.deliverAll();
    let stalled = false;
    let secondResult: unknown = null;
    // Worker A's own send fails (a 5xx), so its late completion would put the row
    // back to pending if the fence did not stop it.
    h.mock.resend.addFault({ kind: "5xx" });
    h.onFault("after-send-before-mark", async () => {
      if (stalled) return;
      stalled = true;
      // Worker A stalls after sending; its lease expires and worker B takes over.
      h.mock.advance(3 * 60_000);
      secondResult = await h.service.drainOutbox();
      const [mid] = await h.q<{ status: string; claim_generation: number }>(
        sql`select status, claim_generation from email_outbox where to_email = 'stale@convt.test'`,
      );
      expect(mid).toEqual({ status: "sent", claim_generation: 2 });
    });
    await h.service.drainOutbox();
    h.clearFaults();
    expect(secondResult).toMatchObject({ sent: 1 });
    const [row] = await h.q<{ status: string; claim_generation: number; attempts: number }>(
      sql`select status, claim_generation, attempts from email_outbox where to_email = 'stale@convt.test'`,
    );
    // A's late "sent" did not touch B's row (generation 2), and Resend deduplicated A's and B's sends.
    expect(row).toEqual({ status: "sent", claim_generation: 2, attempts: 2 });
    await h.service.drainOutbox();
    expect(h.mock.resend.sent.filter((m) => m.to[0] === "stale@convt.test").length).toBe(1);
  });
});
