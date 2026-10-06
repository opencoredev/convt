// When an identity provider's email claim counts as convt's own verification.
// `users.email_verified` means convt has evidence the user controls the address:
// a convt email code, or an authoritative provider. GitHub never is: its `verified`
// flag means GitHub once confirmed the address, not that the user controls it now.

export type GoogleClaims = {
  email?: string | null;
  email_verified?: boolean | string | null;
  hd?: string | null;
};

/**
 * Google is authoritative for its own mailboxes (gmail.com, googlemail.com) and for
 * Workspace accounts whose `hd` claim names the address's domain. Google's
 * `email_verified` alone is not enough for other addresses.
 */
export function googleEmailIsAuthoritative(claims: GoogleClaims): boolean {
  const email = claims.email?.trim().toLowerCase();
  if (!email || (claims.email_verified !== true && claims.email_verified !== "true")) return false;
  const at = email.lastIndexOf("@");
  if (at < 1) return false;
  const domain = email.slice(at + 1);
  if (domain === "gmail.com" || domain === "googlemail.com") return true;
  return typeof claims.hd === "string" && claims.hd.trim().toLowerCase() === domain;
}

export function githubEmailIsAuthoritative(): boolean {
  return false;
}
