import { createFileRoute } from "@tanstack/react-router";

import { startCheckout } from "#/server/checkout-start";

// GET /checkout/bundle: Desktop + Cloud. Requires sign-in; answers 303 to the provider's checkout.
export const Route = createFileRoute("/checkout/bundle")({
  server: {
    handlers: {
      GET: ({ request, context }) => startCheckout(request, context, "bundle"),
    },
  },
});
