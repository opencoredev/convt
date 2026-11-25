import { expect, test } from "bun:test";
import { renderToStaticMarkup } from "react-dom/server";

import { footerColumns } from "#/components/site/links";
import { cloudconvertAlternative, cloudconvertAlternativeHead } from "#/lib/landing-pages";
import { llmsTxt } from "#/lib/llms";
import { sitemapEntries } from "#/lib/sitemap";
import { SITE_ORIGIN } from "#/lib/site";
import { Route } from "../../src/routes/_site/cloudconvert-alternative";
import { Route as HeicRoute } from "../../src/routes/_site/guides/batch-heic-to-jpg-windows";
import { Route as HandbrakeRoute } from "../../src/routes/_site/handbrake-alternative";

const url = `${SITE_ORIGIN}/cloudconvert-alternative`;

function render(route: typeof Route | typeof HeicRoute | typeof HandbrakeRoute): string {
  const Page = route.options.component;
  if (!Page) throw new Error("route has no component");
  return renderToStaticMarkup(<Page />);
}

test("the CloudConvert page's head has its canonical URL and WebPage, tool list and breadcrumb JSON-LD", () => {
  const head = cloudconvertAlternativeHead();
  expect(head.links).toContainEqual({ rel: "canonical", href: url });
  const script = head.scripts.find((s) => s.type === "application/ld+json");
  if (!script) throw new Error("no JSON-LD");
  const graph = (
    JSON.parse(script.children) as {
      "@graph": { "@type": string; url?: string; itemListElement?: { name: string }[] }[];
    }
  )["@graph"];
  const types = graph.map((node) => node["@type"]);
  for (const type of ["WebPage", "ItemList", "BreadcrumbList", "Organization"]) {
    expect(types).toContain(type);
  }
  expect(types).not.toContain("SoftwareApplication");
  const list = graph.find((node) => node["@type"] === "ItemList")?.itemListElement ?? [];
  expect(list.map((item) => item.name)).toEqual([
    "convt",
    "VERT",
    "File Converter",
    "Convertio",
    "FreeConvert",
  ]);
  expect(graph.find((node) => node["@type"] === "WebPage")?.url).toBe(url);
});

test("the CloudConvert page tags downloads, gives every tool the same block and prints tested commands", () => {
  const html = render(Route);
  const downloads = [...html.matchAll(/href="(\/download[^"]*)"/g)].map((m) => m[1]);
  expect(downloads.length).toBeGreaterThan(0);
  for (const href of downloads) {
    expect(href).toBe(
      "/download?utm_source=convt_guides&amp;utm_medium=organic&amp;utm_campaign=cloudconvert-alternative",
    );
  }
  for (const tool of ["convt", "VERT", "File Converter", "Convertio", "FreeConvert"]) {
    expect(html).toContain(`${tool} is best for`);
    expect(html).toContain(`What ${tool} does with your files`);
    expect(html).toContain(`Where ${tool} falls short`);
  }
  expect(html).toContain("Where CloudConvert is better");
  expect(html).toContain("Is there an offline alternative to CloudConvert?");
  expect(html).toContain("convt Photos --to webp -r -o webp");
  expect(html).toContain("2 converted, 0 failed");
  expect(html).toContain('href="/api"');
});

test("the CloudConvert page is in the sitemap, llms.txt, the footer, the HEIC guide and the HandBrake page", () => {
  const entry = sitemapEntries.find((e) => e.path === cloudconvertAlternative.path);
  expect(entry?.lastmod).toMatch(/^\d{4}-\d{2}-\d{2}$/);
  expect(llmsTxt).toContain(`(${url})`);
  const hrefs = footerColumns.flatMap((column) => column.links.map((link) => link.href));
  expect(hrefs).toContain(cloudconvertAlternative.path);
  expect(render(HeicRoute)).toContain(`href="${cloudconvertAlternative.path}"`);
  expect(render(HandbrakeRoute)).toContain(`href="${cloudconvertAlternative.path}"`);
});
