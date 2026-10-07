import { env, waitUntil } from "cloudflare:workers";
import { scrub } from "./posthog-scrub";

export { requestAllowsServerExceptions, scrub } from "./posthog-scrub";

export function captureServerException(error: unknown, context: string, allowed = true): void {
  if (!allowed) return;
  const key = (env as unknown as Record<string, unknown>).POSTHOG_KEY;
  if (typeof key !== "string" || !key) return;
  const value = error instanceof Error ? error.message : String(error);
  const stack = error instanceof Error ? error.stack ?? "" : "";
  const payload = {
    api_key: key,
    event: "$exception",
    properties: {
      $lib: "convt-web-worker",
      $lib_version: "0.2.0",
      error_context: context,
      $exception_list: [{ type: error instanceof Error ? error.name : "Error", value: scrub(value), stacktrace: { raw: scrub(stack), frames: [] } }],
    },
  };
  waitUntil(fetch("https://us.i.posthog.com/capture/", {
    method: "POST", headers: { "content-type": "application/json" }, body: JSON.stringify(payload),
  }).catch(() => undefined));
}
