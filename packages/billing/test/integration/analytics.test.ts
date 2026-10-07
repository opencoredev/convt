// Polar ingest analytics: a Pro trial and a paid Desktop or Pro license each
// fire once, even when the webhook is delivered again.

import { afterAll, beforeAll, describe, expect, test } from "bun:test";
import { sql } from "drizzle-orm";

import { createHarness, type Harness } from "../../src/testing";

let h: Harness;
beforeAll(async () => {
  h = await createHarness();
});
afterAll(async () => h?.close());

const eventsNamed = (name: string, userId?: string) =>
  h.analytics.filter((e) => e.event === name && (!userId || e.distinctId === userId));

describe("Polar analytics", () => {
  test("a Desktop purchase fires license_purchased once", async () => {
    const u = await h.user("analytics-desktop@convt.test");
    const before = eventsNamed("license_purchased", u.id).length;
    await h.buy("desktop", u, { email: u.email });
    const held = h.mock.takeHeld();
    const order = held.find((d) => d.type === "order.paid")!;
    expect((await h.deliver(order)).status).toBe(200);
    expect((await h.deliver(order)).status).toBe(200);
    const events = eventsNamed("license_purchased", u.id).slice(before);
    expect(events.length).toBe(1);
    expect(events[0].properties).toEqual({ plan: "desktop" });
    expect(events[0].insertId?.startsWith("license_purchased:desktop:")).toBe(true);
    expect(JSON.stringify(events[0])).not.toContain(u.email);
  });

  test("a Pro trial then conversion fires desktop_trial_started and license_purchased once each", async () => {
    const u = await h.user("analytics-pro@convt.test");
    const b = await h.buy("pro_month", u);
    const subId = h.mock
      .state()
      .checkouts.find((c) => c.id === b.providerCheckoutId)!.subscription_id!;
    await h.deliverAll();
    const trials = eventsNamed("desktop_trial_started", u.id);
    expect(trials.length).toBe(1);
    expect(trials[0].insertId).toBe(`desktop_trial_started:${subId}`);
    expect(trials[0].properties).toEqual({ plan: "pro" });
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
    expect(purchases[0].properties).toEqual({ plan: "pro" });

    h.mock.cycle(subId);
    await h.deliverAll();
    expect(eventsNamed("desktop_trial_started", u.id).length).toBe(1);
    expect(eventsNamed("license_purchased", u.id).length).toBe(1);
  });

  test("a guest Desktop purchase has no user event until it is claimed", async () => {
    const before = h.analytics.filter((e) => e.event === "license_purchased").length;
    await h.buy("desktop", null, { email: "analytics-guest@convt.test" });
    await h.deliverAll();
    expect(h.analytics.filter((e) => e.event === "license_purchased").length).toBe(before);
  });
});
