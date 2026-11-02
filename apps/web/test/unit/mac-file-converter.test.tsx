import { expect, test } from "bun:test";
import { renderToStaticMarkup } from "react-dom/server";

import { footerColumns } from "#/components/site/links";
import { guidePath, guides } from "#/lib/guides";
import { macFileConverter, macFileConverterHead } from "#/lib/landing-pages";
import { llmsTxt } from "#/lib/llms";
import { sitemapEntries } from "#/lib/sitemap";
import { SITE_ORIGIN } from "#/lib/site";
import { Route as GuideRoute } from "../../src/routes/_site/guides/heic-to-jpg-mac";
import { Route } from "../../src/routes/_site/mac-file-converter";

const url = `${SITE_ORIGIN}/mac-file-converter`;

function render(route: typeof Route | typeof GuideRoute): string {
  const Page = route.options.component;
  if (!Page) throw new Error("route has no component");
  return renderToStaticMarkup(<Page />);
}

test("the Mac page's head has its canonical URL and WebPage, app and breadcrumb JSON-LD", () => {
  const head = macFileConverterHead();
  expect(head.links).toContainEqual({ rel: "canonical", href: url });
  const data: unknown = JSON.parse(head.scripts[0].children);
  expect(data).toMatchObject({
    "@graph": [
      { "@type": "WebPage", url },
      { "@type": "SoftwareApplication", operatingSystem: "macOS 13 or later (Apple silicon)" },
      { "@type": "BreadcrumbList" },
      { "@type": "Organization" },
    ],
  });
});

test("the Mac page tags every download link and links every guide", () => {
  const html = render(Route);
  const downloads = [...html.matchAll(/href="(\/download[^"]*)"/g)].map((m) => m[1]);
  expect(downloads.length).toBeGreaterThan(0);
  for (const href of downloads) {
    expect(href).toBe(
      "/download?utm_source=convt_guides&amp;utm_medium=organic&amp;utm_campaign=mac-file-converter",
    );
  }
  for (const guide of guides) expect(html).toContain(`href="${guidePath(guide.slug)}"`);
  expect(html).toContain("convt Photos --to webp -r -o webp");
});

test("the Mac page is in the sitemap, llms.txt, the footer and every guide", () => {
  const entry = sitemapEntries.find((e) => e.path === macFileConverter.path);
  expect(entry?.lastmod).toMatch(/^\d{4}-\d{2}-\d{2}$/);
  expect(llmsTxt).toContain(`(${url})`);
  const hrefs = footerColumns.flatMap((column) => column.links.map((link) => link.href));
  expect(hrefs).toContain(macFileConverter.path);
  expect(render(GuideRoute)).toContain(`href="${macFileConverter.path}"`);
});
