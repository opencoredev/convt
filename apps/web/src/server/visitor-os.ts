import { createServerFn } from "@tanstack/react-start";
import { getRequestHeader } from "@tanstack/react-start/server";

import { osFromUserAgent, type Os } from "#/lib/platform";

const detectOs = createServerFn({ method: "GET" }).handler(() =>
  osFromUserAgent(getRequestHeader("user-agent") ?? ""),
);

/**
 * The visitor's OS for a route loader: the request's User-Agent on the first load,
 * the browser's on client navigations. Check `window`, not `navigator`: Workers
 * define navigator with their own user agent.
 */
export function visitorOs(): Promise<Os | null> | Os | null {
  return typeof window === "undefined" ? detectOs() : osFromUserAgent(navigator.userAgent);
}
