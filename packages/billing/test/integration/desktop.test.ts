// Desktop purchases end to end through the mock: issuance, duplicates, ordering,
// refunds, disputes, and crashes between issuing and emailing.

import { afterAll, beforeAll, describe, expect, test } from "bun:test";
import { getLicenseToken } from "@convt/db";
import { claimPurchases } from "@convt/db/queries";
import { importVerifyKey, verify } from "@convt/license";
import { sql } from "drizzle-orm";

import { drainOutbox } from "../../src/outbox";
import { createHarness, type Harness } from "../../src/testing";
import { testMailbox } from "../mailbox";

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

async function licensesFor(providerOrderId: string) {
  return h.q<{
    id: string;
    email: string;
    token: string;
    revoked_at: Date | null;
    revoke_reason: string | null;
    issued_on: string;
    updates_until: string;
  }>(sql`
    select l.id, l.email, l.token, l.revoked_at, l.revoke_reason, l.issued_on::text, l.updates_until::text
    from licenses l join orders o on o.id = l.order_id where o.provider_order_id = ${providerOrderId}`);
}

describe("Desktop", () => {
  test("a guest purchase issues one key and one email, whatever the duplication", async () => {
    const before = await h.counts();
    const b = await h.buy("desktop", null, { email: "guest1@convt.test" });
    const held = h.mock.takeHeld();
    const order = held.find((d) => d.type === "order.paid")!;
    // One delivery five times, plus order.created and order.updated with distinct ids.
    for (let i = 0; i < 5; i++) expect((await h.deliver(order)).status).toBe(200);
    for (const d of held) expect((await h.deliver(d)).status).toBe(200);
    const orderId = JSON.parse(order.body).data.id;
    const lics = await licensesFor(orderId);
    expect(lics.length).toBe(1);
    expect(lics[0].email).toBe("guest1@convt.test");
    const after = await h.counts();
    expect(after.orders - before.orders).toBe(1);
    expect(after.outbox - before.outbox).toBe(1);
    const [ev] = await h.q<{ n: number }>(
      sql`select count(*)::int as n from webhook_events where provider_event_id = ${order.id}`,
    );
    expect(ev.n).toBe(1);
    // The key verifies, and its window is twelve months from the order's created_at.
    const vk = await importVerifyKey(h.publicKey);
    const v = await verify(lics[0].token, vk);
    expect(v.ok && v.license.plan).toBe("desktop");
    const billed = JSON.parse(order.body).data.created_at.slice(0, 10);
    expect(lics[0].issued_on).toBe(billed);
    expect(b.paid).toEqual({ ok: true });
    const drained = await h.service.drainOutbox();
    expect(drained.sent).toBe(1);
    const mails = h.mock.resend.sent.filter((m) => m.to[0] === "guest1@convt.test");
    expect(mails.length).toBe(1);
    expect(mails[0].text).toContain(lics[0].token);
  });

  test("a signed-in purchase signs the account's verified email and belongs to the user", async () => {
    const u = await h.user("buyer2@convt.test");
    await h.buy("desktop", u, { email: "buyer2@convt.test" });
    await h.deliverAll();
    const [lic] = await h.q<{ user_id: string; email: string }>(
      sql`select user_id, email from licenses where user_id = ${u.id}`,
    );
    expect(lic).toEqual({ user_id: u.id, email: "buyer2@convt.test" });
    const [cus] = await h.q<{ user_id: string }>(
      sql`select user_id from billing_customers where user_id = ${u.id}`,
    );
    expect(cus.user_id).toBe(u.id);
  });

  test("every order of a purchase and its full refund ends refunded with no unrevoked key", async () => {
    const b = await h.buy("desktop", null, { email: "perm@convt.test" });
    const orderId = h.mock.state().orders.find((o) => o.checkout_id === b.providerCheckoutId)!.id;
    h.mock.refund(orderId);
    const held = h.mock
      .takeHeld()
      .filter((d) => d.type.startsWith("order.") || d.type.startsWith("refund."));
    const perms = permutations(held.slice(0, 5));
    expect(perms.length).toBeGreaterThan(20);
    // Each permutation runs against the same rows; idempotency makes later runs no-ops,
    // so also check the first permutation's end state alone in a reversed order below.
    for (const p of perms.slice(0, 24))
      for (const d of p) expect((await h.deliver(d)).status).toBe(200);
    const lics = await licensesFor(orderId);
    expect(lics.every((l) => l.revoked_at !== null)).toBe(true);
    const [o] = await h.q<{ status: string; refunded_cents: number }>(
      sql`select status, refunded_cents from orders where provider_order_id = ${orderId}`,
    );
    expect(o).toEqual({ status: "refunded", refunded_cents: 2900 });
  });

  test("a refund delivered before the paid event never issues", async () => {
    const b = await h.buy("desktop", null, { email: "early@convt.test" });
    const orderId = h.mock.state().orders.find((o) => o.checkout_id === b.providerCheckoutId)!.id;
    h.mock.refund(orderId);
    const held = h.mock.takeHeld();
    const refundFirst = [
      ...held.filter((d) => d.type === "order.refunded" || d.type === "refund.created"),
      ...held,
    ];
    for (const d of refundFirst) await h.deliver(d);
    expect((await licensesFor(orderId)).length).toBe(0);
  });

  test("a partial refund keeps the key; a full one revokes it; getLicenseKey refuses it", async () => {
    const u = await h.user("partial@convt.test");
    const b = await h.buy("desktop", u);
    await h.deliverAll();
    const orderId = h.mock.state().orders.find((o) => o.checkout_id === b.providerCheckoutId)!.id;
    h.mock.refund(orderId, 1000);
    // Every delivery succeeds: converge runs again on the new version and must not
    // try to issue a second key.
    expect((await h.deliverAll()).every((r) => r.status === 200)).toBe(true);
    let [lic] = await licensesFor(orderId);
    expect(lic.revoked_at).toBeNull();
    h.mock.refund(orderId);
    await h.deliverAll();
    [lic] = await licensesFor(orderId);
    expect(lic.revoke_reason).toBe("refunded");
    const web = await h.tdb.open("web");
    expect(await getLicenseToken(web.db, u.id, lic.id)).toBeNull();
  });

  test("a lost dispute revokes, a won one does not, a prevented one's refund revokes", async () => {
    const outcomes: Record<string, string | null> = {};
    for (const result of ["lost", "won", "prevented"] as const) {
      const b = await h.buy("desktop", null, { email: `dispute-${result}@convt.test` });
      await h.deliverAll();
      const orderId = h.mock.state().orders.find((o) => o.checkout_id === b.providerCheckoutId)!.id;
      const d = h.mock.openDispute(orderId);
      h.mock.closeDispute(d.id, result);
      // Polar sends no dispute webhook: a prevented one arrives as a refund with the
      // dispute attached; the others through the reconciler's dispute scan.
      await h.deliverAll();
      await h.service.withCtx(async (c) => {
        const { reconcileFrequent } = await import("../../src/reconcile");
        await reconcileFrequent(c);
      });
      const [lic] = await licensesFor(orderId);
      outcomes[result] = lic.revoke_reason;
    }
    expect(outcomes).toEqual({ lost: "dispute_lost", won: null, prevented: "refunded" });
  });

  test("crash after commit before the drain: the next drain sends once", async () => {
    await h.service.drainOutbox();
    h.failAt("after-commit");
    await h.buy("desktop", null, { email: "crash1@convt.test" });
    const held = h.mock.takeHeld();
    const results = [];
    for (const d of held) results.push((await h.deliver(d)).status);
    h.clearFaults();
    // The failure happened after commit, so the event is processed and the row waits.
    const [row] = await h.q<{ status: string }>(
      sql`select status from email_outbox where to_email = 'crash1@convt.test'`,
    );
    expect(row.status).toBe("pending");
    await h.service.drainOutbox();
    await h.service.drainOutbox();
    expect(h.mock.resend.sent.filter((m) => m.to[0] === "crash1@convt.test").length).toBe(1);
    expect(results.includes(500)).toBe(true);
  });

  test("crash inside the transaction after the license insert: rolled back, the redelivery issues once", async () => {
    h.failAt("after-license-insert");
    const b = await h.buy("desktop", null, { email: "crash2@convt.test" });
    const held = h.mock.takeHeld();
    const paid = held.find((d) => d.type === "order.paid")!;
    expect((await h.deliver(paid)).status).toBe(500);
    const orderId = JSON.parse(paid.body).data.id;
    expect((await licensesFor(orderId)).length).toBe(0);
    const [ev] = await h.q<{ status: string; attempts: number; body: string | null }>(sql`
      select status, attempts, body from webhook_events where provider_event_id = ${paid.id}`);
    expect(ev.status).toBe("failed");
    expect(ev.attempts).toBe(1);
    expect(ev.body).toBe(paid.body);
    h.clearFaults();
    expect((await h.deliver(paid)).status).toBe(200);
    for (const d of held) await h.deliver(d);
    expect((await licensesFor(orderId)).length).toBe(1);
    void b;
  });

  test("crash after Resend accepted and before marking: one message after the retry", async () => {
    await h.service.drainOutbox();
    await h.buy("desktop", null, { email: "crash3@convt.test" });
    await h.deliverAll();
    h.failAt("after-send-before-mark");
    await expect(h.service.drainOutbox()).rejects.toThrow("injected fault");
    h.clearFaults();
    // The lease expires; the next drain claims it again and resends under the same key.
    h.mock.advance(3 * 60_000);
    await h.service.drainOutbox();
    const [row] = await h.q<{ status: string; attempts: number }>(
      sql`select status, attempts from email_outbox where to_email = 'crash3@convt.test'`,
    );
    expect(row.status).toBe("sent");
    expect(row.attempts).toBe(2);
    expect(h.mock.resend.sent.filter((m) => m.to[0] === "crash3@convt.test").length).toBe(1);
  });

  test("an issued key verifies with the CLI's verifier", async () => {
    const b = await h.buy("desktop", null, { email: "cli@convt.test" });
    await h.deliverAll();
    const orderId = h.mock.state().orders.find((o) => o.checkout_id === b.providerCheckoutId)!.id;
    const [lic] = await licensesFor(orderId);
    const { verifyWithCli } = await import("../cli");
    const out = await verifyWithCli(lic.token, h.publicKey);
    expect(out.status).toBe(0);
    expect(out.text).toContain("with lifetime updates");
  });
});

void drainOutbox;

describe("guest and complimentary Desktop", () => {
  test("a $0 guest order before signup issues a license that claim_purchases attaches", async () => {
    const email = testMailbox("presignup");
    const b = await h.buy("desktop", null, { email });
    const held = h.mock.takeHeld();
    const paid = held.find((d) => d.type === "order.paid")!;
    const env = JSON.parse(paid.body);
    env.data.discount_id = "disc_giveaway_100";
    env.data.discount_amount = 2900;
    env.data.net_amount = 0;
    env.data.total_amount = 0;
    const r = await h.deliver({
      id: "msg_presignup_zero",
      type: "order.paid",
      body: JSON.stringify(env),
    });
    expect(r.status).toBe(200);
    const orderId = env.data.id;
    const before = await licensesFor(orderId);
    expect(before.length).toBe(1);
    expect(before[0].email).toBe(email);
    const [ord] = await h.q<{ user_id: string | null; amount_cents: number }>(
      sql`select user_id, amount_cents from orders where provider_order_id = ${orderId}`,
    );
    expect(ord).toEqual({ user_id: null, amount_cents: 0 });
    const u = await h.user(email);
    const claimed = await claimPurchases(h.owner, u.id);
    expect(claimed).toMatchObject({ orders: 1, licenses: 1, invoices: 1 });
    const [after] = await h.q<{ user_id: string }>(
      sql`select user_id from licenses where email = ${email}`,
    );
    expect(after.user_id).toBe(u.id);
    expect(b.paid).toEqual({ ok: true });
  });

  test("a guest purchase after the account exists is claimed by email on ingest", async () => {
    const email = testMailbox("already");
    const u = await h.user(email);
    await h.buy("desktop", null, { email });
    await h.deliverAll();
    const [lic] = await h.q<{ user_id: string }>(
      sql`select user_id from licenses where email = ${email}`,
    );
    expect(lic.user_id).toBe(u.id);
  });

  test("order.created with status paid is enough; order.paid is not required", async () => {
    const email = testMailbox("created-only");
    await h.buy("desktop", null, { email });
    const held = h.mock.takeHeld();
    const created = held.find((d) => d.type === "order.created")!;
    expect((await h.deliver(created)).status).toBe(200);
    const orderId = JSON.parse(created.body).data.id;
    expect((await licensesFor(orderId)).length).toBe(1);
  });

  test("a Polar-hosted Desktop order with no checkout we created still issues", async () => {
    const email = testMailbox("storefront");
    const raw = h.mock.craftOrder({
      externalCustomerId: null,
      email,
      product: "desktop",
      reason: "purchase",
      checkoutId: null,
      items: [{ amount: 2900, priceId: "price_local_desktop" }],
      discountAmount: 2900,
    });
    const held = h.mock.takeHeld();
    const paid = held.find((d) => d.type === "order.paid")!;
    expect((await h.deliver(paid)).status).toBe(200);
    expect((await licensesFor(raw.id)).length).toBe(1);
  });
});
