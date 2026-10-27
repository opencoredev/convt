import { expect, test } from "bun:test";

import {
  syncPostHog,
  trackWith,
  type PostHogClient,
  type PostHogState,
} from "../../src/components/posthog-provider";

const config = { key: "phc_test", host: "https://us.i.posthog.com" };
const fresh: PostHogState = { loaded: false, identified: null };

/** A fake posthog-js; `saved` is the account an earlier visit left in its storage. */
function recorder(saved: string | null = null) {
  const calls: string[] = [];
  const client: PostHogClient = {
    init: (key) => void calls.push(`init:${key}`),
    identify: (id) => void calls.push(`identify:${id}`),
    reset: () => void calls.push("reset"),
    get_property: (name) => (name === "$user_state" && saved ? "identified" : "anonymous"),
    get_distinct_id: () => saved ?? "anon_1",
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

test("a signed-out page forgets the account an earlier visit saved", () => {
  // Signed out in another tab, or the session expired: this page never saw the account.
  const { calls, client } = recorder("usr_1");
  const state = syncPostHog({ client, config, userId: null, choice: "on", state: fresh });
  expect(calls).toEqual(["init:phc_test", "reset"]);
  expect(state).toEqual({ loaded: true, identified: null });
});

test("a saved account that is still signed in is not identified again", () => {
  const { calls, client } = recorder("usr_1");
  syncPostHog({ client, config, userId: "usr_1", choice: "on", state: fresh });
  expect(calls).toEqual(["init:phc_test"]);
});

test("another account on the same browser starts a fresh visitor", () => {
  const { calls, client } = recorder("usr_1");
  syncPostHog({ client, config, userId: "usr_2", choice: "on", state: fresh });
  expect(calls).toEqual(["init:phc_test", "reset", "identify:usr_2"]);
});

test("a named event is sent only once PostHog loaded and while analytics are on", () => {
  const captured: string[] = [];
  const client = { capture: (event: string) => void captured.push(event) };
  const event = "download_clicked" as const;
  const properties = { os: "macos" };
  trackWith({ client, loaded: false, choice: "on", event, properties });
  trackWith({ client, loaded: true, choice: "off", event, properties });
  trackWith({ client, loaded: true, choice: "browser", event, properties });
  expect(captured).toEqual([]);
  trackWith({ client, loaded: true, choice: "on", event, properties });
  expect(captured).toEqual(["download_clicked"]);
});
