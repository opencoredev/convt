// Every payload the mock emits parses with our schemas; ignored and malformed
// events are classified without throwing.

import { randomBytes } from "node:crypto";

import { describe, expect, test } from "bun:test";
import { createBillingMock, localProducts } from "@convt/billing-mock";

import { loadCatalog } from "../../src/catalog";
import { parsePolarEvent } from "../../src/polar";

describe("parsePolarEvent", () => {
  test("every mock delivery parses into facts", async () => {
    const secret = `whsec_${randomBytes(32).toString("base64")}`;
    const mock = createBillingMock({
      publicUrl: "http://127.0.0.1:1",
      accessToken: "t",
      resendApiKey: "r",
      webhook: { secret, mode: "hold" },
    });
    const call = (path: string, body: unknown, method = "POST") => {
      const init: RequestInit = {
        method,
        headers: { authorization: "Bearer t", "content-type": "application/json" },
      };
      if (body) init.body = JSON.stringify(body);
      return mock.fetch(new Request(`http://127.0.0.1:1${path}`, init));
    };
    for (const product of ["desktop", "pro_month", "pro_year", "api"] as const) {
      const co = await (
        await call("/v1/checkouts/", {
          products: [localProducts[product].productId],
          external_customer_id: `usr_${"a".repeat(26)}`,
          metadata: { convt_checkout: `chk_${"b".repeat(26)}` },
          allow_trial: product === "pro_month",
        })
      ).json();
      mock.completeCheckout(co.id);
      if (co.subscription_id === null) {
        const sub = mock.state().checkouts.find((c) => c.id === co.id)!.subscription_id;
        if (sub) {
          mock.cycle(sub);
          await call(`/v1/subscriptions/${sub}`, { cancel_at_period_end: true }, "PATCH");
        }
      }
    }
    const order = mock.state().orders[0];
    mock.refund(order.id, 100);
    const catalog = loadCatalog("local");
    const held = mock.takeHeld();
    const types = new Set<string>();
    for (const d of held) {
      const r = parsePolarEvent(catalog, d.body);
      expect({ type: d.type, ok: r.ok }).toEqual({ type: d.type, ok: true });
      if (r.ok) types.add(`${r.type}:${r.ignored}`);
    }
    expect(types.has("order.paid:false")).toBe(true);
    expect(types.has("subscription.cycled:false")).toBe(true);
    expect(types.has("refund.created:false")).toBe(true);
    const paid = parsePolarEvent(catalog, held.find((d) => d.type === "order.paid")!.body);
    if (!paid.ok) throw new Error("no");
    expect(paid.facts.orders[0].checkoutRef).toBe(`chk_${"b".repeat(26)}`);
    expect(paid.facts.orders[0].userId).toBe(`usr_${"a".repeat(26)}`);
    const hinted = held
      .filter((d) => d.type === "checkout.updated")
      .map((d) => parsePolarEvent(catalog, d.body))
      .find((r) => r.ok && r.facts.hints.some((x) => x.kind === "subscription"));
    expect(hinted).toBeDefined();
  });

  test("ignored, unknown and malformed events", () => {
    const catalog = loadCatalog("local");
    const env = (type: string, data: unknown) =>
      JSON.stringify({ type, timestamp: "2026-10-05T00:00:00Z", api_version: "2026-10", data });
    expect(parsePolarEvent(catalog, env("benefit.created", {}))).toMatchObject({
      ok: true,
      ignored: true,
    });
    expect(parsePolarEvent(catalog, env("something.new", {}))).toMatchObject({
      ok: true,
      ignored: true,
    });
    expect(parsePolarEvent(catalog, env("order.paid", { id: 1 }))).toMatchObject({ ok: false });
    expect(parsePolarEvent(catalog, "not json")).toMatchObject({
      ok: false,
      reason: "malformed: not JSON",
    });
  });
});
