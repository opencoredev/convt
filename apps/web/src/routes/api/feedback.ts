import { createFileRoute } from "@tanstack/react-router";

import { handleFeedback } from "#/server/feedback-route";

// POST /api/feedback, from the site's feedback dialog and the desktop app (see server/feedback.ts).
export const Route = createFileRoute("/api/feedback")({
  server: {
    handlers: {
      POST: ({ request, context }) => handleFeedback(request, context),
    },
  },
});
