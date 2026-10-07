import { useEffect } from "react";
import { useRouterState } from "@tanstack/react-router";
import posthog from "posthog-js";

import { analyticsChoice } from "#/lib/analytics-consent";
import { sanitizeEvent } from "#/lib/analytics-sanitize";

type PostHogConfig = { key: string; host: string } | null | undefined;

let initialized = false;

function initPostHog(config: { key: string; host: string }) {
  if (initialized || typeof window === "undefined" || analyticsChoice() !== "on") return;
  posthog.init(config.key, {
    api_host: config.host,
    person_profiles: "identified_only",
    capture_pageview: false,
    capture_pageleave: true,
    // The privacy policy promises these: no session recordings, nothing sent after an
    // opt-out, and no query string or fragment in any captured URL.
    disable_session_recording: true,
    before_send: (event) => (analyticsChoice() === "on" ? sanitizeEvent(event) : null),
  });
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
