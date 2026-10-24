// convt-billing's settings and its production guards. Production values are Worker
// vars and secrets; local development gets them from apps/billing/.dev.vars,
// which scripts/dev-web.sh writes. Pure, so the guards are unit-tested.

import { importSigningKey, parseSeed, publicKeyOf } from "@convt/license";

import { loadCatalog, type CatalogEnv } from "./catalog";

export type BillingEnv = {
  env: "production" | "staging" | "development" | "test";
  catalogEnv: CatalogEnv;
  desktopProduct: { productId: string; priceId: string };
  polar: { accessToken: string; apiUrl: string; webhookSecret: string; portalOrigin: string };
  mail:
    | { transport: "sequenzy"; apiKey: string; from: string }
    | { transport: "resend"; apiKey: string; apiUrl: string; from: string }
    | { transport: "log"; from: string };
  siteUrl: string;
  alertEmail: string | null;
  downloadUrl: string;
  signingSeed: string;
  /** Production: the public key release builds embed; the signing key must match it. */
  licensePublicKey: string | null;
  /** Public keys that must never sign in production (the local dev keys). */
  devPublicKeys: string[];
  /** Public PostHog project key + host. Null when unset (staging, local). */
  posthog: { key: string; host: string } | null;
  marketing: MarketingEnv;
};

/**
 * Campaign email. `sync` is null until Sequenzy's marketing key is set; consent
 * is still recorded and rows wait. `linkSecret` signs the preferences links in
 * campaign footers; `webhookSecret` checks Sequenzy's unsubscribe webhooks.
 */
export type MarketingEnv = {
  sync: {
    apiKey: string;
    apiUrl: string;
    /** Sequenzy list ids new contacts join; null keeps the workspace defaults. */
    lists: string[] | null;
    tags: string[];
  } | null;
  linkSecret: string | null;
  webhookSecret: string | null;
};

export type RawEnv = Record<string, unknown>;

const str = (raw: RawEnv, key: string) => {
  const v = raw[key];
  return typeof v === "string" && v.trim() !== "" ? v.trim() : undefined;
};

export function isLoopback(url: string): boolean {
  try {
    const host = new URL(url).hostname;
    return host === "127.0.0.1" || host === "localhost" || host === "[::1]";
  } catch {
    return false;
  }
}

export class ConfigError extends Error {
  constructor(message: string) {
    super(message);
    this.name = "ConfigError";
  }
}

const csv = (value: string | undefined) =>
  (value ?? "")
    .split(",")
    .map((s) => s.trim())
    .filter(Boolean);

function readMarketingEnv(raw: RawEnv, production: boolean): MarketingEnv {
  const apiKey = str(raw, "SEQUENZY_MARKETING_API_KEY");
  const linkSecret = str(raw, "MARKETING_LINK_SECRET") ?? null;
  const webhookSecret = str(raw, "SEQUENZY_WEBHOOK_SECRET") ?? null;
  const apiUrl = (str(raw, "SEQUENZY_API_URL") ?? "https://api.sequenzy.com/api/v1").replace(
    /\/$/,
    "",
  );
  if (linkSecret !== null && linkSecret.length < 32)
    throw new ConfigError("MARKETING_LINK_SECRET must be at least 32 characters");
  if (apiKey && !linkSecret)
    throw new ConfigError(
      "MARKETING_LINK_SECRET is required with SEQUENZY_MARKETING_API_KEY: every contact gets a preferences link",
    );
  if (production && (isLoopback(apiUrl) || !apiUrl.startsWith("https://")))
    throw new ConfigError("SEQUENZY_API_URL must be a public https URL in production");
  const lists = csv(str(raw, "SEQUENZY_MARKETING_LIST_IDS"));
  const tags = csv(str(raw, "SEQUENZY_MARKETING_TAGS") ?? "convt-account");
  return {
    sync: apiKey ? { apiKey, apiUrl, lists: lists.length ? lists : null, tags } : null,
    linkSecret,
    webhookSecret,
  };
}

export function readBillingEnv(raw: RawEnv): BillingEnv {
  const envName = str(raw, "ENV") ?? "production";
  if (
    envName !== "production" &&
    envName !== "staging" &&
    envName !== "development" &&
    envName !== "test"
  )
    throw new ConfigError(`ENV must be production, staging, development or test, not ${envName}`);
  const production = envName === "production" || envName === "staging";
  const need = (k: string) => {
    const v = str(raw, k);
    if (!v) throw new ConfigError(`${k} is not set`);
    return v;
  };
  const catalogEnv = (str(raw, "BILLING_CATALOG") ??
    (envName === "staging" ? "sandbox" : production ? "production" : "local")) as CatalogEnv;
  if (!["local", "sandbox", "production"].includes(catalogEnv))
    throw new ConfigError(`unknown BILLING_CATALOG ${catalogEnv}`);
  const desktopProductId = str(raw, "POLAR_DESKTOP_LIFETIME_PRODUCT_ID");
  const desktopPriceId = str(raw, "POLAR_DESKTOP_LIFETIME_PRICE_ID");
  if ((desktopProductId && !desktopPriceId) || (!desktopProductId && desktopPriceId))
    throw new ConfigError(
      "POLAR_DESKTOP_LIFETIME_PRODUCT_ID and POLAR_DESKTOP_LIFETIME_PRICE_ID must be set together",
    );
  if ((envName === "production" || envName === "staging") && (!desktopProductId || !desktopPriceId))
    throw new ConfigError(
      "POLAR_DESKTOP_LIFETIME_PRODUCT_ID and POLAR_DESKTOP_LIFETIME_PRICE_ID are required in production",
    );
  const apiUrl =
    str(raw, "POLAR_API_URL") ??
    (catalogEnv === "sandbox" ? "https://sandbox-api.polar.sh" : "https://api.polar.sh");
  const portalOrigin =
    str(raw, "POLAR_PORTAL_ORIGIN") ??
    (catalogEnv === "sandbox" ? "https://sandbox.polar.sh" : "https://polar.sh");
  const siteUrl = need("SITE_URL").replace(/\/$/, "");
  const from = str(raw, "MAIL_FROM") ?? "convt <hello@convt.app>";
  const transport = str(raw, "MAIL_TRANSPORT") ?? "resend";
  let mail: BillingEnv["mail"];
  if (transport === "resend") {
    mail = {
      transport,
      apiKey: need("RESEND_API_KEY"),
      apiUrl: str(raw, "RESEND_API_URL") ?? "https://api.resend.com",
      from,
    };
  } else if (transport === "sequenzy") {
    mail = { transport, apiKey: need("SEQUENZY_API_KEY"), from };
  } else if (transport === "log") {
    if (production) throw new ConfigError("MAIL_TRANSPORT=log is refused in production");
    mail = { transport, from };
  } else throw new ConfigError(`unknown MAIL_TRANSPORT ${transport}`);

  if (production) {
    if (catalogEnv !== (envName === "staging" ? "sandbox" : "production"))
      throw new ConfigError(
        `${envName} must use the ${envName === "staging" ? "sandbox" : "production"} catalog`,
      );
    // `bun run deploy` in apps/billing sets this from the checkout's dev key, so a
    // deploy that skipped it fails closed instead of skipping the dev-key check.
    const devKeys = (str(raw, "DEV_LICENSE_PUBKEYS") ?? "").split(",").map((k) => k.trim());
    if (!devKeys.some((k) => /^[A-Za-z0-9_-]{43}$/.test(k)))
      throw new ConfigError(
        "DEV_LICENSE_PUBKEYS must list the dev public key in production (use bun run deploy)",
      );
    if (!siteUrl.startsWith("https://"))
      throw new ConfigError("SITE_URL must be https in production");
    for (const [k, u] of [
      ["POLAR_API_URL", apiUrl],
      ["POLAR_PORTAL_ORIGIN", portalOrigin],
      ["RESEND_API_URL", mail.transport === "resend" ? mail.apiUrl : "https://api.resend.com"],
    ] as const) {
      if (isLoopback(u) || !u.startsWith("https://"))
        throw new ConfigError(`${k} must be a public https URL in production`);
    }
  } else if (str(raw, "POLAR_API_URL") && !isLoopback(apiUrl) && catalogEnv === "local") {
    throw new ConfigError("the local catalog only talks to a loopback billing mock");
  }
  const posthogKey = str(raw, "POSTHOG_KEY");
  const posthogHost = (str(raw, "POSTHOG_HOST") ?? "https://us.i.posthog.com").replace(/\/$/, "");
  const posthog = posthogKey ? { key: posthogKey, host: posthogHost } : null;
  return {
    env: envName,
    catalogEnv,
    desktopProduct: {
      productId: desktopProductId ?? loadCatalog(catalogEnv).products.desktop.productId,
      priceId: desktopPriceId ?? loadCatalog(catalogEnv).products.desktop.priceId,
    },
    polar: {
      accessToken: need("POLAR_ACCESS_TOKEN"),
      apiUrl,
      webhookSecret: need("POLAR_WEBHOOK_SECRET"),
      portalOrigin,
    },
    mail,
    siteUrl,
    alertEmail: str(raw, "ALERT_EMAIL") ?? null,
    downloadUrl: str(raw, "DOWNLOAD_URL") ?? `${siteUrl}/download`,
    signingSeed: need("LICENSE_SIGNING_KEY"),
    licensePublicKey: str(raw, "LICENSE_PUBLIC_KEY") ?? null,
    devPublicKeys: (str(raw, "DEV_LICENSE_PUBKEYS") ?? "")
      .split(",")
      .map((s) => s.trim())
      .filter(Boolean),
    posthog,
    marketing: readMarketingEnv(raw, production),
  };
}

/**
 * Imports the signing key and checks it: malformed is refused; in production its
 * public key must equal LICENSE_PUBLIC_KEY (what release builds embed) and must
 * not be a dev key.
 */
export async function loadSigningKey(e: BillingEnv): Promise<CryptoKey> {
  let key: CryptoKey;
  try {
    key = await importSigningKey(parseSeed(e.signingSeed));
  } catch {
    throw new ConfigError("LICENSE_SIGNING_KEY is malformed");
  }
  const pub = await publicKeyOf(key);
  if (e.env === "production" || e.env === "staging") {
    if (!e.licensePublicKey) throw new ConfigError("LICENSE_PUBLIC_KEY is required in production");
    if (pub !== e.licensePublicKey)
      throw new ConfigError("LICENSE_SIGNING_KEY does not match LICENSE_PUBLIC_KEY");
  }
  if ((e.env === "production" || e.env === "staging") && e.devPublicKeys.includes(pub))
    throw new ConfigError("a dev signing key is refused in production");
  return key;
}
