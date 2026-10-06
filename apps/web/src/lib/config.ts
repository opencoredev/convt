// Outbound links and product facts the account pages show. The URLs marked
// PLACEHOLDER do not exist yet; swap them when the real services are live.

export const links = {
  /** The API reference on this site (renders convt-server's OpenAPI spec). */
  docs: "/docs/api",
  apiReference: "/docs/api",
  formats: "/formats",
  /** PLACEHOLDER: webhooks are not designed in the API yet. */
  webhooks: "https://docs.convt.app/webhooks",
  help: "/contact",
  /** The download page, which offers the macOS build first to Mac visitors. */
  downloadMac: "/download?os=macos",
  /** The pricing section on the landing page. */
  pricing: "/#pricing",
  terms: "/terms",
  privacy: "/privacy",
} as const;

/** PLACEHOLDER: the API host is not deployed (plan P9). */
export const apiBaseUrl = "https://api.convt.app";

/** How long a sign-in code and its link work. Matches `codeMinutes` in src/server/auth.ts. */
export const magicLinkMinutes = 15;

/** Length of the code in the sign-in email. */
export const signInCodeLength = 6;
