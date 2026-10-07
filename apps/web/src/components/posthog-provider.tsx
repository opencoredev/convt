import { useEffect } from "react";
import { useRouterState } from "@tanstack/react-router";
import posthog from "posthog-js";

type PostHogConfig = { key: string; host: string } | null | undefined;

let initialized = false;

function initPostHog(config: { key: string; host: string }) {
  if (initialized || typeof window === "undefined") return;
  posthog.init(config.key, {
    api_host: config.host,
    person_profiles: "identified_only",
    capture_pageview: false,
    capture_pageleave: true,
    capture_exceptions: true,
  });
  initialized = true;
}

/** Client-only PostHog init + SPA $pageview on TanStack Router navigations. */
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
    posthog.capture("$pageview", {
      $current_url: window.location.href,
      $pathname: pathname,
    });
  }, [config?.key, pathname, search]);

  return children ?? null;
}
