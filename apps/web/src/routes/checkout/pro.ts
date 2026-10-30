import { createFileRoute } from "@tanstack/react-router";

import { startCheckout } from "#/server/checkout-start";

// GET /checkout/pro?from=app: the desktop app's Pro trial, the one way Pro is still
// started (CNV-101). Requires sign-in. Without from=app it answers 303 to pricing.
export const Route = createFileRoute("/checkout/pro")({
  server: {
    handlers: {
      GET: ({ request, context }) => startCheckout(request, context, "pro"),
    },
  },
});
