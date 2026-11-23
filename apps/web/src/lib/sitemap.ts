import { guidePath, guides } from "#/lib/guides";
import { routes, sitemapPages } from "#/lib/site";

export type SitemapEntry = { path: string; lastmod: string };

/** The later of two `YYYY-MM-DD` days; the format sorts as text. */
const later = (a: string, b: string): string => (a > b ? a : b);

/**
 * The newest guide's `published` or `listingUpdated` day: a page that lists guides changes when
 * a guide is added or when its listing heading/description is edited. Edits inside a guide's body
 * only move that guide's own `updated` date.
 */
export const guidesLastmod: string = guides
  .map((guide): string =>
    "listingUpdated" in guide ? later(guide.published, guide.listingUpdated) : guide.published,
  )
  .reduce(later);

/**
 * Every URL in /sitemap.xml with its `lastmod`: the fixed pages (dated from the newest guide
 * when they list guides), then /guides and each guide.
 */
export const sitemapEntries: readonly SitemapEntry[] = [
  ...sitemapPages.map((page): SitemapEntry => ({
    path: page.path,
    lastmod: "listsGuides" in page ? later(page.lastmod, guidesLastmod) : page.lastmod,
  })),
  { path: routes.guides, lastmod: guidesLastmod },
  ...guides.map((guide) => ({ path: guidePath(guide.slug), lastmod: guide.updated })),
];
