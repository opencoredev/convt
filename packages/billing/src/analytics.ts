// PostHog events from Polar ingest. Fired after the transaction commits, from the
// rows that changed, so a webhook retry or the reconciler sends nothing new.

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
  /** What was bought: `bundle` for a Desktop + Cloud order's Desktop key. */
  product: "desktop" | "bundle" | "pro" = plan,
): AnalyticsEvent {
  return {
    event: "license_purchased",
    distinctId: userId,
    insertId: `license_purchased:${plan}:${subjectId}`,
    properties: { plan, product },
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

export type LicensePurchaseRef = {
  plan: string;
  /** The order's product for a Desktop key: `desktop` or `bundle`. */
  product?: string | null;
  orderId?: string | null;
  subscriptionId?: string | null;
};

/** Idempotent purchase events for licenses attached by claim_purchases. */
export function purchaseEventsFromLicenses(
  userId: string,
  licenses: LicensePurchaseRef[],
): AnalyticsEvent[] {
  const events: AnalyticsEvent[] = [];
  const seen = new Set<string>();
  for (const license of licenses) {
    const event =
      license.plan === "desktop" && license.orderId
        ? licensePurchasedEvent(
            userId,
            "desktop",
            license.orderId,
            license.product === "bundle" ? "bundle" : "desktop",
          )
        : license.plan === "pro" && license.subscriptionId
          ? licensePurchasedEvent(userId, "pro", license.subscriptionId)
          : null;
    if (!event || seen.has(event.insertId!)) continue;
    seen.add(event.insertId!);
    events.push(event);
  }
  return events;
}

/**
 * Purchase events for the licenses a claim attached: those in `after` but not in
 * `before`. PostHog does not dedupe on `$insert_id`, so re-sending a license the user
 * already had would count that purchase twice.
 */
export function claimedPurchaseEvents(
  userId: string,
  before: LicensePurchaseRef[],
  after: LicensePurchaseRef[],
): AnalyticsEvent[] {
  const owned = new Set(purchaseEventsFromLicenses(userId, before).map((e) => e.insertId));
  return purchaseEventsFromLicenses(userId, after).filter((e) => !owned.has(e.insertId));
}

/** Same first-party cookie the privacy switch sets on convt.app. */
export const ANALYTICS_CONSENT_COOKIE = "convt_analytics";

type HeaderMap = { get(name: string): string | null };

/** DNT, GPC, or `convt_analytics=off` — the same three opt-outs the site honors. */
export function analyticsAllowedFromHeaders(headers: HeaderMap | null | undefined): boolean {
  if (!headers) return true;
  const dnt = headers.get("dnt");
  if (dnt === "1" || dnt?.toLowerCase() === "yes") return false;
  if (headers.get("sec-gpc") === "1") return false;
  const cookie = headers.get("cookie") ?? "";
  return !new RegExp(`(?:^|;\\s*)${ANALYTICS_CONSENT_COOKIE}=off(?:;|$)`).test(cookie);
}

/** Events the Worker may send after Polar replies. Empty when the request opted out. */
export function analyticsEventsToCapture(
  events: AnalyticsEvent[] | undefined,
  headers: HeaderMap | null | undefined,
): AnalyticsEvent[] {
  if (!events?.length || !analyticsAllowedFromHeaders(headers)) return [];
  return events;
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
  const res = await fetch(`${config.host.replace(/\/$/, "")}/capture/`, {
    method: "POST",
    headers: { "content-type": "application/json" },
    body: JSON.stringify({
      api_key: config.key,
      event: event.event,
      distinct_id: event.distinctId,
      properties,
      timestamp: event.timestamp ?? new Date().toISOString(),
    }),
    signal: AbortSignal.timeout(3000),
  });
  if (!res.ok) {
    console.warn(`[analytics] ${event.event} capture returned ${res.status}`);
    throw new Error(`posthog ${res.status}`);
  }
}
