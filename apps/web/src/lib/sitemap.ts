import { guidePath, guides, type ListedGuide } from "#/lib/guides";
import { routes, sitemapPages } from "#/lib/site";

export type SitemapEntry = { path: string; lastmod: string };

type SitemapPageInput = {
  readonly path: string;
  readonly lastmod: string;
  readonly listsGuides?: "headings" | "cards";
};

type SitemapGuideInput = Pick<ListedGuide, "slug" | "published" | "updated" | "listingUpdated">;

/** The later of two `YYYY-MM-DD` days; the format sorts as text. */
const later = (a: string, b: string): string => (a > b ? a : b);

/** Newest `published` day across guides; moves pages that only render guide headings/links. */
function newestPublished(list: readonly SitemapGuideInput[]): string {
  return list.map((guide): string => guide.published).reduce(later);
}

/**
 * Newest `published` or `listingUpdated` day across guides; moves pages that render guide cards
 * (heading + description). Body-only edits (`updated`) only move that guide's own entry.
 */
function newestListing(list: readonly SitemapGuideInput[]): string {
  return list
    .map((guide): string =>
      guide.listingUpdated ? later(guide.published, guide.listingUpdated) : guide.published,
    )
    .reduce(later);
}

/** Newest guide `published` day; pages that list only headings/links (/formats) use this. */
export const guidesPublishedLastmod: string = newestPublished(guides);

/** Newest guide `published` or `listingUpdated` day; pages that list guide cards use this. */
export const guidesLastmod: string = newestListing(guides);

/**
 * Builds every URL in /sitemap.xml with its `lastmod`: the fixed pages (dated from the guides
 * they render), then /guides and each guide.
 */
export function buildSitemapEntries(
  pages: readonly SitemapPageInput[],
  list: readonly SitemapGuideInput[],
): SitemapEntry[] {
  const published = newestPublished(list);
  const listing = newestListing(list);
  return [
    ...pages.map((page): SitemapEntry => {
      if (page.listsGuides === "headings") {
        return { path: page.path, lastmod: later(page.lastmod, published) };
      }
      if (page.listsGuides === "cards") {
        return { path: page.path, lastmod: later(page.lastmod, listing) };
      }
      return { path: page.path, lastmod: page.lastmod };
    }),
    { path: routes.guides, lastmod: listing },
    ...list.map((guide) => ({ path: guidePath(guide.slug), lastmod: guide.updated })),
  ];
}

/** Every URL in /sitemap.xml with its `lastmod`. */
export const sitemapEntries: readonly SitemapEntry[] = buildSitemapEntries(sitemapPages, guides);
