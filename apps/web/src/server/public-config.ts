import { createServerFn } from "@tanstack/react-start";
import { setResponseHeader } from "@tanstack/react-start/server";

import { availableProviders } from "./env";
import { requestContext } from "./context";

/** Public availability only; credentials stay in the Worker. */
export const getPublicConfig = createServerFn({ method: "GET" }).handler(({ context }) => {
  setResponseHeader("cache-control", "no-store");
  const { appEnv } = requestContext(context);
  return {
    sales: appEnv.sales,
    providers: availableProviders(appEnv),
    posthog: appEnv.posthog,
  };
});
