import { createFileRoute } from "@tanstack/react-router";

import { createAuth } from "#/server/auth";
import { requestContext } from "#/server/context";

// Better Auth's endpoints. The handlers are inline so the browser bundle never
// imports the auth stack (the Start compiler strips server handler bodies).
export const Route = createFileRoute("/api/auth/$")({
  server: {
    handlers: {
      GET: ({ request, context }) => {
        const { scope, appEnv } = requestContext(context);
        return createAuth(scope, appEnv).handler(request);
      },
      POST: ({ request, context }) => {
        const { scope, appEnv } = requestContext(context);
        return createAuth(scope, appEnv).handler(request);
      },
    },
  },
});
