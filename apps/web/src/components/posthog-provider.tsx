import { useEffect } from "react";
import { useRouterState } from "@tanstack/react-router";
import posthog, { type PostHogConfig as PostHogConfigOptions, type Properties } from "posthog-js";

import { analyticsChoice, type AnalyticsChoice } from "#/lib/analytics-consent";
import { sanitizeEvent } from "#/lib/analytics-sanitize";
import { syncIdentifiedUser, type IdentifyClient } from "#/lib/identify-user";

type PostHogConfig = { key: string; host: string } | null | undefined;

/**
 * The privacy policy promises these: no session recordings or heatmaps, nothing sent
 * after an opt-out, no query string or fragment in any URL PostHog receives, and
 * clicks recorded without element text or attributes (names, emails, avatar URLs).
 * Feature flags are off because their request carries the raw first-visit URL
 * outside `before_send`; the site uses none.
 */
export function posthogOptions(host: string): Partial<PostHogConfigOptions> {
  return {
    api_host: host,
    person_profiles: "identified_only",
    capture_pageview: false,
    capture_pageleave: true,
    advanced_disable_flags: true,
    disable_session_recording: true,
    capture_heatmaps: false,
    mask_all_text: true,
    mask_all_element_attributes: true,
    before_send: (event) => (analyticsChoice() === "on" ? sanitizeEvent(event) : null),
  };
}

/** The part of posthog-js that syncPostHog drives. */
export type PostHogClient = IdentifyClient & {
  init: (key: string, options: Partial<PostHogConfigOptions>) => unknown;
  get_property: (name: string) => unknown;
  get_distinct_id: () => string;
};

/** Whether this page loaded PostHog, and the account it is identified as. */
export type PostHogState = { loaded: boolean; identified: string | null };

/**
 * Loads PostHog once, then links or unlinks the signed-in account. Both happen in one
 * step because posthog-js drops identify() made before init(), and a child
 * component's effect would run before the provider's. The account PostHog saved
 * from an earlier visit counts as identified, so a signed-out page resets it even
 * when the sign-out happened elsewhere (another tab, an expired session).
 */
export function syncPostHog(input: {
  client: PostHogClient;
  config: { key: string; host: string };
  userId: string | null;
  choice: AnalyticsChoice;
  state: PostHogState;
}): PostHogState {
  const { client, config, userId, choice, state } = input;
  if (state.loaded)
    return {
      loaded: true,
      identified: syncIdentifiedUser(client, userId, choice, state.identified),
    };
  if (choice !== "on") return state;
  client.init(config.key, posthogOptions(config.host));
  const saved =
    client.get_property("$user_state") === "identified" ? client.get_distinct_id() : null;
  return { loaded: true, identified: syncIdentifiedUser(client, userId, choice, saved) };
}

let state: PostHogState = { loaded: false, identified: null };

/** Named events the site sends from the browser. Server-side events live in server/analytics. */
export type BrowserEvent = "download_clicked";

/** Sends `event` only when PostHog loaded on this page and analytics are still on. */
export function trackWith(input: {
  client: { capture: (event: string, properties: Properties) => unknown };
  loaded: boolean;
  choice: AnalyticsChoice;
  event: BrowserEvent;
  properties: Properties;
}) {
  if (!input.loaded || input.choice !== "on") return;
  input.client.capture(input.event, input.properties);
}

export function track(event: BrowserEvent, properties: Properties) {
  trackWith({
    client: posthog,
    loaded: state.loaded,
    choice: analyticsChoice(),
    event,
    properties,
  });
}

/**
 * Client-only PostHog init, identify, and SPA $pageview on TanStack Router navigations.
 * Renders children untouched and sends nothing when `config` has no key (staging, local).
 */
export function PostHogProvider({
  config,
  userId,
  children,
}: {
  config: PostHogConfig;
  /** The signed-in account, linked to this browser's visitor id. Never an email. */
  userId: string | null;
  children?: React.ReactNode;
}) {
  const pathname = useRouterState({ select: (s) => s.location.pathname });
  const search = useRouterState({ select: (s) => s.location.searchStr });

  // Declared before the pageview effect so the first pageview carries the account.
  useEffect(() => {
    if (!config?.key) return;
    state = syncPostHog({ client: posthog, config, userId, choice: analyticsChoice(), state });
  }, [config?.key, config?.host, userId]);

  useEffect(() => {
    if (!config?.key || !state.loaded) return;
    // PostHog reads the URL itself; sanitizeEvent strips its query string.
    posthog.capture("$pageview");
  }, [config?.key, pathname, search]);

  return children ?? null;
}
