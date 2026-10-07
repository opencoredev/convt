// Public PostHog config for the browser. The project API token is safe to ship
// in the client; it is not a secret. Prefer VITE_PUBLIC_POSTHOG_KEY (Worker var
// and Vite public env). Production builds fall back to the convt.app token so a
// tip redeploy still captures pageviews if the build env omitted it.

/** US cloud ingest. */
export const POSTHOG_HOST = "https://us.i.posthog.com";

/** PostHog US project 650236 (convt.app). */
export const POSTHOG_PROJECT_TOKEN = "phc_yg96HDaDax6n2MmN7QyzvJjSh5qq2AwMUvaRnhmbJwMw";

/**
 * Resolve the browser token.
 * `envKey` is VITE_PUBLIC_POSTHOG_KEY (or a Worker-provided copy). An explicit
 * empty string disables analytics. When the env is unset, production uses the
 * public project token.
 */
export function resolvePosthogKey(
  envKey: string | undefined | null,
  production: boolean,
): string | undefined {
  if (typeof envKey === "string") {
    const trimmed = envKey.trim();
    return trimmed === "" ? undefined : trimmed;
  }
  return production ? POSTHOG_PROJECT_TOKEN : undefined;
}

export function clientPosthogKey(): string | undefined {
  return resolvePosthogKey(import.meta.env.VITE_PUBLIC_POSTHOG_KEY, import.meta.env.PROD);
}
