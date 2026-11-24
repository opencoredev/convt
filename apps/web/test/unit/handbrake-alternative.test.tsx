import { expect, test } from "bun:test";
import { renderToStaticMarkup } from "react-dom/server";

import { footerColumns } from "#/components/site/links";
import { handbrakeAlternative, handbrakeAlternativeHead } from "#/lib/landing-pages";
import { llmsTxt } from "#/lib/llms";
import { sitemapEntries } from "#/lib/sitemap";
import { SITE_ORIGIN } from "#/lib/site";
import { Route } from "../../src/routes/_site/handbrake-alternative";
import { Route as MkvRoute } from "../../src/routes/_site/guides/mkv-to-mp4-mac";
import { Route as MacRoute } from "../../src/routes/_site/mac-file-converter";

const url = `${SITE_ORIGIN}/handbrake-alternative`;

function render(route: typeof Route | typeof MkvRoute | typeof MacRoute): string {
  const Page = route.options.component;
  if (!Page) throw new Error("route has no component");
  return renderToStaticMarkup(<Page />);
}

test("the HandBrake page's head has its canonical URL and WebPage, app and breadcrumb JSON-LD", () => {
  const head = handbrakeAlternativeHead();
  expect(head.links).toContainEqual({ rel: "canonical", href: url });
  const script = head.scripts.find((s) => s.type === "application/ld+json");
  if (!script) throw new Error("no JSON-LD");
  const graph = (JSON.parse(script.children) as { "@graph": { "@type": string; url?: string }[] })[
    "@graph"
  ];
  const types = graph.map((node) => node["@type"]);
  for (const type of ["WebPage", "SoftwareApplication", "BreadcrumbList", "Organization"]) {
    expect(types).toContain(type);
  }
  expect(graph.find((node) => node["@type"] === "WebPage")?.url).toBe(url);
});

test("the HandBrake page tags downloads, gives every tool the same block and prints tested commands", () => {
  const html = render(Route);
  const downloads = [...html.matchAll(/href="(\/download[^"]*)"/g)].map((m) => m[1]);
  expect(downloads.length).toBeGreaterThan(0);
  for (const href of downloads) {
    expect(href).toBe(
      "/download?utm_source=convt_guides&amp;utm_medium=organic&amp;utm_campaign=handbrake-alternative",
    );
  }
  for (const tool of ["convt", "Shutter Encoder", "Permute", "VidCoder", "FFmpeg"]) {
    expect(html).toContain(`${tool} is best for`);
    expect(html).toContain(`Where ${tool} falls short`);
  }
  expect(html).toContain("Where HandBrake is better");
  expect(html).toContain("convt talk.mp4 --to mp3");
  expect(html).toContain("convt videos --to mp4 -r -o mp4");
  expect(html).toContain("talk.mp4 -&gt; ./talk.mp3");
  expect(html).toContain("3 converted, 0 failed");
});

test("the HandBrake page is in the sitemap, llms.txt, the footer, the MKV guide and the Mac page", () => {
  const entry = sitemapEntries.find((e) => e.path === handbrakeAlternative.path);
  expect(entry?.lastmod).toMatch(/^\d{4}-\d{2}-\d{2}$/);
  expect(llmsTxt).toContain(`(${url})`);
  const hrefs = footerColumns.flatMap((column) => column.links.map((link) => link.href));
  expect(hrefs).toContain(handbrakeAlternative.path);
  expect(render(MkvRoute)).toContain(`href="${handbrakeAlternative.path}"`);
  expect(render(MacRoute)).toContain(`href="${handbrakeAlternative.path}"`);
});
