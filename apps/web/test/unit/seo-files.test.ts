import { describe, expect, test } from "bun:test";

import { guidePath, guides } from "#/lib/guides";
import { llmsTxt } from "#/lib/llms";
import {
  buildSitemapEntries,
  guidesLastmod,
  guidesPublishedLastmod,
  sitemapEntries,
} from "#/lib/sitemap";
import { SITE_ORIGIN, routes, sitemapPages } from "#/lib/site";

describe("sitemap pages", () => {
  test("every lastmod is a real past calendar day", () => {
    for (const { path, lastmod } of sitemapPages) {
      expect(lastmod, path).toMatch(/^\d{4}-\d{2}-\d{2}$/);
      const day = new Date(`${lastmod}T00:00:00Z`);
      expect(day.toISOString().slice(0, 10), path).toBe(lastmod);
      expect(day.getTime(), path).toBeLessThanOrEqual(Date.now());
    }
  });

  test("paths are unique and leave the docs to their own sitemap", () => {
    const paths = sitemapPages.map((p) => p.path);
    expect(new Set(paths).size).toBe(paths.length);
    expect(paths.some((p) => p.startsWith("/docs"))).toBe(false);
  });

  test("pages that list guides are dated from the fields they render", () => {
    const expectedPublished =
      guides
        .map((g) => g.published)
        .sort()
        .at(-1) ?? "";
    const expectedListing =
      guides
        .map((g) =>
          "listingUpdated" in g && g.listingUpdated > g.published ? g.listingUpdated : g.published,
        )
        .sort()
        .at(-1) ?? "";
    expect(guidesPublishedLastmod).toBe(expectedPublished);
    expect(guidesLastmod).toBe(expectedListing);

    // /formats renders headings/links only, so a description-only edit does not advance it.
    const formats = sitemapEntries.find((e) => e.path === routes.formats);
    expect(formats?.lastmod).toBe(guidesPublishedLastmod);

    // Card listings render heading + description, so listing edits advance them.
    for (const path of [routes.guides, routes.macFileConverter, routes.commandLineConverter]) {
      const entry = sitemapEntries.find((e) => e.path === path);
      expect((entry?.lastmod ?? "") >= guidesLastmod, path).toBe(true);
    }
    const index = sitemapEntries.find((e) => e.path === routes.guides);
    expect(index?.lastmod).toBe(guidesLastmod);
  });

  test("body-only guide edits do not move listing pages, and description edits do not move /formats", () => {
    const entries = buildSitemapEntries(
      [
        { path: routes.formats, lastmod: "2026-10-30", listsGuides: "headings" },
        { path: routes.macFileConverter, lastmod: "2026-11-03", listsGuides: "cards" },
        { path: routes.commandLineConverter, lastmod: "2026-11-03", listsGuides: "cards" },
      ],
      [
        {
          slug: "heic-to-jpg-mac",
          published: "2026-11-10",
          updated: "2026-11-28",
        },
        {
          slug: "mov-to-mp4-mac",
          published: "2026-10-30",
          updated: "2026-11-25",
          listingUpdated: "2026-11-20",
        },
      ],
    );
    const byPath = new Map(entries.map((e) => [e.path, e.lastmod]));
    expect(byPath.get(routes.formats)).toBe("2026-11-10");
    expect(byPath.get(routes.guides)).toBe("2026-11-20");
    expect(byPath.get(routes.macFileConverter)).toBe("2026-11-20");
    expect(byPath.get(routes.commandLineConverter)).toBe("2026-11-20");
    expect(byPath.get(guidePath("heic-to-jpg-mac"))).toBe("2026-11-28");
    expect(byPath.get(guidePath("mov-to-mp4-mac"))).toBe("2026-11-25");
  });

  test("lists every fixed page, /guides and each guide once", () => {
    const paths = sitemapEntries.map((e) => e.path);
    expect(new Set(paths).size).toBe(paths.length);
    expect(paths).toEqual([
      ...sitemapPages.map((p) => p.path),
      routes.guides,
      ...guides.map((g) => guidePath(g.slug)),
    ]);
  });
});

describe("llms.txt", () => {
  test("starts with an H1 and a summary blockquote", () => {
    const [h1, blank, summary] = llmsTxt.split("\n");
    expect(h1).toBe("# convt");
    expect(blank).toBe("");
    expect(summary).toStartWith("> ");
  });

  test("links only to absolute convt.app or known outside URLs", () => {
    const links = [...llmsTxt.matchAll(/\]\(([^)]+)\)/g)].map((m) => m[1]);
    expect(links.length).toBeGreaterThan(5);
    for (const link of links) expect(link).toMatch(/^https:\/\//);
    expect(links).toContain(`${SITE_ORIGIN}/formats`);
  });

  test("lists the guides index and every guide", () => {
    expect(llmsTxt).toContain(`(${SITE_ORIGIN}/guides)`);
    for (const guide of guides)
      expect(llmsTxt).toContain(`(${SITE_ORIGIN}${guidePath(guide.slug)})`);
  });

  test("never claims a free tier or an Explorer menu in the installer", () => {
    expect(llmsTxt).toContain("There is no free tier.");
    expect(llmsTxt).toContain("Explorer menu is not in the installer yet");
    expect(llmsTxt).not.toMatch(/\bfree\b(?! tier)/i);
  });
});
