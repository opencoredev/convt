import { useEffect, useRef } from "react";
import posthog from "posthog-js";

import { analyticsChoice } from "#/lib/analytics-consent";
import { syncIdentifiedUser } from "#/lib/identify-user";

/** Identifies the signed-in user with PostHog when analytics are on. Renders nothing. */
export function IdentifyUser({ userId }: { userId: string | null }) {
  const previous = useRef<string | null>(null);
  useEffect(() => {
    previous.current = syncIdentifiedUser(posthog, userId, analyticsChoice(), previous.current);
  }, [userId]);
  return null;
}
