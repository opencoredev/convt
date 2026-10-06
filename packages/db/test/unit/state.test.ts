import { describe, expect, test } from "bun:test";

import {
  activeApiSubscription,
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
