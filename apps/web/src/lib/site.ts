// Public-site facts and outbound links in one place. Values marked PLACEHOLDER
// are stand-ins that Leo replaces before launch; the report lists each one.

/** Canonical origin for sitemap, canonical links and Open Graph URLs. */
export const SITE_ORIGIN = "https://convt.app";
export const SITE_NAME = "convt";

export const GITHUB_URL = "https://github.com/opencoredev/convt";

/**
 * True since launch (7 October 2026): the landing page links downloads, sign-in, the
 * Desktop and Pro checkouts and the footer pages. Setting it to false swaps those
 * links for placeholder labels and hides the footer links. SALES still keeps the API off.
 */
export const LAUNCHED = true;

/** The convt post on Product Hunt, linked from the landing page badge. */
export const PRODUCT_HUNT_URL =
  "https://www.producthunt.com/products/convt?embed=true&utm_source=badge-featured&utm_medium=badge&utm_campaign=badge-convt-2";

/**
 * The landing page is prerendered static HTML, so the browser decides when the Product
 * Hunt badge appears: from 3 am Eastern (EDT) on 7 October 2026.
 */
export const PRODUCT_HUNT_FROM = Date.parse("2026-10-07T03:00:00-04:00");

/** PLACEHOLDER: support inbox (plan P12). */
export const SUPPORT_EMAIL = "support@convt.app";
/** PLACEHOLDER: privacy and data-request inbox. */
export const PRIVACY_EMAIL = "privacy@convt.app";

/** Refund policies the terms can state; `legal.refunds` picks one. */
export type RefundPolicy = "14-days" | "30-days" | "case-by-case";

/**
 * Facts the privacy policy and terms state about who provides convt. Values marked
 * TODO(CNV-48) are Leo's decisions, listed in the CNV-48 pull request, which must not
 * merge until he sets them. Until then each holds wording that reads cleanly on the page
 * without naming an entity or a country: the brand name the footer already uses, no
 * address, and the law of wherever the provider is established.
 */
export const legal: {
  entity: string;
  address: string | null;
  governingLaw: string;
  refunds: RefundPolicy;
  effectiveDate: string;
} = {
  /** TODO(CNV-48): the company or sole trader who sells and runs convt. */
  entity: "convt",
  /** TODO(CNV-48): postal address shown in the privacy policy; null hides it. */
  address: null,
  /** TODO(CNV-48): completes "These terms are governed by …", including the courts. */
  governingLaw: "the law of the country where convt's provider is established",
  /** TODO(CNV-48): which refund policy the terms state (see RefundPolicy). */
  refunds: "14-days",
  /** TODO(CNV-48): the day this text takes effect, normally the day it ships. */
  effectiveDate: "7 October 2026",
};

/** Internal routes the landing page and the site footer link to. */
export const routes = {
  download: "/download",
  formats: "/formats",
  pricing: "/#pricing",
  changelog: "/changelog",
  apiDocs: "/docs/api",
  privacy: "/privacy",
  terms: "/terms",
  contact: "/contact",
  brand: "/brand",
  signIn: "/sign-in",
  dashboard: "/dashboard",
} as const;

/** Checkout for the $29 Desktop License; works signed out. */
export const BUY_DESKTOP_URL = "/checkout/desktop";
/** Checkout for Pro; signs in first. */
export const buyProUrl = (interval: "month" | "year") => `/checkout/pro?interval=${interval}`;

/** Pages listed in the sitemap, in order. */
export const sitemapPaths = [
  "/",
  routes.download,
  routes.formats,
  routes.changelog,
  routes.apiDocs,
  routes.contact,
  routes.brand,
  routes.privacy,
  routes.terms,
] as const;

/** Share card for every public page: the landing hero at 1200x630 (public/og.png). */
const OG_IMAGE = {
  url: `${SITE_ORIGIN}/og.png`,
  width: 1200,
  height: 630,
  alt: "convt: convert any file with a right-click.",
};

/** Head tags for a public page: title, description, canonical URL and Open Graph. */
export function seo({
  title,
  description,
  path,
}: {
  title: string;
  description: string;
  path: string;
}) {
  const url = `${SITE_ORIGIN}${path}`;
  return {
    meta: [
      { title },
      { name: "description", content: description },
      { property: "og:title", content: title },
      { property: "og:description", content: description },
      { property: "og:type", content: "website" },
      { property: "og:url", content: url },
      { property: "og:site_name", content: SITE_NAME },
      { property: "og:image", content: OG_IMAGE.url },
      { property: "og:image:width", content: String(OG_IMAGE.width) },
      { property: "og:image:height", content: String(OG_IMAGE.height) },
      { property: "og:image:alt", content: OG_IMAGE.alt },
      { name: "twitter:card", content: "summary_large_image" },
      { name: "twitter:title", content: title },
      { name: "twitter:description", content: description },
      { name: "twitter:image", content: OG_IMAGE.url },
    ],
    links: [{ rel: "canonical", href: url }],
  };
}
