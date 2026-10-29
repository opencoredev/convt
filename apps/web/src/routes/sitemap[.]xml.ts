import { createFileRoute } from "@tanstack/react-router";

import { guidePath, guides } from "#/lib/guides";
import { SITE_ORIGIN, routes, sitemapPages } from "#/lib/site";

/** The guides index changes whenever a guide is added or updated. */
const guidesLastmod = guides.map((guide) => guide.updated).reduce((a, b) => (a > b ? a : b));

const pages = [
  ...sitemapPages,
  { path: routes.guides, lastmod: guidesLastmod },
  ...guides.map((guide) => ({ path: guidePath(guide.slug), lastmod: guide.updated })),
];

const xml = `<?xml version="1.0" encoding="UTF-8"?>
<urlset xmlns="http://www.sitemaps.org/schemas/sitemap/0.9">
${pages
  .map(
    ({ path, lastmod }) =>
      `  <url><loc>${SITE_ORIGIN}${path}</loc><lastmod>${lastmod}</lastmod></url>`,
  )
  .join("\n")}
</urlset>
`;

export const Route = createFileRoute("/sitemap.xml")({
  server: {
    handlers: {
      GET: () =>
        new Response(xml, {
          headers: {
            "content-type": "application/xml; charset=utf-8",
            "cache-control": "public, max-age=3600",
          },
        }),
    },
  },
});
