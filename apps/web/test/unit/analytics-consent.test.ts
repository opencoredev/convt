import { expect, test } from "bun:test";

import { browserOptedOut } from "../../src/lib/analytics-consent";
import { readEnv } from "../../src/server/env";

test("Do Not Track and Global Privacy Control opt the browser out", () => {
  expect(browserOptedOut(undefined)).toBe(false);
  expect(browserOptedOut({ doNotTrack: null })).toBe(false);
  expect(browserOptedOut({ doNotTrack: "0" })).toBe(false);
  expect(browserOptedOut({ doNotTrack: "1" })).toBe(true);
  expect(browserOptedOut({ doNotTrack: "yes" })).toBe(true);
  expect(browserOptedOut({ globalPrivacyControl: true })).toBe(true);
});

test("an empty POSTHOG_KEY, as staging sets it, turns PostHog off", () => {
  const base = {
    ENV: "staging",
    BETTER_AUTH_URL: "https://x.test",
    BETTER_AUTH_SECRET: "x".repeat(64),
  };
  expect(readEnv({ ...base, POSTHOG_KEY: "" }).posthog).toBeNull();
  expect(readEnv(base).posthog).toBeNull();
  expect(readEnv({ ...base, POSTHOG_KEY: "phc_test" }).posthog).toEqual({
    key: "phc_test",
    host: "https://us.i.posthog.com",
  });
});
