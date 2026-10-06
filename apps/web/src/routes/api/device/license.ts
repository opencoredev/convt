import { createFileRoute } from "@tanstack/react-router";

import { handleDevice } from "#/server/device-routes";

// POST /api/device/license, called by the desktop app (see server/device-auth.ts).
export const Route = createFileRoute("/api/device/license")({
  server: {
    handlers: {
      POST: ({ request, context }) => handleDevice("license", request, context),
    },
  },
});
