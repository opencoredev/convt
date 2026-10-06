import { createFileRoute } from "@tanstack/react-router";

import { handleDevice } from "#/server/device-routes";

// POST /api/device/token, called by the desktop app (see server/device-auth.ts).
export const Route = createFileRoute("/api/device/token")({
  server: {
    handlers: {
      POST: ({ request, context }) => handleDevice("token", request, context),
    },
  },
});
