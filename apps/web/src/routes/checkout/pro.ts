import { createFileRoute } from "@tanstack/react-router";

import { startCheckout } from "#/server/checkout-start";

// GET /checkout/pro?interval=month|year: requires sign-in.
export const Route = createFileRoute("/checkout/pro")({
  server: {
    handlers: {
      GET: ({ request, context }) => startCheckout(request, context, "pro"),
    },
  },
});
