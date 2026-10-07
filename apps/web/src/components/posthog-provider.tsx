import { useEffect } from "react";
import { useRouterState } from "@tanstack/react-router";
import posthog, { type PostHogConfig as PostHogConfigOptions } from "posthog-js";

import { analyticsChoice } from "#/lib/analytics-consent";
import { sanitizeEvent } from "#/lib/analytics-sanitize";

type PostHogConfig = { key: string; host: string } | null | undefined;

let initialized = false;

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

function initPostHog(config: { key: string; host: string }) {
  if (initialized || typeof window === "undefined" || analyticsChoice() !== "on") return;
  posthog.init(config.key, posthogOptions(config.host));
  initialized = true;
}

/**
 * Client-only PostHog init + SPA $pageview on TanStack Router navigations.
 * Renders children untouched and sends nothing when `config` has no key (staging, local).
 */
export function PostHogProvider({
  config,
  children,
}: {
  config: PostHogConfig;
  children?: React.ReactNode;
}) {
  const pathname = useRouterState({ select: (s) => s.location.pathname });
  const search = useRouterState({ select: (s) => s.location.searchStr });

  useEffect(() => {
    if (!config?.key) return;
    initPostHog(config);
  }, [config?.key, config?.host]);

  useEffect(() => {
    if (!config?.key || !initialized) return;
    // PostHog reads the URL itself; sanitizeEvent strips its query string.
    posthog.capture("$pageview");
  }, [config?.key, pathname, search]);

  return children ?? null;
}
