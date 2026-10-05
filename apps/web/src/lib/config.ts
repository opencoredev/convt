// Outbound links and product facts the account pages show. The URLs marked
// PLACEHOLDER do not exist yet; swap them when the real services are live.

export const links = {
  /** PLACEHOLDER: docs site is not built. */
  docs: "https://docs.convt.app",
  /** PLACEHOLDER: docs site is not built. */
  apiReference: "https://docs.convt.app/api",
  /** PLACEHOLDER: docs site is not built. */
  formats: "https://docs.convt.app/formats",
  /** PLACEHOLDER: webhooks are not designed in the API yet. */
  webhooks: "https://docs.convt.app/webhooks",
  /** PLACEHOLDER: no help page yet. */
  help: "https://docs.convt.app/help",
  /** PLACEHOLDER: no download host yet (plan P11). */
  downloadMac: "https://convt.app/download/mac",
  terms: "/terms",
  privacy: "/privacy",
} as const;

/** PLACEHOLDER: the API host is not deployed (plan P9). */
export const apiBaseUrl = "https://api.convt.app";

/** PLACEHOLDER: the magic-link lifetime is decided when auth lands (plan P6). */
export const magicLinkMinutes = 15;

/** Length of the code in the sign-in email. */
export const signInCodeLength = 6;
