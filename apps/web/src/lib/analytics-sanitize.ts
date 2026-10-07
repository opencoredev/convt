// PostHog fills URL properties from window.location and document.referrer, and
// autocapture records link hrefs. Query strings and fragments can carry checkout ids,
// the desktop sign-in state and challenge, or tokens, so every event keeps only
// origin + pathname. The privacy policy (routes/_site/privacy.tsx) relies on this.

import type { CaptureResult, Properties } from "posthog-js";

/** Keys whose values are URLs or paths even when they are not absolute URLs. */
const URL_KEY = /url|referrer|href|pathname/i;
/** href attributes inside autocapture's serialized $elements_chain. */
const CHAIN_HREF = /(href=")([^"?#]*)[?#][^"]*"/g;

/** Drops the query string and fragment from an absolute URL or a path. */
export function stripQuery(value: string): string {
  if (/^https?:\/\//i.test(value)) {
    try {
      const url = new URL(value);
      return url.origin + url.pathname;
    } catch {
      // Not parseable: fall through to the plain cut below.
    }
  }
  const cut = value.search(/[?#]/);
  return cut === -1 ? value : value.slice(0, cut);
}

function sanitizeValue(key: string, value: unknown): unknown {
  if (typeof value === "string") {
    if (key === "$elements_chain") return value.replace(CHAIN_HREF, '$1$2"');
    return URL_KEY.test(key) || /^https?:\/\//i.test(value) ? stripQuery(value) : value;
  }
  if (Array.isArray(value)) return value.map((item) => sanitizeValue(key, item));
  if (value !== null && typeof value === "object") {
    return sanitizeProperties(value as Properties);
  }
  return value;
}

export function sanitizeProperties(properties: Properties): Properties {
  const out: Properties = {};
  for (const [key, value] of Object.entries(properties)) out[key] = sanitizeValue(key, value);
  return out;
}

/** PostHog `before_send` hook: strips query strings from every URL an event carries. */
export function sanitizeEvent(event: CaptureResult | null): CaptureResult | null {
  if (!event) return event;
  return {
    ...event,
    properties: sanitizeProperties(event.properties),
    ...(event.$set ? { $set: sanitizeProperties(event.$set) } : {}),
    ...(event.$set_once ? { $set_once: sanitizeProperties(event.$set_once) } : {}),
  };
}
