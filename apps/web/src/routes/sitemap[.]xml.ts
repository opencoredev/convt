import { createFileRoute } from "@tanstack/react-router";

import { sitemapEntries } from "#/lib/sitemap";
import { SITE_ORIGIN } from "#/lib/site";

const xml = `<?xml version="1.0" encoding="UTF-8"?>
<urlset xmlns="http://www.sitemaps.org/schemas/sitemap/0.9">
${sitemapEntries
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
