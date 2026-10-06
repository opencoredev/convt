import { createFileRoute } from "@tanstack/react-router";

import { SITE_ORIGIN } from "#/lib/site";

// Account, checkout and API routes are private or useless to crawlers; the account
// pages also send `robots: noindex`.
const body = `User-agent: *
Disallow: /dashboard
Disallow: /account
Disallow: /sign-in
Disallow: /checkout
Disallow: /api/
Disallow: /device
Allow: /

Sitemap: ${SITE_ORIGIN}/sitemap.xml
`;

export const Route = createFileRoute("/robots.txt")({
  server: {
    handlers: {
      GET: () =>
        new Response(body, {
          headers: {
            "content-type": "text/plain; charset=utf-8",
            "cache-control": "public, max-age=3600",
          },
        }),
    },
  },
});
