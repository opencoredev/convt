import { createFileRoute } from "@tanstack/react-router";

// The release manifest's purchase_url and the desktop app's buy links point here.
// Pricing lives on the landing page, so this redirects to that section.
export const Route = createFileRoute("/pricing")({
  server: {
    handlers: {
      GET: () => new Response(null, { status: 302, headers: { location: "/#pricing" } }),
    },
  },
});
