// Desktop + Cloud and the $39 Desktop price end to end through the mock: one
// Desktop key and a prepaid cloud year per order, stacking, refunds and lost
// chargebacks, complimentary orders, a Pro subscriber who also buys the bundle,
// account deletion, and $29 orders sold before the price change.

import { afterAll, beforeAll, describe, expect, test } from "bun:test";
import { claimPurchases, cloudAllowance } from "@convt/db/queries";
import { sql } from "drizzle-orm";

import { BUNDLE_CLOUD_DAYS } from "../../src/catalog";
import { ingestFacts } from "../../src/ingest";
import { emptyFacts } from "../../src/provider";
import { createHarness, type Harness } from "../../src/testing";
import { testMailbox } from "../mailbox";

let h: Harness;
beforeAll(async () => {
  h = await createHarness();
});
afterAll(async () => h?.close());

const yearMs = BUNDLE_CLOUD_DAYS * 86_400_000;

type Year = {
  provider_order_id: string;
  status: string;
  current_period_start: Date;
  current_period_end: Date;
  ended_at: Date;
};

/** Timestamps arrive as text on the owner connection. */
const at = (v: Date | string) => new Date(v);

async function cloudYearsOf(userId: string): Promise<Year[]> {
  const rows = await h.q<Year>(sql`
    select o.provider_order_id, s.status, s.current_period_start, s.current_period_end, s.ended_at
    from subscriptions s join orders o on o.id = s.order_id
    where s.user_id = ${userId} order by o.billed_at, o.id`);
  return rows.map((r) => ({
    ...r,
    current_period_start: at(r.current_period_start),
    current_period_end: at(r.current_period_end),
    ended_at: at(r.ended_at),
  }));
}

async function keysOf(userId: string) {
  return h.q<{ plan: string; product: string; revoke_reason: string | null }>(sql`
    select l.plan, o.product, l.revoke_reason from licenses l left join orders o on o.id = l.order_id
    where l.user_id = ${userId} order by l.created_at`);
}

async function buyBundle(u: { id: string; email: string }) {
  const b = await h.buy("bundle", u);
  await h.deliverAll();
  const order = h.mock.state().orders.find((o) => o.checkout_id === b.providerCheckoutId)!;
  return { ...b, orderId: order.id };
}

async function web() {
  return (await h.tdb.open("web")).db;
}

describe("Desktop + Cloud", () => {
  test("the app's trial-only Pro checkout is refused, not sold paid, once the trial is gone", async () => {
    // The race the site's eligibility check cannot close: a bundle lands between that
    // check and checkout creation. Billing decides at creation time.
    const u = await h.user(testMailbox("bundle-trial-race"));
    await buyBundle(u);
    const before = await h.q<{ n: number }>(
      sql`select count(*)::int as n from checkouts where user_id = ${u.id} and product like 'pro_%'`,
    );
    const r = await h.service.createCheckout({
      product: "pro_month",
      user: u,
      fromApp: true,
      trialOnly: true,
    });
    expect(r).toEqual({ ok: false, refusal: "trial_unavailable" });
    const after = await h.q<{ n: number }>(
      sql`select count(*)::int as n from checkouts where user_id = ${u.id} and product like 'pro_%'`,
    );
    expect(after[0].n).toBe(before[0].n);
  });

  test("needs an account: a signed-out checkout is refused", async () => {
    expect(await h.service.createCheckout({ product: "bundle", user: null })).toEqual({
      ok: false,
      refusal: "sign_in_required",
    });
  });

  test("one order: a lifetime Desktop key, a year of cloud, one email, one event, no Pro key", async () => {
    const u = await h.user(testMailbox("bundle1"));
    const before = h.analytics.length;
    const b = await buyBundle(u);
    const [order] = await h.q<{ product: string; amount_cents: number }>(
      sql`select product, amount_cents from orders where provider_order_id = ${b.orderId}`,
    );
    expect(order).toEqual({ product: "bundle", amount_cents: 6900 });
    expect(await keysOf(u.id)).toEqual([
      { plan: "desktop", product: "bundle", revoke_reason: null },
    ]);
    const [year] = await cloudYearsOf(u.id);
    expect(year.status).toBe("active");
    expect(year.current_period_end.getTime() - year.current_period_start.getTime()).toBe(yearMs);
    expect(year.ended_at).toEqual(year.current_period_end);
    const purchases = h.analytics.slice(before).filter((e) => e.event === "license_purchased");
    expect(purchases.map((e) => e.properties)).toEqual([{ plan: "desktop", product: "bundle" }]);
    expect(await h.service.currentProAccess(u.id)).toEqual({ kind: "pro" });
    expect((await cloudAllowance(await web(), u.id, "pro")).allowed).toBe(true);
    // The success page shows this order's Desktop key.
    const result = await h.service.checkoutResult({
      providerCheckoutId: b.providerCheckoutId,
      cookie: b.cookieValue,
      sessionUserId: u.id,
      sync: false,
    });
    expect(result.result.state).toBe("ready");
    expect(result.result.state === "ready" && result.result.product).toBe("bundle");
    await h.service.drainOutbox();
    const mails = h.mock.resend.sent.filter((m) => m.to[0] === u.email);
    expect(mails.length).toBe(1);
  });

  test("a second order stacks; refunding the first moves the second to the refund", async () => {
    const u = await h.user(testMailbox("bundle-stack"));
    const first = await buyBundle(u);
    h.mock.advance(30 * 86_400_000);
    const second = await buyBundle(u);
    let years = await cloudYearsOf(u.id);
    expect(years.map((y) => y.status)).toEqual(["active", "active"]);
    expect(years[1].current_period_start).toEqual(years[0].current_period_end);
    expect(years[1].current_period_end.getTime() - years[0].current_period_end.getTime()).toBe(
      yearMs,
    );
    // The year that hasn't started gives no access of its own, and adds no allowance.
    const allowance = await cloudAllowance(await web(), u.id, "pro");
    expect(allowance.allowed).toBe(true);
    expect(allowance.limit).toBe(50_000_000_000);

    h.mock.advance(10 * 86_400_000);
    const refundedAt = h.mock.now();
    h.mock.refund(first.orderId);
    await h.deliverAll();
    years = await cloudYearsOf(u.id);
    expect(years[0].status).toBe("canceled");
    // The mock clock runs in real time, so "at the refund" is within a few seconds.
    const near = (d: Date, want: number) => expect(Math.abs(d.getTime() - want)).toBeLessThan(5000);
    near(years[0].ended_at, refundedAt.getTime());
    expect(years[1].status).toBe("active");
    expect(years[1].current_period_start).toEqual(years[0].ended_at);
    near(years[1].current_period_end, refundedAt.getTime() + yearMs);
    expect((await keysOf(u.id)).map((k) => k.revoke_reason)).toEqual(["refunded", null]);
    expect(await h.service.currentProAccess(u.id)).toEqual({ kind: "pro" });
    void second;
  });

  test("a lost chargeback revokes the Desktop key and ends the cloud year", async () => {
    const u = await h.user(testMailbox("bundle-dispute"));
    const b = await buyBundle(u);
    const d = h.mock.openDispute(b.orderId);
    h.mock.closeDispute(d.id, "lost");
    await h.deliverAll();
    await h.service.reconcileFrequent();
    expect((await keysOf(u.id)).map((k) => k.revoke_reason)).toEqual(["dispute_lost"]);
    const [year] = await cloudYearsOf(u.id);
    expect(year.status).toBe("canceled");
    expect(year.ended_at.getTime()).toBeLessThanOrEqual(h.mock.now().getTime());
    expect((await cloudAllowance(await web(), u.id, "pro")).allowed).toBe(false);
    expect(await h.service.currentProAccess(u.id)).toEqual({ kind: "lapsed" });
  });

  test("after the year: lapsed, and buying again starts a new year now", async () => {
    const u = await h.user(testMailbox("bundle-after"));
    await buyBundle(u);
    h.mock.advance(yearMs + 86_400_000);
    expect(await h.service.currentProAccess(u.id)).toEqual({ kind: "lapsed" });
    expect((await cloudAllowance(await web(), u.id, "pro")).allowed).toBe(false);
    await buyBundle(u);
    const years = await cloudYearsOf(u.id);
    expect(years[1].current_period_start.getTime()).toBeGreaterThan(
      years[0].current_period_end.getTime(),
    );
    expect(await h.service.currentProAccess(u.id)).toEqual({ kind: "pro" });
  });

  test("complimentary bundles grant the key and the year: a 100% code and a zeroed item", async () => {
    for (const shape of ["full", "zeroed"] as const) {
      const u = await h.user(testMailbox(`bundle-free-${shape}`));
      const created = await h.service.createCheckout({ product: "bundle", user: u });
      if (!created.ok) throw new Error(created.refusal);
      const co = h.mock.checkoutBySecret(created.url.split("/checkout/")[1])!;
      h.mock.completeCheckout(co.id, "4242", u.email);
      const held = h.mock.takeHeld();
      for (const d of held) {
        if (!d.type.startsWith("order.")) {
          await h.deliver(d);
          continue;
        }
        const env = JSON.parse(d.body);
        if (shape === "full") {
          env.data.discount_id = "disc_giveaway_100";
          env.data.discount_amount = 6900;
        } else {
          env.data.items[0].amount = 0;
          env.data.subtotal_amount = 0;
          env.data.discount_amount = 0;
        }
        env.data.net_amount = 0;
        env.data.total_amount = 0;
        expect((await h.deliver(d, { body: JSON.stringify(env) })).status).toBe(200);
      }
      expect((await keysOf(u.id)).map((k) => k.plan)).toEqual(["desktop"]);
      expect((await cloudYearsOf(u.id)).map((y) => y.status)).toEqual(["active"]);
    }
  });

  test("a Pro subscriber who buys the bundle keeps the subscription and its controls", async () => {
    const email = testMailbox("bundle-pro");
    const u = await h.user(email);
    // A returning customer: no trial, the first period is paid at checkout.
    await h.owner
      .execute(sql`insert into subscriptions (id, provider, provider_subscription_id, user_id, email, kind, interval, status, ended_at)
      values (${`sub_old_${u.id.slice(4, 12)}`}, 'polar', ${`old_${u.id}`}, ${u.id}, ${email}, 'pro', 'month', 'canceled', now())`);
    const pro = await h.buy("pro_month", u);
    await h.deliverAll();
    const proSubId = h.mock
      .state()
      .checkouts.find((c) => c.id === pro.providerCheckoutId)!.subscription_id!;
    const b = await buyBundle(u);
    // The bundle's success page shows the bundle's Desktop key, not the Pro key.
    const result = await h.service.checkoutResult({
      providerCheckoutId: b.providerCheckoutId,
      cookie: b.cookieValue,
      sessionUserId: u.id,
      sync: false,
    });
    expect(result.result.state === "ready" && result.result.product).toBe("bundle");
    const [desktopKey] = await h.q<{ id: string }>(
      sql`select l.id from licenses l join orders o on o.id = l.order_id where o.provider_order_id = ${b.orderId}`,
    );
    expect(result.result.state === "ready" && result.result.licenseId).toBe(desktopKey.id);
    // Cancel and resume act on the Polar subscription, never on the prepaid year.
    expect(await h.service.setCancel(u.id, "pro", true)).toEqual({ ok: true });
    await h.deliverAll();
    const [s] = await h.q<{ cancel_at_period_end: boolean }>(
      sql`select cancel_at_period_end from subscriptions where provider_subscription_id = ${proSubId}`,
    );
    expect(s.cancel_at_period_end).toBe(true);
    expect(await h.service.setCancel(u.id, "pro", false)).toEqual({ ok: true });
    // One allowance for the account: usage on the subscription counts while the job
    // goes on whichever row is newest and usable.
    const [proRow] = await h.q<{ id: string }>(
      sql`select id from subscriptions where provider_subscription_id = ${proSubId}`,
    );
    await h.owner.execute(sql`
      insert into usage_events (id, user_id, subscription_id, job_id, kind, quantity, amount_cents, occurred_at)
      values (${`use_${u.id}`}, ${u.id}, ${proRow.id}, ${`job_${u.id}`}, 'pro_bytes', 1000, 0, ${h.mock.now()})`);
    const allowance = await cloudAllowance(await web(), u.id, "pro");
    expect(allowance.used).toBe(1000);
    // No duplicate-subscription alert for a subscription plus a cloud year.
    const alerts = await h.q(
      sql`select 1 from billing_alerts where kind = 'duplicate_subscription' and subject = ${`pro:${u.id}`}`,
    );
    expect(alerts.length).toBe(0);
  });

  test("deleting an account that holds a bundle ends the year and keeps the rows", async () => {
    const u = await h.user(testMailbox("bundle-delete"));
    const b = await buyBundle(u);
    const d = await h.service.requestDeletion(u.id);
    expect(await h.service.advanceDeletion(d.id)).toBe("done");
    const [user] = await h.q(sql`select 1 from users where id = ${u.id}`);
    expect(user).toBeUndefined();
    const [row] = await h.q<{ status: string; user_id: string | null; ended_at: Date }>(sql`
      select s.status, s.user_id, s.ended_at from subscriptions s join orders o on o.id = s.order_id
      where o.provider_order_id = ${b.orderId}`);
    expect(row.status).toBe("canceled");
    expect(row.user_id).toBeNull();
    expect(at(row.ended_at).getTime()).toBeLessThanOrEqual(h.mock.now().getTime());
    const [order] = await h.q<{ status: string }>(
      sql`select status from orders where provider_order_id = ${b.orderId}`,
    );
    expect(order.status).toBe("paid");
  });
});

describe("hosted Desktop + Cloud purchases", () => {
  const dayMs = 86_400_000;
  /** A bundle bought on Polar's own page: no checkout of ours, no account id. */
  async function hostedBundle(email: string) {
    const raw = h.mock.craftOrder({
      externalCustomerId: null,
      email,
      product: "bundle",
      reason: "purchase",
      checkoutId: null,
      items: [{ amount: 6900, priceId: "price_local_bundle" }],
    });
    const paid = h.mock.takeHeld().find((d) => d.type === "order.paid")!;
    expect((await h.deliver(paid)).status).toBe(200);
    return raw.id;
  }
  const stacked = (years: Year[]) =>
    years.length === 2 &&
    years[1].current_period_start.getTime() === years[0].current_period_end.getTime() &&
    years[1].current_period_end.getTime() - years[0].current_period_start.getTime() === 2 * yearMs;

  test("two bought a day apart by an existing account stack once claimed", async () => {
    const u = await h.user(testMailbox("hosted-existing"));
    await hostedBundle(u.email);
    h.mock.advance(dayMs);
    await hostedBundle(u.email);
    const years = await cloudYearsOf(u.id);
    expect(years.map((y) => y.status)).toEqual(["active", "active"]);
    expect(stacked(years)).toBe(true);
  });

  test("two bought before sign-up stack after the site's claim, through billing", async () => {
    const email = testMailbox("hosted-signup");
    await hostedBundle(email);
    h.mock.advance(dayMs);
    await hostedBundle(email);
    // The site claims at sign-up as convt_web, which cannot write coverage; it
    // then asks billing to lay the years out again.
    const u = await h.user(email);
    expect((await claimPurchases(h.owner, u.id)).orders).toBe(2);
    expect(stacked(await cloudYearsOf(u.id))).toBe(false);
    await h.service.scheduleCloud(u.id);
    expect(stacked(await cloudYearsOf(u.id))).toBe(true);
  });

  test("a lost chargeback during the repair is never undone by it", async () => {
    // Three guest bundles claimed together, so the repair re-lays all three. While
    // it is paused between its read and its writes, a lost chargeback on the middle
    // order arrives. The repair must not write from what it read before.
    const email = testMailbox("hosted-race");
    const ids: string[] = [];
    for (let i = 0; i < 3; i++) {
      ids.push(await hostedBundle(email));
      h.mock.advance(dayMs);
    }
    const u = await h.user(email);
    await claimPurchases(h.owner, u.id);
    const d = h.mock.openDispute(ids[1]);
    h.mock.closeDispute(d.id, "lost");
    h.mock.takeHeld();
    let dispute: Promise<unknown> | null = null;
    h.onFault("after-cloud-read", async () => {
      if (dispute) return;
      dispute = h.service.withCtx(async (c) =>
        ingestFacts(c, { ...emptyFacts(), disputes: [await c.provider.getDispute(d.id)] }, "test"),
      );
      // Give the chargeback every chance to land before the repair writes. With the
      // shared account lock it waits for the repair instead.
      await Promise.race([dispute, new Promise((r) => setTimeout(r, 1500))]);
    });
    try {
      await h.service.scheduleCloud(u.id);
      await dispute;
    } finally {
      h.clearFaults();
    }
    const years = await cloudYearsOf(u.id);
    expect(years.map((y) => y.status)).toEqual(["active", "canceled", "active"]);
    // The charged-back year ended at the chargeback and covered nothing...
    expect(years[1].ended_at.getTime()).toBeLessThanOrEqual(h.mock.now().getTime());
    // ...so the third starts where the first ends: no gap, no overlap.
    expect(years[2].current_period_start).toEqual(years[0].current_period_end);
  });

  test("the reconciler stacks overlapping years the site's request missed", async () => {
    const email = testMailbox("hosted-sweep");
    await hostedBundle(email);
    h.mock.advance(dayMs);
    await hostedBundle(email);
    const u = await h.user(email);
    await claimPurchases(h.owner, u.id);
    await h.service.reconcileFrequent();
    expect(stacked(await cloudYearsOf(u.id))).toBe(true);
  });
});

describe("Desktop at $39", () => {
  test("a $39 order issues a key; a $29 order at the new price is rejected", async () => {
    const u = await h.user(testMailbox("desktop39"));
    const b = await h.buy("desktop", u);
    await h.deliverAll();
    const [o] = await h.q<{ amount_cents: number }>(
      sql`select amount_cents from orders where checkout_id = ${b.checkoutId}`,
    );
    expect(o.amount_cents).toBe(3900);
    expect((await keysOf(u.id)).map((k) => k.product)).toEqual(["desktop"]);
    expect(await cloudYearsOf(u.id)).toEqual([]);

    const cheap = await h.buy("desktop", null, { email: testMailbox("desktop39-cheap") });
    const held = h.mock.takeHeld();
    const paid = held.find((d) => d.type === "order.paid")!;
    const env = JSON.parse(paid.body);
    env.data.items[0].amount = 2900;
    env.data.subtotal_amount = 2900;
    env.data.net_amount = 2900;
    env.data.total_amount = 2900;
    const r = await h.deliver(paid, { body: JSON.stringify(env) });
    expect(r.status).toBe(200);
    const rows = await h.q(sql`select 1 from orders where checkout_id = ${cheap.checkoutId}`);
    expect(rows.length).toBe(0);
  });

  test("a $29 order sold before the change still grants, refetches, and revokes on refund", async () => {
    const legacy = { key: "desktop", price_id: "price_local_desktop", amount: 2900 };
    const current = { key: "desktop", price_id: "price_local_desktop_39", amount: 3900 };
    const setPrice = (p: typeof legacy) =>
      h.mock.fetch(
        new Request(`${h.base}/admin/product`, { method: "POST", body: JSON.stringify(p) }),
      );
    await setPrice(legacy);
    const u = await h.user(testMailbox("desktop29"));
    const b = await h.buy("desktop", u);
    await setPrice(current);
    await h.deliverAll();
    const orderId = h.mock.state().orders.find((o) => o.checkout_id === b.providerCheckoutId)!.id;
    expect((await keysOf(u.id)).map((k) => k.revoke_reason)).toEqual([null]);
    // The reconciler refetches it at the current catalog without an alert.
    await h.service.reconcileFrequent();
    h.mock.refund(orderId);
    await h.deliverAll();
    expect((await keysOf(u.id)).map((k) => k.revoke_reason)).toEqual(["refunded"]);
    const alerts = await h.q(
      sql`select detail from billing_alerts where kind = 'rejected_fact' and detail like '%Desktop is%'`,
    );
    expect(alerts).toEqual([]);
  });
});
