import { createFileRoute } from "@tanstack/react-router";

import { startCheckout } from "#/server/checkout-start";

// GET /checkout/desktop: allowed signed out. Answers 303 to the provider's checkout.
export const Route = createFileRoute("/checkout/desktop")({
  server: {
    handlers: {
      GET: ({ request, context }) => startCheckout(request, context, "desktop"),
    },
  },
});
