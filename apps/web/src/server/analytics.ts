// Server-side PostHog capture for account events. Signup is recorded here so an
// ad blocker cannot drop it. Events carry the user id and no email, name or other
// PII. `$insert_id` makes a live capture and the backfill the same event.

import { stripQuery } from "#/lib/analytics-sanitize";
import {
  parseAttributionCookie,
  signupMethodFromAuthPath,
  utmFromAttribution,
  type SignupAttribution,
} from "#/lib/analytics-attribution";

export type AnalyticsEvent = {
  event: string;
  distinctId: string;
  timestamp?: string;
  insertId?: string;
  properties?: Record<string, string | number | boolean>;
};

export type CaptureAnalytics = (event: AnalyticsEvent) => Promise<void>;

export type AuthHookContext = {
  path?: string;
  headers?: Headers;
  body?: unknown;
  request?: Request;
} | null;

const PII_KEY = /email|phone|image|avatar|^name$/i;

/** Drops keys and values that look like personal data. */
export function withoutPii(
  properties: Record<string, unknown>,
): Record<string, string | number | boolean> {
  const out: Record<string, string | number | boolean> = {};
  for (const [key, value] of Object.entries(properties)) {
    if (PII_KEY.test(key)) continue;
    if (typeof value === "string") {
      if (value.includes("@") || value.length > 200) continue;
      out[key] = value;
    } else if (typeof value === "number" || typeof value === "boolean") {
      out[key] = value;
    }
  }
  return out;
}

export function userSignedUpInsertId(userId: string): string {
  return `user_signed_up:${userId}`;
}

export function userSignedUpEvent(
  userId: string,
  properties: Record<string, string>,
  timestamp = new Date(),
): AnalyticsEvent {
  return {
    event: "user_signed_up",
    distinctId: userId,
    insertId: userSignedUpInsertId(userId),
    timestamp: timestamp.toISOString(),
    properties: withoutPii(properties),
  };
}

function pathOnly(value: string | undefined, siteOrigin?: string): string | undefined {
  if (!value) return undefined;
  try {
    const url = value.startsWith("http")
      ? new URL(value)
      : siteOrigin
        ? new URL(value, siteOrigin)
        : null;
    if (url) {
      if (siteOrigin && url.origin !== new URL(siteOrigin).origin) return undefined;
      return url.pathname || undefined;
    }
  } catch {
    // Fall through to the plain cut.
  }
  const cut = stripQuery(value);
  return cut.startsWith("/") && !cut.startsWith("//") ? cut : undefined;
}

export function signupProperties(input: {
  path?: string;
  cookie?: string | null;
  callbackURL?: string;
  referer?: string | null;
  siteOrigin?: string;
}): Record<string, string> {
  const fromCookie = parseAttributionCookie(input.cookie);
  const props: Record<string, string> = {
    signup_method: signupMethodFromAuthPath(input.path ?? ""),
  };
  const source =
    fromCookie?.source ??
    pathOnly(input.callbackURL, input.siteOrigin) ??
    pathOnly(input.referer ?? undefined, input.siteOrigin);
  if (source) props.source = source;
  Object.assign(props, utmFromAttribution(fromCookie));
  if (input.callbackURL?.startsWith("http") || input.callbackURL?.startsWith("/")) {
    try {
      const url = input.callbackURL.startsWith("http")
        ? new URL(input.callbackURL)
        : new URL(input.callbackURL, input.siteOrigin ?? "https://convt.app");
      const extra: SignupAttribution = {};
      for (const key of ["utm_source", "utm_medium", "utm_campaign", "utm_content", "utm_term"]) {
        const value = url.searchParams.get(key);
        if (value) extra[key as keyof SignupAttribution] = value;
      }
      Object.assign(props, utmFromAttribution(extra));
    } catch {
      // Ignore a malformed callback URL.
    }
  }
  return withoutPii(props) as Record<string, string>;
}

export function signupEventFromAuthHook(
  userId: string,
  ctx: AuthHookContext,
  siteOrigin: string,
): AnalyticsEvent {
  const headers = ctx?.headers ?? ctx?.request?.headers;
  const body = (ctx?.body ?? {}) as Record<string, unknown>;
  return userSignedUpEvent(
    userId,
    signupProperties({
      path: ctx?.path,
      cookie: headers?.get("cookie"),
      callbackURL: typeof body.callbackURL === "string" ? body.callbackURL : undefined,
      referer: headers?.get("referer") ?? headers?.get("referrer"),
      siteOrigin,
    }),
  );
}

/** Best-effort capture. Missing config (staging, tests) is a no-op. */
export async function captureEvent(
  config: { key: string; host: string } | null,
  event: AnalyticsEvent,
): Promise<void> {
  if (!config?.key) return;
  const properties: Record<string, unknown> = {
    $lib: "convt-web",
    ...withoutPii(event.properties ?? {}),
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
    // PostHog is best-effort; a failed capture must not fail sign-up.
  }
}
