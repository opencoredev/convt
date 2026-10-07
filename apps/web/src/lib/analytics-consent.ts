// Whether this browser has opted out of PostHog. The privacy policy names these
// three ways out: Do Not Track, Global Privacy Control, and the switch on the
// privacy page, which stores its choice in localStorage.

const OPT_OUT_KEY = "convt:analytics-opt-out";

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
}
