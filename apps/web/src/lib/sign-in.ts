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
