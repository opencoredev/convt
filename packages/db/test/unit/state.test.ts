import { describe, expect, test } from "bun:test";

import {
  activeApiSubscription,
  cloudYears,
  currentProSubscription,
  deriveAccountState,
} from "../../src/queries/state";

const now = new Date("2026-10-04T12:00:00Z");
const sub = (kind: string, status: string, endedAt: Date | null = null, createdAt = now) => ({
  kind,
  status,
  endedAt,
  createdAt,
});
const desktop = { plan: "desktop", revokedAt: null };
const revoked = { plan: "desktop", revokedAt: new Date("2026-09-01T00:00:00Z") };
const past = new Date("2026-09-01T00:00:00Z");

describe("deriveAccountState", () => {
  const cases: Array<
    [string, ReturnType<typeof sub>[], { plan: string; revokedAt: Date | null }[], string]
  > = [
    ["nothing", [], [], "new"],
    ["revoked desktop only", [], [revoked], "new"],
    ["desktop", [], [desktop], "desktop"],
    ["pro active", [sub("pro", "active")], [], "pro"],
    ["pro past due", [sub("pro", "past_due")], [], "pro"],
    ["pro trialing", [sub("pro", "trialing")], [], "trial"],
    ["pro beats trial", [sub("pro", "trialing"), sub("pro", "active")], [], "pro"],
    ["trial beats desktop", [sub("pro", "trialing")], [desktop], "trial"],
    ["desktop beats lapsed", [sub("pro", "canceled", past)], [desktop], "desktop"],
    ["lapsed", [sub("pro", "canceled", past)], [], "pro_lapsed"],
    ["unpaid counts as ended", [sub("pro", "unpaid")], [], "pro_lapsed"],
    ["active with an end in the past is ended", [sub("pro", "active", past)], [], "pro_lapsed"],
    ["lapsed beats api", [sub("pro", "canceled", past), sub("api", "active")], [], "pro_lapsed"],
    ["api only", [sub("api", "active")], [], "api_only"],
    ["ended api is new", [sub("api", "canceled", past)], [], "new"],
    ["pro with api is pro", [sub("pro", "active"), sub("api", "active")], [], "pro"],
    ["incomplete pro is not pro", [sub("pro", "incomplete")], [], "new"],
    ["incomplete_expired counts as ended", [sub("pro", "incomplete_expired")], [], "pro_lapsed"],
    ["paused shows as lapsed", [sub("pro", "paused")], [], "pro_lapsed"],
    ["paused api is not enrolled", [sub("api", "paused")], [], "new"],
    ["incomplete api is not enrolled", [sub("api", "incomplete")], [], "new"],
    ["incomplete pro does not hide desktop", [sub("pro", "incomplete")], [desktop], "desktop"],
  ];
  for (const [name, subs, lics, expected] of cases) {
    test(name, () => expect(deriveAccountState(subs, lics, now) as string).toBe(expected));
  }
});

test("API enrollment is independent of the state", () => {
  expect(activeApiSubscription([sub("pro", "active"), sub("api", "active")], now)?.kind).toBe(
    "api",
  );
  expect(activeApiSubscription([sub("api", "canceled", past)], now)).toBeNull();
});

test("the billing page shows the live Pro subscription, else the newest ended one", () => {
  const old = sub("pro", "canceled", past, new Date("2026-01-01T00:00:00Z"));
  const newer = sub("pro", "canceled", past, new Date("2026-05-01T00:00:00Z"));
  const live = sub("pro", "active", null, new Date("2025-01-01T00:00:00Z"));
  expect(currentProSubscription([old, newer], now)).toBe(newer);
  expect(currentProSubscription([old, live, newer], now)).toBe(live);
  expect(currentProSubscription([sub("api", "active")], now)).toBeNull();
  // An unfinished checkout neither shows as a plan nor hides a real one.
  const incomplete = sub("pro", "incomplete", null, new Date("2026-09-30T00:00:00Z"));
  expect(currentProSubscription([incomplete], now)).toBeNull();
  expect(currentProSubscription([incomplete, live], now)).toBe(live);
  expect(activeApiSubscription([sub("api", "incomplete")], now)).toBeNull();
});

describe("Desktop + Cloud years", () => {
  const day = 86_400_000;
  const year = (status: string, start: number, end: number, endedAt = end) => ({
    kind: "pro",
    status,
    orderId: `ord_${start}`,
    currentPeriodStart: new Date(now.getTime() + start * day),
    currentPeriodEnd: new Date(now.getTime() + end * day),
    endedAt: new Date(now.getTime() + endedAt * day),
    createdAt: now,
  });

  test("a cloud year is not a Pro plan: the account is a Desktop account", () => {
    expect(deriveAccountState([year("active", -10, 355)], [desktop], now)).toBe("desktop");
    expect(deriveAccountState([year("active", -400, -35)], [desktop], now)).toBe("desktop");
    expect(currentProSubscription([year("active", -10, 355)], now)).toBeNull();
  });

  test("a subscription next to a cloud year is still the plan the billing page manages", () => {
    const pro = { ...sub("pro", "active"), orderId: null };
    expect(currentProSubscription([year("active", -10, 355), pro], now)).toBe(pro);
    expect(deriveAccountState([year("active", -10, 355), pro], [desktop], now)).toBe("pro");
  });

  test("stacked years run to the last end; ended and canceled years show when they stopped", () => {
    expect(cloudYears([], now)).toBeNull();
    expect(cloudYears([year("active", -10, 355), year("active", 355, 720)], now)).toEqual({
      state: "active",
      until: new Date(now.getTime() + 720 * day),
    });
    expect(cloudYears([year("active", -400, -35)], now)).toEqual({
      state: "ended",
      on: new Date(now.getTime() - 35 * day),
    });
    expect(cloudYears([year("canceled", -10, 355, -2)], now)).toEqual({
      state: "ended",
      on: new Date(now.getTime() - 2 * day),
    });
    // A recurring subscription is never a cloud year.
    expect(cloudYears([{ ...year("active", -10, 20), orderId: null }], now)).toBeNull();
  });
});
