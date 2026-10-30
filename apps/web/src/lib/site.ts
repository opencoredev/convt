// Public-site facts and outbound links in one place. Values marked PLACEHOLDER
// are stand-ins that Leo replaces before launch; the report lists each one.

/** Canonical origin for sitemap, canonical links and Open Graph URLs. */
export const SITE_ORIGIN = "https://convt.app";
export const SITE_NAME = "convt";

export const GITHUB_URL = "https://github.com/opencoredev/convt";
/** The Raycast extension by Dhruv Suthar (0xdhrv); source in integrations/raycast. */
export const RAYCAST_URL = "https://www.raycast.com/0xdhrv/convt";

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

/**
 * Facts the privacy policy and terms state about who sells convt. convt is the trading
 * name; Polar sells every purchase as merchant of record. No postal address is published.
 */
export const legal = {
  /** The name the pages give the seller and data controller. */
  entity: "convt",
  /** Days after a Desktop or Pro payment in which a refund needs no reason. */
  refundDays: 14,
  /** Completes "These terms are governed by …". */
  governingLaw: "the laws of the State of Florida, USA",
  /** Completes "Any dispute about them goes to …". */
  courts: "the state and federal courts located in Florida",
  /** The day this text takes effect; move it to the merge date if that slips. */
  effectiveDate: "October 7, 2026",
} as const;

/** Internal routes the landing page and the site footer link to. */
export const routes = {
  download: "/download",
  formats: "/formats",
  guides: "/guides",
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

/** Checkout for the $39 Desktop License; works signed out. */
export const BUY_DESKTOP_URL = "/checkout/desktop";
/** Checkout for Desktop + Cloud ($69 once); signs in first, since the cloud year is the account's. */
export const BUY_BUNDLE_URL = "/checkout/bundle";

/** A calendar day, `YYYY-MM-DD`, as the sitemap's `lastmod` expects. */
type IsoDate = `${number}-${number}-${number}`;

/**
 * Pages listed in the sitemap, in order; the sitemap route adds /guides and each guide after
 * these, dated from `guides`. /download asks for an account, so it isn't one, and the docs
 * (/docs, /docs/api) have their own sitemap on the convt-docs Worker.
 *
 * `lastmod` is the day the page's content last changed, taken from git (the route file, the
 * content or components it renders, and the shared site header and footer), never the build
 * time. Move it when you change what the page says, not for refactors that leave the page
 * the same.
 */
export const sitemapPages = [
  { path: "/", lastmod: "2026-10-30" },
  { path: routes.formats, lastmod: "2026-10-30" },
  { path: routes.changelog, lastmod: "2026-10-26" },
  { path: routes.contact, lastmod: "2026-10-30" },
  { path: routes.brand, lastmod: "2026-10-23" },
  { path: routes.privacy, lastmod: "2026-10-27" },
  { path: routes.terms, lastmod: "2026-10-30" },
] as const satisfies readonly { path: string; lastmod: IsoDate }[];

/** The docs Worker's own sitemap, referenced from robots.txt. */
export const DOCS_SITEMAP_URL = `${SITE_ORIGIN}/docs/sitemap.xml`;

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
  type = "website",
}: {
  title: string;
  description: string;
  path: string;
  type?: "website" | "article";
}) {
  const url = `${SITE_ORIGIN}${path}`;
  return {
    meta: [
      { title },
      { name: "description", content: description },
      { property: "og:title", content: title },
      { property: "og:description", content: description },
      { property: "og:type", content: type },
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
