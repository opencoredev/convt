// Forged deliveries change nothing; signed facts that fail our business checks
// are rejected without an entitlement; failures after verification are stored
// and finished by a redelivery or a replay.

import { randomBytes } from "node:crypto";

import { afterAll, beforeAll, describe, expect, test } from "bun:test";
import { localProducts, type HeldDelivery } from "@convt/billing-mock";
import { sql } from "drizzle-orm";

import { replayFailedEvents } from "../../src/webhook";
import { createHarness, type Harness } from "../../src/testing";

let h: Harness;
beforeAll(async () => {
  h = await createHarness();
});
afterAll(async () => h?.close());

const bytes = (s: string) => new TextEncoder().encode(s);

async function snapshot() {
  const tables = [
    "orders",
    "invoices",
    "subscriptions",
    "licenses",
    "email_outbox",
    "webhook_events",
    "payment_coverage",
    "disputes",
    "billing_customers",
    "billing_alerts",
    "checkouts",
  ];
  const out: Record<string, string> = {};
  for (const t of tables) {
    const [r] = await h.q<{ h: string }>(
      sql`select md5(coalesce(string_agg(x::text, ',' order by x::text), '')) as h from ${sql.identifier(t)} x`,
    );
    out[t] = r.h;
  }
  return out;
}

describe("forged signatures", () => {
  test("every forgery is refused with 401 or 413 and leaves every table unchanged", async () => {
    await h.buy("desktop", null, { email: "forge@convt.test" });
    const held = h.mock.takeHeld();
    const paid = held.find((d) => d.type === "order.paid")!;
    const before = await snapshot();
    const now = Math.floor(Date.now() / 1000);
    const other = `whsec_${randomBytes(32).toString("base64")}`;
    const send = (body: string, headers: Record<string, string>) =>
      h.service.handleWebhook("POST", bytes(body), new Headers(headers));
    const good = h.mock.sign(paid);
    const cases: Array<[string, Promise<{ status: number }>]> = [
      ["wrong secret", send(paid.body, h.mock.sign(paid, { secret: other }))],
      [
        "wrong secret, legacy",
        send(paid.body, h.mock.sign(paid, { secret: other, scheme: "legacy" })),
      ],
      [
        "one body byte changed",
        send(paid.body.replace('"paid"', '"paid"').replace("forge@", "forgf@"), good),
      ],
      [
        "valid signature for another webhook-id",
        send(paid.body, { ...good, "webhook-id": "msg_other" }),
      ],
      ["6 minutes old", send(paid.body, h.mock.sign(paid, { timestamp: now - 360 }))],
      ["6 minutes ahead", send(paid.body, h.mock.sign(paid, { timestamp: now + 360 }))],
      ["missing headers", send(paid.body, { "content-type": "application/json" })],
      [
        "garbage signature header",
        send(paid.body, { ...good, "webhook-signature": "v1,@@@ v9,xyz nonsense" }),
      ],
      [
        "oversized body",
        send(
          "x".repeat(256 * 1024 + 1),
          h.mock.sign({ ...paid, body: "x".repeat(256 * 1024 + 1) }),
        ),
      ],
    ];
    for (const [name, p] of cases) {
      const r = await p;
      expect({ name, ok: r.status === 401 || r.status === 413 }).toEqual({ name, ok: true });
    }
    expect(await snapshot()).toEqual(before);
    // A GET is not a delivery either.
    expect((await h.service.handleWebhook("GET", bytes(""), new Headers())).status).toBe(405);
    expect(await snapshot()).toEqual(before);
  });

  test("one bad and one good signature is accepted; both key schemes are accepted", async () => {
    await h.buy("desktop", null, { email: "both@convt.test" });
    const held = h.mock.takeHeld();
    const [a, b] = held;
    const bad = h.mock.sign(a, { secret: `whsec_${randomBytes(32).toString("base64")}` })[
      "webhook-signature"
    ];
    const good = h.mock.sign(a)["webhook-signature"];
    const headers = { ...h.mock.sign(a), "webhook-signature": `${bad} ${good}` };
    expect(
      (await h.service.handleWebhook("POST", bytes(a.body), new Headers(headers))).status,
    ).toBe(200);
    expect((await h.deliver(b, { scheme: "legacy" })).status).toBe(200);
    for (const d of held.slice(2))
      expect((await h.deliver(d, { scheme: "legacy" })).status).toBe(200);
  });
});

/** A signed delivery of a hand-made order body. */
async function deliverOrder(
  mutate: (data: Record<string, unknown>) => void,
  id = `msg_${randomBytes(6).toString("hex")}`,
) {
  const b = await h.buy("desktop", null, { email: `check-${id}@convt.test` });
  const held = h.mock.takeHeld();
  const paid = held.find((d) => d.type === "order.paid")!;
  const env = JSON.parse(paid.body);
  mutate(env.data);
  const d: HeldDelivery = { id, type: "order.paid", body: JSON.stringify(env) };
  const r = await h.deliver(d);
  const [ev] = await h.q<{ status: string; reason: string }>(
    sql`select status, reason from webhook_events where provider_event_id = ${id}`,
  );
  const lic = await h.q(
    sql`select 1 from licenses l join orders o on o.id = l.order_id where o.provider_order_id = ${env.data.id}`,
  );
  return { status: r.status, event: ev, licenses: lic.length, checkout: b };
}

describe("business checks", () => {
  test("each failing order is rejected with 200 and gets no license", async () => {
    const cases: Array<[string, (d: Record<string, unknown>) => void, RegExp]> = [
      [
        "unknown product",
        (d) => {
          d.product_id = "prod_unknown";
        },
        /unknown_product/,
      ],
      [
        "unknown price",
        (d) => {
          (d.items as Array<Record<string, unknown>>)[0].product_price_id = "price_unknown";
        },
        /unknown_price/,
      ],
      [
        "wrong amount",
        (d) => {
          (d.items as Array<Record<string, unknown>>)[0].amount = 1900;
          d.subtotal_amount = 1900;
          d.net_amount = 1900;
        },
        /amount/,
      ],
      [
        "a discount",
        (d) => {
          d.discount_id = "disc_1";
          d.discount_amount = 500;
        },
        /discount/,
      ],
      [
        "EUR",
        (d) => {
          d.currency = "eur";
        },
        /currency/,
      ],
      [
        "a checkout we did not create",
        (d) => {
          d.checkout_id = "co_foreign";
          d.metadata = {};
        },
        /foreign_checkout/,
      ],
    ];
    for (const [name, mutate, reason] of cases) {
      const r = await deliverOrder(mutate);
      expect({ name, status: r.status, event: r.event.status, licenses: r.licenses }).toEqual({
        name,
        status: 200,
        event: "rejected",
        licenses: 0,
      });
      expect(r.event.reason).toMatch(reason);
    }
  });

  test("another user's checkout and a changed customer id are rejected", async () => {
    const alice = await h.user("alice@convt.test");
    const mallory = await h.user("mallory@convt.test");
    // Alice's checkout, but an order whose customer is Mallory.
    const created = await h.service.createCheckout({ product: "desktop", user: alice });
    if (!created.ok) throw new Error("refused");
    const co = h.mock.checkoutBySecret(created.url.split("/checkout/")[1])!;
    h.mock.completeCheckout(co.id, "4242", alice.email);
    const held = h.mock.takeHeld();
    const paid = held.find((d) => d.type === "order.paid")!;
    const env = JSON.parse(paid.body);
    env.data.customer.external_id = mallory.id;
    const r = await h.deliver({
      id: "msg_other_user",
      type: "order.paid",
      body: JSON.stringify(env),
    });
    expect(r.status).toBe(200);
    const [ev] = await h.q<{ status: string; reason: string }>(
      sql`select status, reason from webhook_events where provider_event_id = 'msg_other_user'`,
    );
    expect(ev.status).toBe("rejected");
    expect(ev.reason).toMatch(/checkout_user/);
    // The genuine events map Alice to her Polar customer; then a fact for Alice from another customer.
    for (const d of held) await h.deliver(d);
    const env2 = JSON.parse(paid.body);
    env2.data.id = "ord_other_customer";
    env2.data.customer_id = "cus_someone_else";
    env2.data.customer.id = "cus_someone_else";
    const r2 = await h.deliver({
      id: "msg_changed_customer",
      type: "order.paid",
      body: JSON.stringify(env2),
    });
    expect(r2.status).toBe(200);
    const [ev2] = await h.q<{ status: string; reason: string }>(
      sql`select status, reason from webhook_events where provider_event_id = 'msg_changed_customer'`,
    );
    expect(ev2.status).toBe("rejected");
    expect(ev2.reason).toMatch(/customer_changed/);
  });

  test("a Pro subscription citing another user's Pro checkout is rejected, even as a switched product", async () => {
    const a = await h.user("owner-pro@convt.test");
    const b = await h.user("thief-pro@convt.test");
    await h.buy("pro_month", a);
    const held = h.mock.takeHeld();
    const created = held.find((d) => d.type === "subscription.created")!;
    const env = JSON.parse(created.body);
    env.data.id = "sub_stolen_checkout";
    env.data.customer.external_id = b.id;
    env.data.product_id = localProducts.pro_year.productId;
    expect(
      (
        await h.deliver({
          id: "msg_stolen_checkout",
          type: "subscription.created",
          body: JSON.stringify(env),
        })
      ).status,
    ).toBe(200);
    const [ev] = await h.q<{ status: string; reason: string }>(
      sql`select status, reason from webhook_events where provider_event_id = 'msg_stolen_checkout'`,
    );
    expect(ev.status).toBe("rejected");
    expect(ev.reason).toMatch(/checkout_user/);
    for (const d of held) await h.deliver(d);
  });

  test("a fact fetched on an equal-version conflict passes the same checks", async () => {
    const b = await h.buy("desktop", null, { email: "fetchcheck@convt.test" });
    const held = h.mock.takeHeld();
    for (const d of held) await h.deliver(d);
    const paid = held.find((d) => d.type === "order.paid")!;
    const order = h.mock.state().orders.find((o) => o.checkout_id === b.providerCheckoutId)!;
    // At the provider the order quietly gains a discount without a new version.
    h.mock.mutateQuietly(
      "order",
      order.id,
      (o) => {
        o.discountId = "disc_quiet";
        o.discountAmount = 500;
      },
      true,
    );
    // A valid-looking snapshot with the same version but other content forces a fetch.
    const env = JSON.parse(paid.body);
    env.data.description = "changed";
    expect(
      (await h.deliver({ id: "msg_fetch_check", type: "order.updated", body: JSON.stringify(env) }))
        .status,
    ).toBe(200);
    const [ev] = await h.q<{ reason: string }>(
      sql`select reason from webhook_events where provider_event_id = 'msg_fetch_check'`,
    );
    expect(ev.reason).toMatch(/fetched fact rejected \(discount/);
    expect(
      (
        await h.q(
          sql`select 1 from billing_alerts where kind = 'rejected_fact' and subject = ${`fetch:${order.id}`}`,
        )
      ).length,
    ).toBe(1);
  });

  test("signed but malformed bodies are rejected and alerted", async () => {
    for (const [id, body] of [
      ["msg_malformed_1", "{"],
      ["msg_malformed_2", JSON.stringify({ type: "order.paid", timestamp: "x", data: { id: 1 } })],
      ["msg_malformed_3", JSON.stringify({ nope: true })],
    ]) {
      const r = await h.deliver({ id, type: "x", body });
      expect(r.status).toBe(200);
      const [ev] = await h.q<{ status: string }>(
        sql`select status from webhook_events where provider_event_id = ${id}`,
      );
      expect(ev.status).toBe("rejected");
      const alerts = await h.q(
        sql`select 1 from billing_alerts where kind = 'rejected_event' and subject = ${id}`,
      );
      expect(alerts.length).toBe(1);
    }
  });

  test("ignored event types are recorded as ignored", async () => {
    const body = JSON.stringify({
      type: "benefit.created",
      timestamp: new Date().toISOString(),
      api_version: "2026-10",
      data: {},
    });
    expect((await h.deliver({ id: "msg_benefit", type: "benefit.created", body })).status).toBe(
      200,
    );
    const [ev] = await h.q<{ status: string }>(
      sql`select status from webhook_events where provider_event_id = 'msg_benefit'`,
    );
    expect(ev.status).toBe("ignored");
  });

  test("an API subscription whose checkout is not ours is rejected", async () => {
    const u = await h.user("apiforeign@convt.test");
    // A checkout created at the provider directly, not through convt-billing.
    const res = await fetch(`${h.base}/v1/checkouts/`, {
      method: "POST",
      headers: {
        authorization: `Bearer ${(h.provider as unknown as { name: string }).name && ""}`,
      },
    }).catch(() => null);
    void res;
    const co = await h.provider.createCheckout({
      product: "api",
      checkoutRef: "chk_00000000000000000000000000",
      successUrl: "http://x",
      allowTrial: false,
      externalCustomerId: u.id,
      email: u.email,
    });
    h.mock.completeCheckout(co.providerCheckoutId);
    const results = await h.deliverAll();
    expect(results.every((r) => r.status === 200)).toBe(true);
    const subs = await h.q(sql`select 1 from subscriptions where user_id = ${u.id}`);
    expect(subs.length).toBe(0);
    const rejected = await h.q<{ reason: string }>(
      sql`select reason from webhook_events where status = 'rejected' and reason like 'foreign_checkout%'`,
    );
    expect(rejected.length).toBeGreaterThan(0);
  });
});

describe("failed events", () => {
  test("a fault after verification leaves a failed row with the body and attempts 1; a redelivery processes it", async () => {
    await h.buy("desktop", null, { email: "fail1@convt.test" });
    const held = h.mock.takeHeld();
    const paid = held.find((d) => d.type === "order.paid")!;
    h.failAt("before-commit");
    expect((await h.deliver(paid)).status).toBe(500);
    const [ev] = await h.q<{ status: string; attempts: number; body: string; reason: string }>(sql`
      select status, attempts, body, reason from webhook_events where provider_event_id = ${paid.id}`);
    expect(ev).toMatchObject({ status: "failed", attempts: 1, body: paid.body });
    expect(ev.reason).not.toContain("fail1@convt.test");
    h.clearFaults();
    expect((await h.deliver(paid)).status).toBe(200);
    const [ev2] = await h.q<{ status: string; attempts: number }>(
      sql`select status, attempts from webhook_events where provider_event_id = ${paid.id}`,
    );
    expect(ev2).toEqual({ status: "processed", attempts: 2 });
  });

  test("the reconciler replays a stored failed event from its body", async () => {
    await h.buy("desktop", null, { email: "fail2@convt.test" });
    const held = h.mock.takeHeld();
    const paid = held.find((d) => d.type === "order.paid")!;
    h.failAt("before-commit");
    await h.deliver(paid);
    h.clearFaults();
    const r = await h.service.withCtx((c) => replayFailedEvents(c));
    expect(r.processed).toBeGreaterThanOrEqual(1);
    const [ev] = await h.q<{ status: string }>(
      sql`select status from webhook_events where provider_event_id = ${paid.id}`,
    );
    expect(ev.status).toBe("processed");
    const lic = await h.q(sql`select 1 from licenses where email = 'fail2@convt.test'`);
    expect(lic.length).toBe(1);
  });

  test("ten failures make the event dead and alerted, and it is no longer replayed", async () => {
    await h.buy("desktop", null, { email: "fail3@convt.test" });
    const held = h.mock.takeHeld();
    const paid = held.find((d) => d.type === "order.paid")!;
    h.failAt("before-commit", 100);
    for (let i = 0; i < 10; i++) expect((await h.deliver(paid)).status).toBe(500);
    const [ev] = await h.q<{ status: string; attempts: number }>(
      sql`select status, attempts from webhook_events where provider_event_id = ${paid.id}`,
    );
    expect(ev).toEqual({ status: "dead", attempts: 10 });
    expect(
      (
        await h.q(
          sql`select 1 from billing_alerts where kind = 'dead_event' and subject = ${paid.id}`,
        )
      ).length,
    ).toBe(1);
    h.clearFaults();
    // A dead event is answered 200 and not processed again.
    expect((await h.deliver(paid)).status).toBe(200);
    expect((await h.q(sql`select 1 from licenses where email = 'fail3@convt.test'`)).length).toBe(
      0,
    );
  });

  test("a signing key that fails to load is a 500 before the transaction", async () => {
    const before = await snapshot();
    const svc = (await import("../../src/service")).createBillingService({
      connect: async () => {
        const { connect } = await import("@convt/db");
        const { client, db } = await connect(h.tdb.billingUrl);
        return { db, close: () => client.end() };
      },
      provider: h.provider,
      catalog: h.catalog,
      mail: { name: "log", send: async () => ({ ok: true, id: "x" }) },
      signingKey: async () => {
        throw new Error("LICENSE_SIGNING_KEY is malformed");
      },
      config: {
        siteUrl: "http://localhost:3000",
        mailFrom: "a <a@b.c>",
        alertEmail: null,
        downloadUrl: "x",
        budgetMs: 5000,
        checkoutCookie: "c",
      },
      clock: () => h.mock.now(),
      log: () => {},
    });
    await h.buy("desktop", null, { email: "nokey@convt.test" });
    const paid = h.mock.takeHeld().find((d) => d.type === "order.paid")!;
    const r = await svc.handleWebhook("POST", bytes(paid.body), new Headers(h.mock.sign(paid)));
    expect(r.status).toBe(500);
    expect((await h.q(sql`select 1 from licenses where email = 'nokey@convt.test'`)).length).toBe(
      0,
    );
    const [ev] = await h.q<{ status: string }>(
      sql`select status from webhook_events where provider_event_id = ${paid.id}`,
    );
    expect(ev.status).toBe("failed");
    void before;
    void localProducts;
  });
});
