// The mock against the real SDK client and the SDK's webhook validator: every API
// answer goes through createPolar's methods, and every delivery passes
// validateEvent under both key schemes. Payload shapes are also checked at compile
// time, because the mock builds them as the SDK's model types.

import { randomBytes } from "node:crypto";

import { createPolar, webhooks } from "@polar-sh/sdk/2026-10";
import { afterAll, beforeAll, describe, expect, test } from "bun:test";

import { createBillingMock, localProducts, type BillingMock } from "../src/mock";

const secret = `whsec_${randomBytes(32).toString("base64")}`;
const token = "polar_oat_mock";
let mock: BillingMock;
let server: ReturnType<typeof Bun.serve>;
let polar: ReturnType<typeof createPolar>;
let base: string;
const deliveries: Array<{ body: string; headers: Record<string, string> }> = [];

beforeAll(() => {
  server = Bun.serve({ hostname: "127.0.0.1", port: 0, fetch: (r) => mock.fetch(r) });
  base = `http://127.0.0.1:${server.port}`;
  mock = createBillingMock({
    publicUrl: base,
    accessToken: token,
    resendApiKey: "re_mock",
    webhook: { secret, mode: "hold" },
    sink: async (body, headers) => {
      deliveries.push({ body, headers });
      return 200;
    },
  });
  polar = createPolar({ accessToken: token, baseUrl: base });
});
afterAll(() => server.stop(true));

async function checkout(
  product: keyof typeof localProducts,
  opts: { externalId?: string; email?: string; trial?: boolean } = {},
) {
  const co = await polar.checkouts.create({
    products: [localProducts[product].productId],
    external_customer_id: opts.externalId ?? null,
    customer_email: opts.email ?? null,
    allow_trial: opts.trial ?? false,
    metadata: { convt_checkout: `chk_${product}` },
    success_url: "http://localhost:3000/checkout/success?checkout_id={CHECKOUT_ID}",
    currency: "usd",
  });
  return co;
}

describe("Polar API through the SDK client", () => {
  test("checkout, pay through the hosted page, list and fetch", async () => {
    const co = await checkout("desktop", { email: "guest@convt.test" });
    expect(co.url).toStartWith(`${base}/checkout/`);
    const page = await fetch(co.url);
    expect(await page.text()).toContain("convt Desktop");
    const res = await fetch(`${co.url}/confirm`, {
      method: "POST",
      body: new URLSearchParams({ action: "pay", email: "guest@convt.test", card: "4242" }),
      redirect: "manual",
    });
    expect(res.status).toBe(303);
    expect(res.headers.get("location")).toBe(
      `http://localhost:3000/checkout/success?checkout_id=${co.id}`,
    );
    const fetched = await polar.checkouts.get(co.id);
    expect(fetched.status).toBe("succeeded");
    const list = await polar.orders.list({ checkout_id: co.id });
    expect(list.items.length).toBe(1);
    const order = await polar.orders.get(list.items[0].id);
    expect(order).toMatchObject({
      status: "paid",
      billing_reason: "purchase",
      net_amount: 2900,
      currency: "usd",
    });
    expect(order.metadata.convt_checkout).toBe("chk_desktop");
    expect((await polar.orders.invoice(order.id)).url).toContain("/invoices/");
  });

  test("a trial, its $0 order, cancel and uncancel, revoke", async () => {
    const co = await checkout("pro_month", {
      externalId: "usr_trial",
      email: "t@convt.test",
      trial: true,
    });
    expect(mock.completeCheckout(co.id)).toEqual({ ok: true });
    const subId = (await polar.checkouts.get(co.id)).subscription_id!;
    const sub = await polar.subscriptions.get(subId);
    expect(sub.status).toBe("trialing");
    expect(sub.customer.external_id).toBe("usr_trial");
    const [order] = (await polar.orders.list({ subscription_id: subId })).items;
    expect(order).toMatchObject({
      status: "paid",
      net_amount: 0,
      billing_reason: "subscription_create",
    });
    const canceled = await polar.subscriptions.update(subId, { cancel_at_period_end: true });
    expect(canceled.cancel_at_period_end).toBe(true);
    await polar.subscriptions.update(subId, { cancel_at_period_end: false });
    const revoked = await polar.subscriptions.revoke(subId);
    expect(revoked.status).toBe("canceled");
    await expect(polar.subscriptions.revoke(subId)).rejects.toThrow(/403|AlreadyCanceled/);
  });

  test("switching interval charges a proration invoice, a declined card changes nothing, a downgrade credits", async () => {
    const co = await checkout("pro_month", { externalId: "usr_switch", email: "s@convt.test" });
    mock.completeCheckout(co.id);
    const subId = (await polar.checkouts.get(co.id)).subscription_id!;
    mock.setDecline("usr_switch", true);
    await expect(
      polar.subscriptions.update(subId, {
        product_id: localProducts.pro_year.productId,
        proration_behavior: "invoice",
      }),
    ).rejects.toThrow(/402/);
    expect((await polar.subscriptions.get(subId)).product_id).toBe(
      localProducts.pro_month.productId,
    );
    mock.setDecline("usr_switch", false);
    const up = await polar.subscriptions.update(subId, {
      product_id: localProducts.pro_year.productId,
      proration_behavior: "invoice",
    });
    expect(up.recurring_interval).toBe("year");
    const update = (await polar.orders.list({ subscription_id: subId })).items.find(
      (o) => o.billing_reason === "subscription_update",
    )!;
    expect(update.items.length).toBe(2);
    expect(update.items.some((i) => i.amount < 0 && i.proration)).toBe(true);
    expect(update.items.find((i) => i.amount > 0)!.end_timestamp).toBe(up.current_period_end);
    expect(update.net_amount).toBeGreaterThan(0);
    const down = await polar.subscriptions.update(subId, {
      product_id: localProducts.pro_month.productId,
      proration_behavior: "invoice",
    });
    expect(down.recurring_interval).toBe("month");
    const credit = (
      await polar.orders.list({ subscription_id: subId, sorting: ["created_at"] })
    ).items.at(-1)!;
    expect(credit.net_amount).toBeLessThan(0);
    // The next monthly period is paid from the credit balance.
    mock.cycle(subId);
    const next = (
      await polar.orders.list({ subscription_id: subId, sorting: ["created_at"] })
    ).items.at(-1)!;
    expect(next).toMatchObject({
      billing_reason: "subscription_cycle",
      status: "paid",
      applied_balance_amount: 1200,
    });
  });

  test("a failed renewal is past due; payment methods and the portal session", async () => {
    const co = await checkout("pro_year", { externalId: "usr_fail", email: "f@convt.test" });
    mock.completeCheckout(co.id);
    const subId = (await polar.checkouts.get(co.id)).subscription_id!;
    mock.setDecline("usr_fail", true);
    mock.cycle(subId);
    expect((await polar.subscriptions.get(subId)).status).toBe("past_due");
    const pm = await polar.customers.listPaymentMethodsExternal("usr_fail");
    expect(pm.items[0]).toMatchObject({ type: "card", method_metadata: { last4: "4242" } });
    const session = await polar.customerSessions.create({
      external_customer_id: "usr_fail",
      return_url: "http://localhost:3000/dashboard/billing",
    });
    expect(new URL(session.customer_portal_url).origin).toBe(base);
    await fetch(`${session.customer_portal_url}/card`, { method: "POST" });
    expect((await polar.subscriptions.get(subId)).status).toBe("active");
  });

  test("refunds, disputes and settings", async () => {
    const co = await checkout("desktop", { email: "r@convt.test" });
    mock.completeCheckout(co.id);
    const [order] = (await polar.orders.list({ checkout_id: co.id })).items;
    mock.refund(order.id, 1000);
    expect((await polar.orders.get(order.id)).status).toBe("partially_refunded");
    const d = mock.openDispute(order.id);
    mock.closeDispute(d.id, "lost");
    expect((await polar.disputes.get(d.id)).status).toBe("lost");
    const listed = await polar.disputes.list({ sorting: ["created_at"], page: 1, limit: 10 });
    expect(listed.items.some((x) => x.id === d.id)).toBe(true);
    const refunds = await polar.refunds.list({ sorting: ["created_at"] });
    expect(refunds.items.some((r) => r.order_id === order.id)).toBe(true);
    const org = (await polar.organizations.list()).items[0];
    expect(org.subscription_settings.allow_multiple_subscriptions).toBe(true);
    const products = await polar.products.list({});
    expect(products.items.length).toBe(4);
  });

  test("with multiple subscriptions off, a second subscription checkout fails", async () => {
    mock.settings.allowMultipleSubscriptions = false;
    try {
      const a = await checkout("pro_month", { externalId: "usr_multi", email: "m@convt.test" });
      expect(mock.completeCheckout(a.id)).toEqual({ ok: true });
      const b = await checkout("api", { externalId: "usr_multi", email: "m@convt.test" });
      expect(mock.completeCheckout(b.id)).toMatchObject({ ok: false });
      expect((await polar.checkouts.get(b.id)).status).toBe("failed");
    } finally {
      mock.settings.allowMultipleSubscriptions = true;
    }
  });

  test("pagination: pages in ascending created_at stay stable as new objects append", async () => {
    const p1 = await polar.orders.list({ sorting: ["created_at"], page: 1, limit: 2 });
    const all = await polar.orders.list({ sorting: ["created_at"], page: 1, limit: 100 });
    expect(p1.items.map((o) => o.id)).toEqual(all.items.slice(0, 2).map((o) => o.id));
    expect(p1.pagination.max_page).toBe(Math.ceil(all.items.length / 2));
  });

  test("a wrong access token is refused", async () => {
    const bad = createPolar({ accessToken: "nope", baseUrl: base });
    await expect(bad.orders.list({})).rejects.toThrow(/401/);
  });
});

describe("webhooks", () => {
  test("every delivery passes the SDK's validateEvent, under both key schemes", async () => {
    const held = mock.takeHeld();
    expect(held.length).toBeGreaterThan(20);
    const types = new Set<string>();
    for (const scheme of ["standard", "legacy"] as const) {
      for (const d of held) {
        const headers = mock.sign(d, { scheme });
        const event = await webhooks.validateEvent(d.body, headers, secret);
        expect(event.type).toBe(d.type as typeof event.type);
        types.add(event.type);
      }
    }
    for (const t of [
      "order.created",
      "order.paid",
      "order.refunded",
      "subscription.created",
      "subscription.past_due",
      "checkout.created",
      "checkout.updated",
      "refund.created",
      "customer.created",
    ])
      expect(types.has(t)).toBe(true);
  });

  test("a forged delivery fails validateEvent", async () => {
    await checkout("desktop", { email: "x@convt.test" });
    const [d] = mock.takeHeld();
    const forged = mock.sign(d, { secret: `whsec_${randomBytes(32).toString("base64")}` });
    await expect(webhooks.validateEvent(d.body, forged, secret)).rejects.toThrow(
      webhooks.PolarWebhookVerificationError,
    );
  });

  test("auto mode duplicates, drops and retries failed deliveries", async () => {
    let fail = 1;
    const got: string[] = [];
    const m = createBillingMock({
      publicUrl: base,
      accessToken: token,
      resendApiKey: "re",
      webhook: { secret, mode: "auto", duplicate: 1, retryBaseMs: 1 },
      sink: async (body) => {
        got.push(JSON.parse(body).type);
        return fail-- > 0 ? 500 : 200;
      },
    });
    const res = await m.fetch(
      new Request(`${base}/v1/checkouts/`, {
        method: "POST",
        headers: { authorization: `Bearer ${token}`, "content-type": "application/json" },
        body: JSON.stringify({ products: [localProducts.desktop.productId] }),
      }),
    );
    expect(res.status).toBe(201);
    await m.settle();
    // Two copies; the first failed once and was retried.
    expect(got).toEqual(["checkout.created", "checkout.created", "checkout.created"]);
    m.webhook.dropNext = 1;
    m.webhook.duplicate = 0;
    got.length = 0;
    await m.fetch(
      new Request(`${base}/v1/checkouts/`, {
        method: "POST",
        headers: { authorization: `Bearer ${token}`, "content-type": "application/json" },
        body: JSON.stringify({ products: [localProducts.desktop.productId] }),
      }),
    );
    await m.settle();
    expect(got).toEqual([]);
  });
});

describe("Resend", () => {
  const send = (key: string, body: object, auth = "re_mock") =>
    mock.fetch(
      new Request(`${base}/emails`, {
        method: "POST",
        headers: {
          authorization: `Bearer ${auth}`,
          "content-type": "application/json",
          "idempotency-key": key,
        },
        body: JSON.stringify(body),
      }),
    );
  const email = {
    from: "convt <hello@convt.test>",
    to: ["a@convt.test"],
    subject: "Hi",
    text: "t",
  };

  test("the same key and payload returns the first response; another payload is 409", async () => {
    const a = await send("eml_1", email);
    const b = await send("eml_1", email);
    expect(a.status).toBe(200);
    expect(await b.json()).toEqual(await a.json());
    const c = await send("eml_1", { ...email, subject: "Other" });
    expect(c.status).toBe(409);
    expect((await c.json()).name).toBe("invalid_idempotent_request");
    expect(mock.resend.sent.filter((e) => e.key === "eml_1").length).toBe(1);
  });

  test("a key whose first request is still running is 409 concurrent_idempotent_requests", async () => {
    mock.resend.addFault({ kind: "delay", ms: 150 });
    const first = send("eml_2", email);
    await Bun.sleep(30);
    const second = await send("eml_2", email);
    expect(second.status).toBe(409);
    expect((await second.json()).name).toBe("concurrent_idempotent_requests");
    expect((await first).status).toBe(200);
  });

  test("5xx accepts nothing; keys expire after 24 hours on the mock clock", async () => {
    mock.resend.addFault({ kind: "5xx" });
    expect((await send("eml_3", email)).status).toBe(500);
    expect(mock.resend.sent.some((e) => e.key === "eml_3")).toBe(false);
    expect((await send("eml_3", email)).status).toBe(200);
    mock.advance(25 * 3600_000);
    // After expiry the same key with another payload is accepted as new: a second copy.
    expect((await send("eml_3", { ...email, subject: "Again" })).status).toBe(200);
    expect(mock.resend.sent.filter((e) => e.key === "eml_3").length).toBe(2);
  });

  test("a wrong API key and an invalid address are refused", async () => {
    expect((await send("eml_4", email, "bad")).status).toBe(401);
    expect((await send("eml_5", { ...email, to: ["not-an-address"] })).status).toBe(422);
  });
});
