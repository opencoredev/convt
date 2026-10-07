import { createFileRoute } from "@tanstack/react-router";

import { handleDevice } from "#/server/device-routes";

// POST /api/device/trial, called by the desktop app (see server/device-auth.ts).
export const Route = createFileRoute("/api/device/trial")({
  server: {
    handlers: {
      POST: ({ request, context }) => handleDevice("trial", request, context),
    },
  },
});
