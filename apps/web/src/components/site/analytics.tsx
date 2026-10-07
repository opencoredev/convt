import { useEffect } from "react";
import { useRouter } from "@tanstack/react-router";
import { PostHogProvider, usePostHog } from "@posthog/react";

import { POSTHOG_HOST, clientPosthogKey } from "#/lib/posthog";

const options = {
  api_host: POSTHOG_HOST,
  ui_host: "https://us.posthog.com",
  defaults: "2025-05-24" as const,
  // The provider inits after hydrate; TanStack Router owns in-app navigations.
  capture_pageview: false,
  capture_pageleave: true,
  autocapture: true,
  person_profiles: "identified_only" as const,
  disable_session_recording: true,
};

export function Analytics({ children }: { children: React.ReactNode }) {
  const apiKey = clientPosthogKey();
  if (!apiKey) return children;
  return (
    <PostHogProvider apiKey={apiKey} options={options}>
      <Pageviews />
      {children}
    </PostHogProvider>
  );
}

function Pageviews() {
  const posthog = usePostHog();
  const router = useRouter();

  useEffect(() => {
    const capture = () => {
      posthog.capture("$pageview", { $current_url: window.location.href });
    };
    capture();
    return router.subscribe("onResolved", capture);
  }, [posthog, router]);

  return null;
}
