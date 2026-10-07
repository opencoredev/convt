import { expect, test } from "bun:test";

import {
  captureEvent,
  desktopTrialStartedEvent,
  emitAnalytics,
  licensePurchasedEvent,
  purchaseEventsFromLicenses,
} from "../../src/analytics";

test("license_purchased names Desktop or Pro and is idempotent by subject", () => {
  expect(licensePurchasedEvent("usr_1", "desktop", "ord_1")).toEqual({
    event: "license_purchased",
    distinctId: "usr_1",
    insertId: "license_purchased:desktop:ord_1",
    properties: { plan: "desktop" },
  });
  expect(licensePurchasedEvent("usr_1", "pro", "sub_1").insertId).toBe(
    "license_purchased:pro:sub_1",
  );
});

test("desktop_trial_started uses the Polar subscription id", () => {
  expect(desktopTrialStartedEvent("usr_2", "sub_polar")).toEqual({
    event: "desktop_trial_started",
    distinctId: "usr_2",
    insertId: "desktop_trial_started:sub_polar",
    properties: { plan: "pro" },
  });
});

test("claimed licenses become one purchase event per order or subscription", () => {
  expect(
    purchaseEventsFromLicenses("usr_1", [
      { plan: "desktop", orderId: "ord_1" },
      { plan: "desktop", orderId: "ord_1" },
      { plan: "pro", subscriptionId: "sub_1" },
      { plan: "desktop" },
    ]),
  ).toEqual([
    licensePurchasedEvent("usr_1", "desktop", "ord_1"),
    licensePurchasedEvent("usr_1", "pro", "sub_1"),
  ]);
});

test("captureEvent fails on a non-2xx PostHog response", async () => {
  const original = globalThis.fetch;
  globalThis.fetch = (async () => new Response("down", { status: 503 })) as unknown as typeof fetch;
  try {
    await expect(
      captureEvent(
        { key: "phc_test", host: "https://us.i.posthog.com" },
        { event: "license_purchased", distinctId: "usr_1" },
      ),
    ).rejects.toThrow(/503/);
  } finally {
    globalThis.fetch = original;
  }
});

test("emitAnalytics is a no-op without a sink and swallows capture errors", async () => {
  await emitAnalytics(undefined, [{ event: "x", distinctId: "u" }]);
  const seen: string[] = [];
  await emitAnalytics(
    async (e) => {
      seen.push(e.event);
      throw new Error("nope");
    },
    [
      { event: "a", distinctId: "u" },
      { event: "b", distinctId: "u" },
    ],
  );
  expect(seen).toEqual(["a", "b"]);
});
