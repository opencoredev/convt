// Outbound links and product facts the account pages show. The URLs marked
// PLACEHOLDER do not exist yet; swap them when the real services are live.

export const links = {
  /** The Blume docs site (apps/docs), served on convt.app/docs* by its own Worker. */
  docs: "/docs",
  /** Blume's reference, rendered from crates/convt-server/openapi.json. */
  apiReference: "/docs/api",
  docsQuickStart: "/docs/quick-start",
  docsErrors: "/docs/reference/errors",
  docsLimits: "/docs/reference/limits",
  docsFormats: "/docs/reference/formats",
  formats: "/formats",
  /** PLACEHOLDER: webhooks are not designed in the API yet. */
  webhooks: "https://docs.convt.app/webhooks",
  help: "/contact",
  /** The download page; it picks the visitor's OS, including Linux when that build exists. */
  download: "/download",
  buyDesktop: "/checkout/desktop",
  buyPro: "/checkout/pro?interval=month",
  /** The pricing section on the landing page. */
  pricing: "/#pricing",
  terms: "/terms",
  privacy: "/privacy",
} as const;

/** Interim Railway host until api.convt.app resolves (CNV-36); switch back then. */
export const apiBaseUrl = "https://convt-api-production.up.railway.app";

/** How long a sign-in code and its link work. Matches `codeMinutes` in src/server/auth.ts. */
export const magicLinkMinutes = 15;

/** Length of the code in the sign-in email. */
export const signInCodeLength = 6;
