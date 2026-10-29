import { describe, expect, test } from "bun:test";

import { guidePath, guides } from "#/lib/guides";
import { llmsTxt } from "#/lib/llms";
import { SITE_ORIGIN, sitemapPages } from "#/lib/site";

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
