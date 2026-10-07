// PostHog events from Polar ingest. Fired after the transaction commits, with
// `$insert_id` so a webhook retry or the reconciler does not count twice.

export type AnalyticsEvent = {
  event: string;
  distinctId: string;
  timestamp?: string;
  insertId?: string;
  properties?: Record<string, string | number | boolean>;
};

export type CaptureAnalytics = (event: AnalyticsEvent) => Promise<void>;

export function licensePurchasedEvent(
  userId: string,
  plan: "desktop" | "pro",
  subjectId: string,
): AnalyticsEvent {
  return {
    event: "license_purchased",
    distinctId: userId,
    insertId: `license_purchased:${plan}:${subjectId}`,
    properties: { plan },
  };
}

export function desktopTrialStartedEvent(userId: string, subscriptionId: string): AnalyticsEvent {
  return {
    event: "desktop_trial_started",
    distinctId: userId,
    insertId: `desktop_trial_started:${subscriptionId}`,
    properties: { plan: "pro" },
  };
}

export async function emitAnalytics(
  capture: CaptureAnalytics | undefined,
  events: AnalyticsEvent[] | undefined,
): Promise<void> {
  if (!capture || !events?.length) return;
  for (const event of events) {
    try {
      await capture(event);
    } catch {
      // Best-effort: a failed capture must not fail ingest.
    }
  }
}

/** Best-effort capture. Missing config (staging, tests) is a no-op. */
export async function captureEvent(
  config: { key: string; host: string } | null,
  event: AnalyticsEvent,
): Promise<void> {
  if (!config?.key) return;
  const properties: Record<string, unknown> = {
    $lib: "convt-billing",
    ...event.properties,
  };
  if (event.insertId) properties.$insert_id = event.insertId;
  try {
    await fetch(`${config.host.replace(/\/$/, "")}/capture/`, {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify({
        api_key: config.key,
        event: event.event,
        distinct_id: event.distinctId,
        properties,
        timestamp: event.timestamp ?? new Date().toISOString(),
      }),
    });
  } catch {
    // Best-effort.
  }
}
