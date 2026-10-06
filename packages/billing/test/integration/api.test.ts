// API enrollment: the spend cap's handoff from the checkout row to the
// subscription, pending until a card is seen, Pro and API together, and cap
// changes serialized with P9's reservations.

import { afterAll, beforeAll, describe, expect, test } from "bun:test";
import { sql } from "drizzle-orm";

import { reconcileFrequent } from "../../src/reconcile";
import { createHarness, type Harness } from "../../src/testing";

let h: Harness;
beforeAll(async () => {
  h = await createHarness();
});
afterAll(async () => h?.close());

async function apiSub(userId: string) {
  const [s] = await h.q<{
    id: string;
    status: string;
    spend_cap_cents: number;
    card_seen_at: string | null;
    provider_subscription_id: string;
  }>(sql`
    select id, status, spend_cap_cents, card_seen_at, provider_subscription_id from subscriptions where user_id = ${userId} and kind = 'api'`);
  return s;
}

describe("spend cap", () => {
  test("the cap reaches the subscription when the webhook is on time", async () => {
    const u = await h.user("cap1@convt.test");
    await h.buy("api", u, { cap: 2500 });
    await h.deliverAll();
    const s = await apiSub(u.id);
    expect(s).toMatchObject({ status: "active", spend_cap_cents: 2500 });
    expect(s.card_seen_at).not.toBeNull();
  });

  test("delayed: the success page's sync carries the cap", async () => {
    const u = await h.user("cap2@convt.test");
    const b = await h.buy("api", u, { cap: 1500 });
    const held = h.mock.takeHeld(); // delayed
    const r = await h.service.checkoutResult({
      providerCheckoutId: b.providerCheckoutId,
      cookie: null,
      sessionUserId: u.id,
      sync: true,
    });
    expect(r.result.state).toBe("api_enrolled");
    expect((await apiSub(u.id)).spend_cap_cents).toBe(1500);
    for (const d of held) await h.deliver(d); // the late webhooks change nothing
    expect((await apiSub(u.id)).spend_cap_cents).toBe(1500);
  });

  test("dropped: the reconciler carries the cap", async () => {
    const u = await h.user("cap3@convt.test");
    const b = await h.buy("api", u, { cap: 3000 });
    h.mock.takeHeld(); // dropped
    // Checkouts still open after 10 minutes are fetched again.
    h.mock.advance(11 * 60_000);
    await h.service.withCtx((c) => reconcileFrequent(c, ["refetch"]));
    expect((await apiSub(u.id)).spend_cap_cents).toBe(3000);
    // The orders scan reaches the same state on its own.
    const v = await h.user("cap3b@convt.test");
    await h.buy("api", v, { cap: 3100 });
    h.mock.takeHeld();
    await h.service.withCtx((c) => reconcileFrequent(c, ["orders"]));
    expect((await apiSub(v.id)).spend_cap_cents).toBe(3100);
    void b;
  });

  test("a restart between the checkout row and the provider call: the row expires unused", async () => {
    const u = await h.user("cap4@convt.test");
    h.failAt("after-checkout-row");
    await expect(
      h.service.createCheckout({ product: "api", user: u, spendCapCents: 2000 }),
    ).rejects.toThrow("injected fault");
    h.clearFaults();
    const [row] = await h.q<{ id: string; provider_checkout_id: string | null; status: string }>(
      sql`select id, provider_checkout_id, status from checkouts where user_id = ${u.id}`,
    );
    expect(row.provider_checkout_id).toBeNull();
    h.mock.advance(3 * 3600_000);
    await h.service.withCtx((c) => reconcileFrequent(c, ["refetch"]));
    const [after] = await h.q<{ status: string }>(
      sql`select status from checkouts where id = ${row.id}`,
    );
    expect(after.status).toBe("expired");
  });

  test("a restart after the provider call but before storing its id: matched by metadata, cap applied", async () => {
    const u = await h.user("cap5@convt.test");
    h.failAt("before-checkout-store");
    await expect(
      h.service.createCheckout({ product: "api", user: u, spendCapCents: 4200 }),
    ).rejects.toThrow("injected fault");
    h.clearFaults();
    const co = h.mock.lastCheckout()!;
    h.mock.completeCheckout(co.id, "4242", u.email);
    await h.deliverAll();
    expect((await apiSub(u.id)).spend_cap_cents).toBe(4200);
    const [row] = await h.q<{ provider_checkout_id: string }>(
      sql`select provider_checkout_id from checkouts where user_id = ${u.id}`,
    );
    expect(row.provider_checkout_id).toBe(co.id);
  });

  test("enrollment is pending until a card is seen", async () => {
    const u = await h.user("card@convt.test");
    const b = await h.buy("api", u, { cap: 2000 });
    // The provider reports no card yet (as if the card were still being saved).
    const c = h.mock.customerByExternalId(u.id)!;
    const saved = c.cards;
    c.cards = [];
    await h.deliverAll();
    expect((await apiSub(u.id)).card_seen_at).toBeNull();
    expect(
      (
        await h.service.checkoutResult({
          providerCheckoutId: b.providerCheckoutId,
          cookie: null,
          sessionUserId: u.id,
          sync: false,
        })
      ).result.state,
    ).toBe("pending");
    c.cards = saved;
    await h.service.withCtx((ctx) => reconcileFrequent(ctx, ["refetch"]));
    expect((await apiSub(u.id)).card_seen_at).not.toBeNull();
  });

  test("the cap is validated: whole cents from $1 to $10,000", async () => {
    const u = await h.user("badcap@convt.test");
    for (const cap of [0, 99, 1_000_001, 12.5]) {
      expect(
        await h.service.createCheckout({ product: "api", user: u, spendCapCents: cap }),
      ).toEqual({ ok: false, refusal: "bad_cap" });
    }
  });
});

describe("Pro and API together", () => {
  test("with multiple subscriptions on, one user holds both and gets no duplicate of either", async () => {
    const u = await h.user("both@convt.test");
    await h.buy("pro_month", u);
    await h.buy("api", u, { cap: 2000 });
    await h.deliverAll();
    const subs = await h.q<{ kind: string }>(
      sql`select kind from subscriptions where user_id = ${u.id} order by kind`,
    );
    expect(subs.map((s) => s.kind)).toEqual(["api", "pro"]);
    expect(
      await h.service.createCheckout({ product: "api", user: u, spendCapCents: 2000 }),
    ).toEqual({ ok: false, refusal: "already_enrolled" });
    expect(await h.service.createCheckout({ product: "pro_year", user: u })).toEqual({
      ok: false,
      refusal: "already_pro",
    });
  });

  test("with it off, API enrollment is refused with the notice", async () => {
    const u = await h.user("off@convt.test");
    h.mock.settings.allowMultipleSubscriptions = false;
    try {
      expect(
        await h.service.createCheckout({ product: "api", user: u, spendCapCents: 2000 }),
      ).toEqual({ ok: false, refusal: "needs_multiple_subscriptions" });
    } finally {
      h.mock.settings.allowMultipleSubscriptions = true;
    }
  });

  test("a second live subscription that does arrive is stored and alerted", async () => {
    const u = await h.user("dup@convt.test");
    await h.buy("pro_month", u);
    await h.deliverAll();
    // A second Pro checkout made at the provider with our metadata (say, two tabs).
    const created = await h.provider.createCheckout({
      product: "pro_year",
      checkoutRef: "chk_00000000000000000000000000",
      successUrl: "http://x",
      allowTrial: false,
      externalCustomerId: u.id,
      email: u.email,
    });
    await h.owner
      .execute(sql`insert into checkouts (id, provider, provider_checkout_id, user_id, product, nonce_hash, nonce_expires_at, status)
      values ('chk_00000000000000000000000000', 'polar', ${created.providerCheckoutId}, ${u.id}, 'pro_year', '\\x00', now(), 'open')`);
    h.mock.completeCheckout(created.providerCheckoutId, "4242", u.email);
    await h.deliverAll();
    const subs = await h.q(
      sql`select 1 from subscriptions where user_id = ${u.id} and kind = 'pro'`,
    );
    expect(subs.length).toBe(2);
    expect(
      (
        await h.q(
          sql`select 1 from billing_alerts where kind = 'duplicate_subscription' and subject = ${`pro:${u.id}`}`,
        )
      ).length,
    ).toBe(1);
  });
});

describe("cap changes and P9 reservations", () => {
  /**
   * A stand-in for P9's reservation: in one transaction, lock the API subscription
   * row, sum open reservations, and refuse one that would exceed the cap.
   */
  async function reserve(subId: string, cents: number, holdMs = 0) {
    const { client } = await h.tdb.open("owner");
    try {
      await client.query("begin");
      const s = await client.query(
        "select spend_cap_cents, user_id from subscriptions where id = $1 for update",
        [subId],
      );
      const open = await client.query(
        "select coalesce(sum(reserved_cents), 0)::int as n from cloud_jobs where user_id = $1 and reservation = 'open'",
        [s.rows[0].user_id],
      );
      if (open.rows[0].n + cents > s.rows[0].spend_cap_cents) {
        await client.query("rollback");
        return false;
      }
      await client.query(
        `insert into cloud_jobs (id, user_id, source, status, input_format, target_format, reserved_cents, reservation, expires_at)
         values ($1, $2, 'api', 'created', 'docx', 'pdf', $3, 'open', now() + interval '1 day')`,
        [`job_${crypto.randomUUID().replace(/-/g, "").slice(0, 26)}`, s.rows[0].user_id, cents],
      );
      if (holdMs) await Bun.sleep(holdMs);
      await client.query("commit");
      return true;
    } catch (e) {
      await client.query("rollback").catch(() => {});
      throw e;
    }
  }

  test("lowering the cap while a reservation runs keeps it; the next over-cap reservation is refused", async () => {
    const u = await h.user("reserve@convt.test");
    await h.buy("api", u, { cap: 1000 });
    await h.deliverAll();
    const s = await apiSub(u.id);
    expect(await reserve(s.id, 600)).toBe(true);
    // A reservation holds the row lock while the cap is lowered to $5.
    const started = Date.now();
    const holding = reserve(s.id, 300, 400);
    await Bun.sleep(50);
    const lowered = await h.service.setSpendCap(u.id, 500);
    expect(lowered).toEqual({ ok: true });
    expect(Date.now() - started).toBeGreaterThanOrEqual(350); // it waited for the lock
    expect(await holding).toBe(true);
    // Both earlier reservations stand (900 > 500); a new one is refused.
    const open = await h.q<{ n: number }>(
      sql`select count(*)::int as n from cloud_jobs where user_id = ${u.id} and reservation = 'open'`,
    );
    expect(open[0].n).toBe(2);
    expect(await reserve(s.id, 1)).toBe(false);
    expect((await apiSub(u.id)).spend_cap_cents).toBe(500);
  });
});
