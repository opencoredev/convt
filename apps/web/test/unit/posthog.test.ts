import { expect, test } from "bun:test";

import { POSTHOG_HOST, POSTHOG_PROJECT_TOKEN, resolvePosthogKey } from "../../src/lib/posthog";

test("VITE_PUBLIC_POSTHOG_KEY wins and an empty string disables capture", () => {
  expect(resolvePosthogKey("phc_from_env", true)).toBe("phc_from_env");
  expect(resolvePosthogKey("  phc_from_env  ", false)).toBe("phc_from_env");
  expect(resolvePosthogKey("", true)).toBeUndefined();
  expect(resolvePosthogKey("   ", false)).toBeUndefined();
});

test("production falls back to the public convt.app token when env is unset", () => {
  expect(resolvePosthogKey(undefined, true)).toBe(POSTHOG_PROJECT_TOKEN);
  expect(resolvePosthogKey(null, true)).toBe(POSTHOG_PROJECT_TOKEN);
  expect(resolvePosthogKey(undefined, false)).toBeUndefined();
  expect(POSTHOG_HOST).toBe("https://us.i.posthog.com");
  expect(POSTHOG_PROJECT_TOKEN.startsWith("phc_")).toBe(true);
});
