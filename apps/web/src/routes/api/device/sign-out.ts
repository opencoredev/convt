import { createFileRoute } from "@tanstack/react-router";

import { handleDevice } from "#/server/device-routes";

// POST /api/device/sign-out, called by the desktop app (see server/device-auth.ts).
export const Route = createFileRoute("/api/device/sign-out")({
  server: {
    handlers: {
      POST: ({ request, context }) => handleDevice("sign-out", request, context),
    },
  },
});
