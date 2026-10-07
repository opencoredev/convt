// First-touch signup attribution: the landing path and UTM fields, kept in a
// first-party cookie so the server-side `user_signed_up` event can include them.
// Nothing here is an email or other PII.

export const ATTRIBUTION_COOKIE = "convt_signup";

export type SignupMethod = "email" | "github" | "google" | "unknown";

export type SignupAttribution = {
  source?: string;
  utm_source?: string;
  utm_medium?: string;
  utm_campaign?: string;
  utm_content?: string;
  utm_term?: string;
};

const UTM_KEYS = ["utm_source", "utm_medium", "utm_campaign", "utm_content", "utm_term"] as const;

export function signupMethodFromAuthPath(path: string): SignupMethod {
  const p = path.toLowerCase();
  if (p.includes("github")) return "github";
  if (p.includes("google")) return "google";
  if (p.includes("email-otp") || p.includes("email")) return "email";
  return "unknown";
}

/**
 * Better Auth's OAuth callback is `/callback/:id`; `path` is that pattern and
 * the provider is `params.id`. Fall back to the request URL when the hook
 * context omits params.
 */
export function signupMethodFromAuthContext(input: {
  path?: string;
  providerId?: string;
  requestUrl?: string;
}): SignupMethod {
  let pathname = "";
  if (input.requestUrl) {
    try {
      pathname = new URL(input.requestUrl).pathname;
    } catch {
      pathname = input.requestUrl;
    }
  }
  for (const candidate of [input.providerId, input.path, pathname]) {
    const method = signupMethodFromAuthPath(candidate ?? "");
    if (method !== "unknown") return method;
  }
  return "unknown";
}

export function isSafeAttributionValue(value: string): boolean {
  return value.length > 0 && value.length <= 100 && !value.includes("@") && !/[\s<>]/.test(value);
}

function sanitizeSource(value: string): string | undefined {
  const path = value.startsWith("/") ? value.split(/[?#]/, 1)[0] : "";
  if (!path || path.startsWith("//") || !isSafeAttributionValue(path)) return undefined;
  return path;
}

export function attributionFromSearch(url: URL): SignupAttribution {
  const out: SignupAttribution = {};
  const source = sanitizeSource(url.pathname);
  if (source && source !== "/") out.source = source;
  for (const key of UTM_KEYS) {
    const value = url.searchParams.get(key);
    if (value && isSafeAttributionValue(value)) out[key] = value;
  }
  return out;
}

export function hasAttribution(value: SignupAttribution | null | undefined): boolean {
  if (!value) return false;
  return Boolean(value.source || UTM_KEYS.some((key) => value[key]));
}

export function utmFromAttribution(value: SignupAttribution | null | undefined): SignupAttribution {
  if (!value) return {};
  const out: SignupAttribution = {};
  for (const key of UTM_KEYS) {
    const item = value[key];
    if (item && isSafeAttributionValue(item)) out[key] = item;
  }
  return out;
}

export function sanitizeAttribution(raw: unknown): SignupAttribution | null {
  if (!raw || typeof raw !== "object") return null;
  const rec = raw as Record<string, unknown>;
  const out: SignupAttribution = {};
  if (typeof rec.source === "string") {
    const source = sanitizeSource(rec.source);
    if (source) out.source = source;
  }
  for (const key of UTM_KEYS) {
    const value = rec[key];
    if (typeof value === "string" && isSafeAttributionValue(value)) out[key] = value;
  }
  return hasAttribution(out) ? out : null;
}

export function parseAttributionCookie(
  cookieHeader: string | null | undefined,
): SignupAttribution | null {
  if (!cookieHeader) return null;
  const match = cookieHeader.match(new RegExp(`(?:^|;\\s*)${ATTRIBUTION_COOKIE}=([^;]+)`));
  if (!match) return null;
  try {
    return sanitizeAttribution(JSON.parse(decodeURIComponent(match[1])));
  } catch {
    return null;
  }
}

function writeAttributionCookie(value: SignupAttribution) {
  const encoded = encodeURIComponent(JSON.stringify(value));
  document.cookie = `${ATTRIBUTION_COOKIE}=${encoded}; Path=/; Max-Age=${60 * 60 * 24 * 90}; SameSite=Lax`;
}

/** Client only. Records the first landing path and UTM fields for the signup event. */
export function rememberAttribution() {
  if (typeof window === "undefined") return;
  if (parseAttributionCookie(document.cookie)) return;
  const next = attributionFromSearch(new URL(window.location.href));
  if (hasAttribution(next)) writeAttributionCookie(next);
}
