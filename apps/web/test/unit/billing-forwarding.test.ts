import { expect, test } from "bun:test";

import { billingForwarding } from "../../src/server/billing-forwarding";

for (const env of ["production", "test"] as const) {
  test(`${env} refuses manual billing cron forwarding`, () => {
    expect(billingForwarding(env, "/__billing/scheduled")).toBe("deny");
  });
}
test("development forwards local cron and webhooks", () => {
  expect(billingForwarding("development", "/__billing/scheduled")).toBe("forward");
  expect(billingForwarding("development", "/webhooks/polar")).toBe("forward");
  expect(billingForwarding("production", "/webhooks/polar")).toBe("deny");
  expect(billingForwarding("development", "/dashboard")).toBeNull();
});

test("staging refuses the web manual cron endpoint", () => {
  expect(billingForwarding("staging", "/__billing/scheduled")).toBe("deny");
});
