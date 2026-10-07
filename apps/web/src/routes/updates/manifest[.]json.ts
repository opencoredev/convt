import { createFileRoute } from "@tanstack/react-router";

import { LATEST_UPDATE_MANIFEST_URL } from "#/server/latest-release";

// The desktop app polls this fixed URL (UPDATE_MANIFEST_URL in convt-app). The signed
// manifest lives on the newest GitHub release; the app checks its signature against
// the key built into it, wherever the bytes came from.
export const Route = createFileRoute("/updates/manifest.json")({
  server: {
    handlers: {
      GET: () =>
        new Response(null, {
          status: 302,
          headers: { location: LATEST_UPDATE_MANIFEST_URL, "cache-control": "public, max-age=300" },
        }),
    },
  },
});
