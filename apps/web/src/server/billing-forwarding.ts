import type { AppEnv } from "./env";

/** Webhooks have their own production route; the manual cron trigger is local only. */
export function billingForwarding(env: AppEnv["env"], path: string) {
  if (path.startsWith("/__billing/")) return env === "development" ? "forward" : "deny";
  if (path.startsWith("/webhooks/")) return env === "production" ? "deny" : "forward";
  return null;
}
