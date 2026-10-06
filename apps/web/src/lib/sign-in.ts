// Search parameters the sign-in pages pass along, and the site origin for
// safeRedirect on either side of the network.

export type AuthSearch = { email?: string; redirect?: string };

export function authSearch(search: Record<string, unknown>): AuthSearch {
  const out: AuthSearch = {};
  if (typeof search.email === "string" && search.email !== "") out.email = search.email;
  if (typeof search.redirect === "string" && search.redirect !== "") out.redirect = search.redirect;
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
