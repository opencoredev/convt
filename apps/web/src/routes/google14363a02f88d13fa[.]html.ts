import { createFileRoute } from "@tanstack/react-router";

// Search Console's HTML-file verification for the https://convt.app/ property. It is a
// route rather than a file in public/ because static assets redirect `/x.html` to `/x`,
// and Google needs the file itself to answer 200. Keep it while the property exists.
const body = "google-site-verification: google14363a02f88d13fa.html";

export const Route = createFileRoute("/google14363a02f88d13fa.html")({
  server: {
    handlers: {
      GET: () =>
        new Response(body, {
          headers: {
            "content-type": "text/html; charset=utf-8",
            "cache-control": "public, max-age=3600",
          },
        }),
    },
  },
});
