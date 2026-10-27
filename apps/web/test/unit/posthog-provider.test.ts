import { expect, test } from "bun:test";

import {
  syncPostHog,
  type PostHogClient,
  type PostHogState,
} from "../../src/components/posthog-provider";

const config = { key: "phc_test", host: "https://us.i.posthog.com" };
const fresh: PostHogState = { loaded: false, identified: null };

function recorder() {
  const calls: string[] = [];
  const client: PostHogClient = {
    init: (key) => void calls.push(`init:${key}`),
    identify: (id) => void calls.push(`identify:${id}`),
    reset: () => void calls.push("reset"),
  };
  return { calls, client };
}

test("a signed-in first load initializes PostHog before identifying the account", () => {
  // posthog-js ignores identify() before init(); this order is what makes it count.
  const { calls, client } = recorder();
  const state = syncPostHog({ client, config, userId: "usr_1", choice: "on", state: fresh });
  expect(calls).toEqual(["init:phc_test", "identify:usr_1"]);
  expect(state).toEqual({ loaded: true, identified: "usr_1" });
});

test("PostHog initializes once, and the same account is identified once", () => {
  const { calls, client } = recorder();
  let state = syncPostHog({ client, config, userId: null, choice: "on", state: fresh });
  state = syncPostHog({ client, config, userId: "usr_1", choice: "on", state });
  state = syncPostHog({ client, config, userId: "usr_1", choice: "on", state });
  state = syncPostHog({ client, config, userId: null, choice: "on", state });
  expect(calls).toEqual(["init:phc_test", "identify:usr_1", "reset"]);
  expect(state).toEqual({ loaded: true, identified: null });
});

test("an opted-out browser never loads PostHog or identifies anyone", () => {
  for (const choice of ["off", "browser"] as const) {
    const { calls, client } = recorder();
    const state = syncPostHog({ client, config, userId: "usr_1", choice, state: fresh });
    expect(calls).toEqual([]);
    expect(state).toEqual(fresh);
  }
});
