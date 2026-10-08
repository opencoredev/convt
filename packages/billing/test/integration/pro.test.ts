// Pro: keys only from paid coverage. Trials, renewals, interval switches, refunds,
// ordering and version rules.

import { afterAll, beforeAll, describe, expect, test } from "bun:test";
import { sql } from "drizzle-orm";

import { localProducts } from "@convt/billing-mock";
import { createHarness, testMailbox, type Harness } from "../../src/testing";

let h: Harness;
beforeAll(async () => {
  h = await createHarness();
});
afterAll(async () => h?.close());

const permutations = <T>(xs: T[]): T[][] =>
  xs.length <= 1
    ? [xs]
    : xs.flatMap((x, i) =>
        permutations([...xs.slice(0, i), ...xs.slice(i + 1)]).map((p) => [x, ...p]),
      );

async function keys(providerSubId: string) {
  return h.q<{
    id: string;
    period_start: string;
    updates_until: string;
    revoked_at: string | null;
    revoke_reason: string | null;
    invoice_id: string;
  }>(sql`
    select l.id, l.period_start::text, l.updates_until::text, l.revoked_at, l.revoke_reason, l.invoice_id
    from licenses l join subscriptions s on s.id = l.subscription_id
    where s.provider_subscription_id = ${providerSubId} order by l.updates_until`);
}

async function sub(providerSubId: string) {
  const [s] = await h.q<{ status: string; interval: string; cancel_at_period_end: boolean }>(sql`
    select status, interval, cancel_at_period_end from subscriptions where provider_subscription_id = ${providerSubId}`);
  return s;
}

async function startPro(email: string, interval: "month" | "year" = "month") {
  const u = await h.user(email);
  const b = await h.buy(interval === "month" ? "pro_month" : "pro_year", u);
  const subId = h.mock
    .state()
    .checkouts.find((c) => c.id === b.providerCheckoutId)!.subscription_id!;
  return { u, b, subId };
}

describe("trials", () => {
  test("device access is read-only and points to the session checkout route", async () => {
    const u = await h.user(testMailbox("device-trial"));
    const before = await h.q<{ n: number }>(sql`select count(*)::int as n from checkouts`);
    expect(await h.service.currentProAccess(u.id)).toEqual({
      kind: "can_start_trial",
      checkoutUrl: "http://localhost:3000/checkout/pro",
    });
    const after = await h.q<{ n: number }>(sql`select count(*)::int as n from checkouts`);
    expect(after[0].n).toBe(before[0].n);
  });

  test("a trial that ended before payment is lapsed", async () => {
    const { u } = await startPro(testMailbox("ended-trial"));
    await h.deliverAll();
    await h.q(
      sql`update subscriptions set trial_ends_at = now() - interval '1 day', status = 'trialing' where user_id = ${u.id}`,
    );
    expect(await h.service.currentProAccess(u.id)).toEqual({ kind: "lapsed" });
  });

  test("an incomplete subscription with a future trial end is still a trial", async () => {
    const { u } = await startPro(testMailbox("incomplete-trial"));
    await h.deliverAll();
    await h.q(sql`update subscriptions set status = 'incomplete' where user_id = ${u.id}`);
    const access = await h.service.currentProAccess(u.id);
    expect(access).toMatchObject({ kind: "trial" });
    if (access.kind === "trial") {
      expect(access.endsAt).toMatch(/Z$/);
      expect(access.endsOn).toBe(access.endsAt.slice(0, 10));
    }
  });

  test("an unfinished account deletion lapses access before subscription state", async () => {
    const { u } = await startPro(testMailbox("deleting-pro"));
    await h.deliverAll();
    await h.q(
      sql`insert into account_deletions (id, user_id, status) values (${`del_${u.id}`}, ${u.id}, 'pending')`,
    );
    expect(await h.service.currentProAccess(u.id)).toEqual({ kind: "lapsed" });
  });

  test("a trialing subscription and its $0 paid order issue nothing; conversion issues one key and one email", async () => {
    const { subId } = await startPro("trial1@convt.test");
    await h.deliverAll();
    expect((await sub(subId)).status).toBe("trialing");
    expect(await keys(subId)).toEqual([]);
    const [inv] = await h.q<{ status: string; net_cents: number }>(
      sql`select i.status, i.net_cents from invoices i join subscriptions s on s.id = i.subscription_id where s.provider_subscription_id = ${subId}`,
    );
    expect(inv).toEqual({ status: "paid", net_cents: 0 });
    h.mock.cycle(subId); // the trial ends and the first payment goes through
    await h.deliverAll();
    const k = await keys(subId);
    expect(k.length).toBe(1);
    const s = h.mock.subscription(subId)!;
    expect(k[0].updates_until).toBe(s.current_period_end.slice(0, 10));
    const [mail] = await h.q<{ n: number }>(
      sql`select count(*)::int as n from email_outbox where kind = 'license_issued' and to_email = 'trial1@convt.test'`,
    );
    expect(mail.n).toBe(1);
    // The next period: one more key, no email.
    h.mock.cycle(subId);
    await h.deliverAll();
    expect((await keys(subId)).length).toBe(2);
    const [mail2] = await h.q<{ n: number }>(
      sql`select count(*)::int as n from email_outbox where kind = 'license_issued' and to_email = 'trial1@convt.test'`,
    );
    expect(mail2.n).toBe(1);
  });

  test("a trial cancelled before its end issues nothing", async () => {
    const { u, subId } = await startPro("trial2@convt.test");
    await h.deliverAll();
    expect(await h.service.setCancel(u.id, "pro", true)).toEqual({ ok: true });
    h.mock.cycle(subId);
    await h.deliverAll();
    expect((await sub(subId)).status).toBe("canceled");
    expect(await keys(subId)).toEqual([]);
  });

  test("a returning customer's checkout has allow_trial false", async () => {
    const { u } = await startPro("trial3@convt.test");
    await h.deliverAll();
    const subId = (
      await h.q<{ provider_subscription_id: string }>(
        sql`select provider_subscription_id from subscriptions where user_id = ${u.id}`,
      )
    )[0].provider_subscription_id;
    await h.provider.revokeSubscription(subId);
    await h.deliverAll();
    const again = await h.service.createCheckout({ product: "pro_month", user: u });
    expect(again.ok).toBe(true);
    const [co] = await h.q<{ allow_trial: boolean }>(
      sql`select allow_trial from checkouts where user_id = ${u.id} order by created_at desc limit 1`,
    );
    expect(co.allow_trial).toBe(false);
    const polarCo = h.mock.lastCheckout()!;
    expect(polarCo.allowTrial).toBe(false);
  });

  test("a live Pro subscription refuses another Pro checkout", async () => {
    const { u } = await startPro("trial4@convt.test");
    await h.deliverAll();
    expect(await h.service.createCheckout({ product: "pro_year", user: u })).toEqual({
      ok: false,
      refusal: "already_pro",
    });
  });
});

describe("paid coverage", () => {
  async function paidMonthly(email: string) {
    const u = await h.user(email);
    // A returning customer: no trial, the first period is paid at checkout.
    await h.owner
      .execute(sql`insert into subscriptions (id, provider, provider_subscription_id, user_id, email, kind, interval, status, ended_at)
      values (${`sub_old_${u.id.slice(4, 12)}`}, 'polar', ${`old_${u.id}`}, ${u.id}, ${email}, 'pro', 'month', 'canceled', now())`);
    const b = await h.buy("pro_month", u);
    const subId = h.mock
      .state()
      .checkouts.find((c) => c.id === b.providerCheckoutId)!.subscription_id!;
    await h.deliverAll();
    return { u, subId };
  }

  test("monthly to yearly with payment success issues a key with the later end", async () => {
    const { u, subId } = await paidMonthly("switch1@convt.test");
    const before = await keys(subId);
    expect(before.length).toBe(1);
    expect(await h.service.switchInterval(u.id, "year")).toEqual({ ok: true });
    await h.deliverAll();
    const after = await keys(subId);
    expect(after.length).toBe(2);
    expect(after[1].updates_until > before[0].updates_until).toBe(true);
    expect((await sub(subId)).interval).toBe("year");
  });

  test("a declined switch changes nothing and issues nothing", async () => {
    const { u, subId } = await paidMonthly("switch2@convt.test");
    h.mock.setDecline(u.id, true);
    expect(await h.service.switchInterval(u.id, "year")).toEqual({ ok: false, reason: "declined" });
    await h.deliverAll();
    expect((await keys(subId)).length).toBe(1);
    expect((await sub(subId)).interval).toBe("month");
    expect(h.mock.subscription(subId)!.product_id).toBe(localProducts.pro_month.productId);
  });

  test("a switch to yearly is refused while the monthly-only launch code applies", async () => {
    const { u, subId } = await paidMonthly("switch-ph@convt.test");
    h.mock.mutateQuietly("subscription", subId, (x) => {
      x.discountId = "disc_local_producthunt";
    });
    expect(await h.service.switchInterval(u.id, "year")).toEqual({
      ok: false,
      reason: "discount",
    });
    await h.deliverAll();
    expect((await keys(subId)).length).toBe(1);
    expect(h.mock.subscription(subId)!.product_id).toBe(localProducts.pro_month.productId);
  });

  test("a zero-charge switch with no balance applied funds nothing", async () => {
    const { u, subId } = await paidMonthly("switch3@convt.test");
    const s = h.mock.subscription(subId)!;
    const end = new Date(Date.parse(s.current_period_end) + 365 * 86_400_000).toISOString();
    h.mock.craftOrder({
      externalCustomerId: u.id,
      email: u.email,
      product: "pro_year",
      reason: "subscription_update",
      subscriptionId: subId,
      items: [
        {
          amount: -9600,
          priceId: localProducts.pro_month.priceId,
          start: s.current_period_start,
          end: s.current_period_end,
          proration: true,
        },
        {
          amount: 9600,
          priceId: localProducts.pro_year.priceId,
          start: s.current_period_end,
          end,
          proration: true,
        },
      ],
    });
    await h.deliverAll();
    expect((await keys(subId)).length).toBe(1);
  });

  test("a yearly to monthly downgrade is credited: nothing until monthly coverage passes the yearly date, then one key", async () => {
    const u = await h.user("down@convt.test");
    await h.owner
      .execute(sql`insert into subscriptions (id, provider, provider_subscription_id, user_id, email, kind, interval, status, ended_at)
      values ('sub_old_down', 'polar', 'old_down', ${u.id}, 'down@convt.test', 'pro', 'year', 'canceled', now())`);
    const b = await h.buy("pro_year", u);
    const subId = h.mock
      .state()
      .checkouts.find((c) => c.id === b.providerCheckoutId)!.subscription_id!;
    await h.deliverAll();
    const yearly = await keys(subId);
    expect(yearly.length).toBe(1);
    h.mock.advance(3 * 86_400_000);
    expect(await h.service.switchInterval(u.id, "month")).toEqual({ ok: true });
    await h.deliverAll();
    expect((await keys(subId)).length).toBe(1);
    // Monthly periods paid from the credit balance; none passes the yearly date until the last.
    let paidFromCredit = 0;
    for (let i = 0; i < 13 && (await keys(subId)).length === 1; i++) {
      h.mock.cycle(subId);
      await h.deliverAll();
      const last = h.mock.ordersFor(subId).at(-1)!;
      if (last.applied_balance_amount > 0) paidFromCredit++;
    }
    const k = await keys(subId);
    expect(k.length).toBe(2);
    expect(k[1].updates_until > yearly[0].updates_until).toBe(true);
    expect(paidFromCredit).toBeGreaterThan(0);
  });

  test("next_period through the interface: the product changes at the cycle, then the key follows", async () => {
    const { subId } = await paidMonthly("nextperiod@convt.test");
    const r = await h.provider.changeProduct(subId, "pro_year", "next_period");
    expect("paymentFailed" in r).toBe(false);
    if (!("paymentFailed" in r)) expect(r.pendingUpdate).not.toBeNull();
    await h.deliverAll();
    expect((await keys(subId)).length).toBe(1);
    h.mock.cycle(subId);
    await h.deliverAll();
    expect((await sub(subId)).interval).toBe("year");
    const k = await keys(subId);
    expect(k.length).toBe(2);
    expect(Date.parse(k[1].updates_until) - Date.parse(k[0].updates_until)).toBeGreaterThan(
      300 * 86_400_000,
    );
  });

  test("a refunded Pro invoice revokes only its key; a partial refund revokes nothing", async () => {
    const { subId } = await paidMonthly("refundpro@convt.test");
    h.mock.cycle(subId);
    await h.deliverAll();
    const k = await keys(subId);
    expect(k.length).toBe(2);
    const orders = h.mock.ordersFor(subId);
    h.mock.refund(orders.at(-1)!.id, 500);
    await h.deliverAll();
    expect((await keys(subId)).every((x) => x.revoked_at === null)).toBe(true);
    h.mock.refund(orders.at(-1)!.id);
    await h.deliverAll();
    const after = await keys(subId);
    expect(after.map((x) => x.revoke_reason)).toEqual([null, "refunded"]);
  });
});

describe("ordering and versions", () => {
  test("every permutation of a renewal's events ends in the same state", async () => {
    const u = await h.user("renew@convt.test");
    await h.owner
      .execute(sql`insert into subscriptions (id, provider, provider_subscription_id, user_id, email, kind, interval, status, ended_at)
      values ('sub_old_renew', 'polar', 'old_renew', ${u.id}, 'renew@convt.test', 'pro', 'month', 'canceled', now())`);
    const b = await h.buy("pro_month", u);
    const subId = h.mock
      .state()
      .checkouts.find((c) => c.id === b.providerCheckoutId)!.subscription_id!;
    await h.deliverAll();
    h.mock.cycle(subId);
    const renewal = h.mock
      .takeHeld()
      .filter((d) => d.type.startsWith("order.") || d.type.startsWith("subscription."));
    expect(renewal.length).toBe(4);
    for (const p of permutations(renewal))
      for (const d of p) expect((await h.deliver(d)).status).toBe(200);
    expect((await keys(subId)).length).toBe(2);
    expect((await sub(subId)).status).toBe("active");

    // A failed renewal, in every order: past due, no new key, one renewal_failed email.
    h.mock.setDecline(u.id, true);
    h.mock.cycle(subId);
    const failure = h.mock.takeHeld();
    for (const p of permutations(failure.slice(0, 4)))
      for (const d of p) expect((await h.deliver(d)).status).toBe(200);
    for (const d of failure) await h.deliver(d);
    expect((await sub(subId)).status).toBe("past_due");
    expect((await keys(subId)).length).toBe(2);
    const [mail] = await h.q<{ n: number }>(
      sql`select count(*)::int as n from email_outbox where kind = 'renewal_failed' and to_email = 'renew@convt.test'`,
    );
    expect(mail.n).toBe(1);
  });

  test("a null modified_at falls back to created_at", async () => {
    const b = await h.buy("desktop", null, { email: "nullmod@convt.test" });
    const held = h.mock.takeHeld();
    const created = held.find((d) => d.type === "order.created")!;
    const data = JSON.parse(created.body).data;
    expect(data.modified_at).toBeNull();
    await h.deliver(created);
    const [o] = await h.q<{ v: string; created: string }>(sql`
      select provider_version::text as v, ${data.created_at}::timestamptz::text as created from orders where provider_order_id = ${data.id}`);
    expect(o.v).toBe(o.created);
    for (const d of held) await h.deliver(d);
    void b;
  });

  test("equal versions with different content fetch and apply the provider's state", async () => {
    const b = await h.buy("desktop", null, { email: "conflict@convt.test" });
    await h.deliverAll();
    const order = h.mock.state().orders.find((o) => o.checkout_id === b.providerCheckoutId)!;
    // Someone changes the order at the provider without moving modified_at.
    h.mock.mutateQuietly(
      "order",
      order.id,
      (o) => {
        o.status = "partially_refunded";
        o.refunded = 700;
      },
      true,
    );
    const body = JSON.stringify({
      type: "order.updated",
      timestamp: new Date().toISOString(),
      api_version: "2026-10",
      data: h.mock.renderOrderRaw(order.id),
    });
    const r = await h.deliver({ id: "msg_conflict_1", type: "order.updated", body });
    expect(r.status).toBe(200);
    const [row] = await h.q<{ status: string; refunded_cents: number }>(
      sql`select status, refunded_cents from orders where provider_order_id = ${order.id}`,
    );
    expect(row).toEqual({ status: "partially_refunded", refunded_cents: 700 });
    const [ev] = await h.q<{ reason: string }>(
      sql`select reason from webhook_events where provider_event_id = 'msg_conflict_1'`,
    );
    expect(ev.reason).toContain("conflict");
  });

  test("a newer fact contradicting a terminal fact is not applied and is alerted", async () => {
    const b = await h.buy("desktop", null, { email: "contra@convt.test" });
    await h.deliverAll();
    const order = h.mock.state().orders.find((o) => o.checkout_id === b.providerCheckoutId)!;
    h.mock.refund(order.id);
    await h.deliverAll();
    h.mock.mutateQuietly("order", order.id, (o) => {
      o.status = "paid";
      o.refunded = 0;
    });
    const body = JSON.stringify({
      type: "order.updated",
      timestamp: new Date().toISOString(),
      api_version: "2026-10",
      data: h.mock.renderOrderRaw(order.id),
    });
    expect((await h.deliver({ id: "msg_contra_1", type: "order.updated", body })).status).toBe(200);
    const [row] = await h.q<{ status: string; refunded_cents: number }>(
      sql`select status, refunded_cents from orders where provider_order_id = ${order.id}`,
    );
    expect(row).toEqual({ status: "refunded", refunded_cents: 2900 });
    const [ev] = await h.q<{ status: string; reason: string }>(
      sql`select status, reason from webhook_events where provider_event_id = 'msg_contra_1'`,
    );
    expect(ev.status).toBe("processed");
    expect(ev.reason).toContain("contradiction");
    const alerts = await h.q(
      sql`select 1 from billing_alerts where kind = 'contradiction' and subject = ${`order:${order.id}`}`,
    );
    expect(alerts.length).toBe(1);
    const lic = await h.q<{ revoked_at: string | null }>(
      sql`select l.revoked_at from licenses l join orders o on o.id = l.order_id where o.provider_order_id = ${order.id}`,
    );
    expect(lic[0].revoked_at).not.toBeNull();
  });

  test("billed_at and paid_at are not moved by a later snapshot", async () => {
    h.mock.takeHeld();
    const b = await h.buy("desktop", null, { email: "moved@convt.test" });
    await h.deliverAll();
    const order = h.mock.state().orders.find((o) => o.checkout_id === b.providerCheckoutId)!;
    const [before] = await h.q<{ billed: string; paid: string }>(
      sql`select billed_at::text as billed, paid_at::text as paid from orders where provider_order_id = ${order.id}`,
    );
    h.mock.advance(5 * 86_400_000);
    h.mock.refund(order.id, 100);
    await h.deliverAll();
    const [after] = await h.q<{ billed: string; paid: string }>(
      sql`select billed_at::text as billed, paid_at::text as paid from orders where provider_order_id = ${order.id}`,
    );
    expect(after).toEqual(before);
  }, 20_000);
});
