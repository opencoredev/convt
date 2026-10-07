// Polar ingest analytics: a Pro trial and a paid Desktop or Pro license each
// fire once, even when the webhook is delivered again.

import { afterAll, beforeAll, describe, expect, test } from "bun:test";
import { sql } from "drizzle-orm";

import { purchaseEventsFromLicenses } from "../../src/analytics";
import { createHarness, testMailbox, type Harness } from "../../src/testing";

let h: Harness;
beforeAll(async () => {
  h = await createHarness();
});
afterAll(async () => h?.close());

const eventsNamed = (name: string, userId?: string) =>
  h.analytics.filter((e) => e.event === name && (!userId || e.distinctId === userId));

describe("Polar analytics", () => {
  test("a Desktop purchase fires license_purchased once", async () => {
    const u = await h.user(testMailbox("analytics-desktop"));
    const before = eventsNamed("license_purchased", u.id).length;
    await h.buy("desktop", u, { email: u.email });
    const held = h.mock.takeHeld();
    const order = held.find((d) => d.type === "order.paid")!;
    expect((await h.deliver(order)).status).toBe(200);
    expect((await h.deliver(order)).status).toBe(200);
    const events = eventsNamed("license_purchased", u.id).slice(before);
    expect(events.length).toBe(1);
    expect(events[0].distinctId).toBe(u.id);
    expect(events[0].properties).toEqual({ plan: "desktop" });
    expect(events[0].insertId?.startsWith("license_purchased:desktop:")).toBe(true);
    expect(JSON.stringify(events[0])).not.toContain(u.email);
    expect(events[0].properties).not.toHaveProperty("email");
  });

  test("a Pro trial then conversion fires desktop_trial_started and license_purchased once each", async () => {
    const u = await h.user(testMailbox("analytics-pro"));
    const b = await h.buy("pro_month", u);
    const subId = h.mock
      .state()
      .checkouts.find((c) => c.id === b.providerCheckoutId)!.subscription_id!;
    await h.deliverAll();
    const trials = eventsNamed("desktop_trial_started", u.id);
    expect(trials.length).toBe(1);
    expect(trials[0].distinctId).toBe(u.id);
    expect(trials[0].insertId).toBe(`desktop_trial_started:${subId}`);
    expect(trials[0].properties).toEqual({ plan: "pro" });
    expect(JSON.stringify(trials[0])).not.toContain(u.email);
    expect(eventsNamed("license_purchased", u.id)).toEqual([]);

    const [keys] = await h.q<{ n: number }>(sql`
      select count(*)::int as n from licenses l
      join subscriptions s on s.id = l.subscription_id
      where s.provider_subscription_id = ${subId}`);
    expect(keys.n).toBe(0);

    h.mock.cycle(subId);
    await h.deliverAll();
    const purchases = eventsNamed("license_purchased", u.id);
    expect(purchases.length).toBe(1);
    expect(purchases[0].distinctId).toBe(u.id);
    expect(purchases[0].properties).toEqual({ plan: "pro" });
    expect(JSON.stringify(purchases[0])).not.toContain(u.email);

    h.mock.cycle(subId);
    await h.deliverAll();
    expect(eventsNamed("desktop_trial_started", u.id).length).toBe(1);
    expect(eventsNamed("license_purchased", u.id).length).toBe(1);
  });

  test("a guest Desktop purchase has no user event until it is claimed", async () => {
    const mailbox = testMailbox("analytics-guest");
    const before = h.analytics.filter((e) => e.event === "license_purchased").length;
    await h.buy("desktop", null, { email: mailbox });
    await h.deliverAll();
    expect(h.analytics.filter((e) => e.event === "license_purchased").length).toBe(before);

    const u = await h.user(mailbox);
    await h.owner.execute(sql`select * from claim_purchases(${u.id})`);
    const [order] = await h.q<{ id: string }>(sql`select id from orders where user_id = ${u.id}`);
    const [license] = await h.q<{ order_id: string }>(
      sql`select order_id from licenses where user_id = ${u.id} and plan = 'desktop'`,
    );
    expect(order.id).toBe(license.order_id);
    const claimed = purchaseEventsFromLicenses(u.id, [
      { plan: "desktop", orderId: license.order_id },
    ]);
    expect(claimed).toEqual([
      {
        event: "license_purchased",
        distinctId: u.id,
        insertId: `license_purchased:desktop:${order.id}`,
        properties: { plan: "desktop" },
      },
    ]);
    expect(JSON.stringify(claimed[0])).not.toContain(u.email);
  });

  test("the webhook returns events without calling PostHog before it replies", async () => {
    const u = await h.user(testMailbox("analytics-nowait"));
    await h.buy("desktop", u, { email: u.email });
    const order = h.mock.takeHeld().find((d) => d.type === "order.paid")!;
    const before = h.analytics.length;
    const result = await h.service.handleWebhook(
      "POST",
      new TextEncoder().encode(order.body),
      new Headers(h.mock.sign(order)),
    );
    expect(result.status).toBe(200);
    expect(h.analytics.length).toBe(before);
    expect(
      result.analytics?.some((e) => e.event === "license_purchased" && e.distinctId === u.id),
    ).toBe(true);
  });
});
