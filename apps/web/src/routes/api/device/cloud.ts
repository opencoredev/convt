import { createFileRoute } from "@tanstack/react-router";

import { handleDevice } from "#/server/device-routes";

// POST /api/device/cloud, called by the desktop app (see server/device-auth.ts).
export const Route = createFileRoute("/api/device/cloud")({
  server: {
    handlers: {
      POST: ({ request, context }) => handleDevice("cloud", request, context),
    },
  },
});
