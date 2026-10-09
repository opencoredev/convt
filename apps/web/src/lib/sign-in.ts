// Search parameters the sign-in pages pass along, the site origin for safeRedirect on
// either side of the network, and where a finished sign-in lands.

import { safeRedirect } from "./safe-redirect";
import { routes } from "./site";

export type AuthSearch = { email?: string; redirect?: string };

/**
 * `redirect` is the page to return to. `next` is the same thing under the name
 * /download and other public links use; `redirect` wins when both are present.
 */
export function authSearch(search: Record<string, unknown>): AuthSearch {
  const out: AuthSearch = {};
  if (typeof search.email === "string" && search.email !== "") out.email = search.email;
  for (const key of ["redirect", "next"] as const) {
    const value = search[key];
    if (out.redirect === undefined && typeof value === "string" && value !== "")
      out.redirect = value;
  }
  return out;
}

/**
 * The origin redirects are checked against. In the browser it is the page's own;
 * on the server any origin works, because safeRedirect returns only a path and the
 * origin check only has to reject other hosts.
 */
export function siteOrigin(): string {
  return typeof window === "undefined" ? "https://convt.app" : window.location.origin;
}

/** An account this young was created by the sign-in that just finished. */
const newAccountMs = 10 * 60 * 1000;

/** True when a sign-in response's `user.createdAt` is from the last few minutes. */
export function isNewAccount(createdAt: unknown, now: number = Date.now()): boolean {
  if (typeof createdAt !== "string" && !(createdAt instanceof Date)) return false;
  const at = new Date(createdAt).getTime();
  return Number.isFinite(at) && now - at >= -60_000 && now - at < newAccountMs;
}

/**
 * Where a finished sign-in goes: the page that sent the visitor here, else /download for
 * an account this sign-in created (the next step is installing the app), else the
 * dashboard.
 */
export function landingAfterSignIn({
  redirect,
  newAccount,
  origin,
}: {
  redirect: string | undefined;
  newAccount: boolean;
  origin: string;
}): string {
  if (redirect) return safeRedirect(redirect, origin);
  return newAccount ? routes.download : safeRedirect(undefined, origin);
}

/** The part of `localStorage` the emailed-link redirect uses. */
export type RedirectStore = Pick<Storage, "getItem" | "setItem" | "removeItem">;

const redirectKey = "convt.sign-in-redirect";
/** As long as the code in the email works (`codeMinutes` on the server). */
const redirectMs = 15 * 60 * 1000;

type StoredRedirect = { email: string; redirect: string; at: number };

const sameEmail = (a: string, b: string) => a.trim().toLowerCase() === b.trim().toLowerCase();

function parseStoredRedirect(raw: string | null): StoredRedirect | null {
  if (raw === null) return null;
  let value: unknown;
  try {
    value = JSON.parse(raw);
  } catch {
    return null;
  }
  if (typeof value !== "object" || value === null) return null;
  const { email, redirect, at } = value as Record<string, unknown>;
  return typeof email === "string" && typeof redirect === "string" && typeof at === "number"
    ? { email, redirect, at }
    : null;
}

/**
 * Remembers in this browser where a code sign-in should return to, for the link in the
 * email: it carries only the address and the code, and opens in a new tab. A sign-in
 * with nowhere to return to clears an older one. Storage that refuses is ignored.
 */
export function rememberSignInRedirect(
  store: RedirectStore,
  email: string,
  redirect: string | undefined,
  now: number = Date.now(),
): void {
  try {
    if (redirect) store.setItem(redirectKey, JSON.stringify({ email, redirect, at: now }));
    else store.removeItem(redirectKey);
  } catch {
    // Private browsing can refuse storage; the link then lands as a plain sign-in.
  }
}

/**
 * The page the emailed link's sign-in returns to: the one remembered for this address
 * while its code still works. Used once.
 */
export function takeSignInRedirect(
  store: RedirectStore,
  email: string,
  now: number = Date.now(),
): string | undefined {
  try {
    const stored = parseStoredRedirect(store.getItem(redirectKey));
    if (!stored || !sameEmail(stored.email, email)) return undefined;
    store.removeItem(redirectKey);
    return now - stored.at >= 0 && now - stored.at < redirectMs ? stored.redirect : undefined;
  } catch {
    return undefined;
  }
}

/**
 * Where a "Get convt" link points: /download for a signed-in visitor, otherwise straight
 * to sign-in with /download as the one `redirect`, so the click costs no extra redirects.
 */
export function downloadEntryHref(signedIn: boolean): string {
  return signedIn ? routes.download : `/sign-in?redirect=${encodeURIComponent(routes.download)}`;
}

/**
 * Where a visitor to /download goes instead, or null to show the page. The page asks for
 * an account; Homebrew and the GitHub releases stay open, and sign-in says so.
 */
export function downloadGate(
  session: { emailVerified: boolean } | null,
  href: string,
): string | null {
  const back = encodeURIComponent(href);
  if (!session) return `/sign-in?redirect=${back}`;
  if (!session.emailVerified) return `/sign-in/verify-email?redirect=${back}`;
  return null;
}
