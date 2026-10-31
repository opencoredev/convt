import { guidePath, guides } from "#/lib/guides";
import { routes, sitemapPages } from "#/lib/site";

export type SitemapEntry = { path: string; lastmod: string };

/** The later of two `YYYY-MM-DD` days; the format sorts as text. */
const later = (a: string, b: string): string => (a > b ? a : b);

/** The newest guide's `updated` day: the guides index changes whenever a guide does. */
export const guidesLastmod: string = guides.map((guide): string => guide.updated).reduce(later);

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
