// Whether this browser has opted out of PostHog. The privacy policy names these
// three ways out: Do Not Track, Global Privacy Control, and the switch on the
// privacy page, which stores its choice in localStorage and a first-party cookie
// so the signup hook can see it.

const OPT_OUT_KEY = "convt:analytics-opt-out";
/** First-party cookie the privacy switch sets so server-side capture can opt out. */
export const ANALYTICS_CONSENT_COOKIE = "convt_analytics";

export type AnalyticsChoice = "on" | "off" | "browser";

type BrowserSignals = {
  doNotTrack?: string | null;
  globalPrivacyControl?: boolean;
};

/** True when the browser sends Do Not Track or Global Privacy Control. */
export function browserOptedOut(nav: BrowserSignals | undefined): boolean {
  if (!nav) return false;
  return nav.globalPrivacyControl === true || nav.doNotTrack === "1" || nav.doNotTrack === "yes";
}

function storedOptOut(): boolean {
  try {
    return window.localStorage.getItem(OPT_OUT_KEY) === "1";
  } catch {
    return false;
  }
}

/** Client only. */
export function analyticsChoice(): AnalyticsChoice {
  if (browserOptedOut(navigator as BrowserSignals)) return "browser";
  return storedOptOut() ? "off" : "on";
}

/** Client only. Takes effect for the next event; PostHog is not loaded on later visits. */
export function setAnalyticsOptOut(optOut: boolean) {
  try {
    if (optOut) window.localStorage.setItem(OPT_OUT_KEY, "1");
    else window.localStorage.removeItem(OPT_OUT_KEY);
  } catch {
    // Storage blocked: nothing persists, and PostHog cannot persist either.
  }
  try {
    document.cookie = optOut
      ? `${ANALYTICS_CONSENT_COOKIE}=off; Path=/; Max-Age=${60 * 60 * 24 * 365}; SameSite=Lax`
      : `${ANALYTICS_CONSENT_COOKIE}=; Path=/; Max-Age=0; SameSite=Lax`;
  } catch {
    // Document blocked (SSR, tests): headers still carry DNT and GPC.
  }
}

type HeaderMap = { get(name: string): string | null };

/** Server-side view of the same three opt-outs the browser already respects. */
export function analyticsAllowedFromHeaders(headers: HeaderMap | null | undefined): boolean {
  if (!headers) return true;
  const dnt = headers.get("dnt");
  if (dnt === "1" || dnt?.toLowerCase() === "yes") return false;
  if (headers.get("sec-gpc") === "1") return false;
  const cookie = headers.get("cookie") ?? "";
  return !new RegExp(`(?:^|;\\s*)${ANALYTICS_CONSENT_COOKIE}=off(?:;|$)`).test(cookie);
}
