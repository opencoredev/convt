import { expect, test } from "bun:test";

import { proTrialEntry } from "../../src/server/pro-trial";

test("the app's trial URL only ever starts a trial", () => {
  expect(proTrialEntry({ kind: "can_start_trial", checkoutUrl: "/checkout/pro?from=app" })).toBe(
    "checkout",
  );
  // A subscriber, a running trial or a Desktop + Cloud year (also "pro") has one.
  expect(proTrialEntry({ kind: "pro" })).toBe("/dashboard/billing");
  expect(
    proTrialEntry({ kind: "trial", endsOn: "2026-11-06", endsAt: "2026-11-06T12:00:00Z" }),
  ).toBe("/dashboard/billing");
  // Used its trial and has nothing live: buy once instead.
  expect(proTrialEntry({ kind: "lapsed" })).toBe("/#pricing");
});
