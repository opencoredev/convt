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
  const out: SignupAttribution = { ...utmFromUrl(url) };
  const source = sanitizeSource(url.pathname);
  if (source && source !== "/") out.source = source;
  return out;
}

/**
 * UTM fields from the URL, else from the same-site path in its `redirect` or `next`
 * parameter: a signed-out visit to a tagged /download link lands on
 * /sign-in?redirect=/download?utm_..., so the tags arrive one level down.
 */
function utmFromUrl(url: URL): SignupAttribution {
  const own = utmFromParams(url.searchParams);
  if (hasAttribution(own)) return own;
  for (const key of ["redirect", "next"]) {
    const target = url.searchParams.get(key);
    if (!target?.startsWith("/") || target.startsWith("//")) continue;
    const nested = utmFromParams(new URL(target, url.origin).searchParams);
    if (hasAttribution(nested)) return nested;
  }
  return {};
}

function utmFromParams(params: URLSearchParams): SignupAttribution {
  const out: SignupAttribution = {};
  for (const key of UTM_KEYS) {
    const value = params.get(key);
    if (value && isSafeAttributionValue(value)) out[key] = value;
  }
  return out;
}

/**
 * The cookie value after a visit to `url`, or null to leave the cookie alone. The
 * landing path is first touch. Campaign tags are the first ones seen, so a visitor who
 * lands on a guide and then follows its tagged download link keeps both.
 */
export function nextAttribution(
  saved: SignupAttribution | null,
  url: URL,
): SignupAttribution | null {
  if (!saved) {
    const first = attributionFromSearch(url);
    return hasAttribution(first) ? first : null;
  }
  if (hasAttribution(utmFromAttribution(saved))) return null;
  const utm = utmFromUrl(url);
  return hasAttribution(utm) ? { ...saved, ...utm } : null;
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

/** Client only. Records the first landing path and campaign tags for the signup event. */
export function rememberAttribution() {
  if (typeof window === "undefined") return;
  const next = nextAttribution(
    parseAttributionCookie(document.cookie),
    new URL(window.location.href),
  );
  if (next) writeAttributionCookie(next);
}
