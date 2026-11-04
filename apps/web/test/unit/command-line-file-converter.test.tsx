import { expect, test } from "bun:test";
import { renderToStaticMarkup } from "react-dom/server";

import { footerColumns } from "#/components/site/links";
import { guidePath, guides } from "#/lib/guides";
import { commandLineConverter, commandLineConverterHead } from "#/lib/landing-pages";
import { llmsTxt } from "#/lib/llms";
import { sitemapEntries } from "#/lib/sitemap";
import { SITE_ORIGIN } from "#/lib/site";
import { Route } from "../../src/routes/_site/command-line-file-converter";
import { Route as GuideRoute } from "../../src/routes/_site/guides/svg-to-png-mac";
import { Route as MacRoute } from "../../src/routes/_site/mac-file-converter";

const url = `${SITE_ORIGIN}/command-line-file-converter`;

function render(route: typeof Route | typeof GuideRoute | typeof MacRoute): string {
  const Page = route.options.component;
  if (!Page) throw new Error("route has no component");
  return renderToStaticMarkup(<Page />);
}

test("the command-line page's head has its canonical URL and WebPage, app and breadcrumb JSON-LD", () => {
  const head = commandLineConverterHead();
  expect(head.links).toContainEqual({ rel: "canonical", href: url });
  const data: unknown = JSON.parse(head.scripts[0].children);
  expect(data).toMatchObject({
    "@graph": [
      { "@type": "WebPage", url },
      {
        "@type": "SoftwareApplication",
        operatingSystem: "macOS (Apple silicon), Windows (x86_64), Linux (x86_64)",
      },
      { "@type": "BreadcrumbList" },
      { "@type": "Organization" },
    ],
  });
});

test("the command-line page tags every download link, links every guide and prints tested commands", () => {
  const html = render(Route);
  const downloads = [...html.matchAll(/href="(\/download[^"]*)"/g)].map((m) => m[1]);
  expect(downloads.length).toBeGreaterThan(0);
  for (const href of downloads) {
    expect(href).toBe(
      "/download?utm_source=convt_guides&amp;utm_medium=organic&amp;utm_campaign=command-line-file-converter",
    );
  }
  for (const guide of guides) expect(html).toContain(`href="${guidePath(guide.slug)}"`);
  expect(html).toContain("convt clip.mov --to mp4 --video-height 720");
  expect(html).toContain("convt Photos --to webp -r -o webp");
  expect(html).toContain("convt photo.png -p web.toml -o web");
  expect(html).toContain("convt --skill &gt; SKILL.md");
  expect(html).toContain('href="/mac-file-converter"');
});

test("the command-line page is in the sitemap, llms.txt, the footer, every guide and the Mac page", () => {
  const entry = sitemapEntries.find((e) => e.path === commandLineConverter.path);
  expect(entry?.lastmod).toMatch(/^\d{4}-\d{2}-\d{2}$/);
  expect(llmsTxt).toContain(`(${url})`);
  const hrefs = footerColumns.flatMap((column) => column.links.map((link) => link.href));
  expect(hrefs).toContain(commandLineConverter.path);
  expect(render(GuideRoute)).toContain(`href="${commandLineConverter.path}"`);
  expect(render(MacRoute)).toContain(`href="${commandLineConverter.path}"`);
});
