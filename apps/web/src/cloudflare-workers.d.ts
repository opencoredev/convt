// The parts of the Workers runtime module this app uses. Bindings are declared in
// wrangler.jsonc; vars and secrets come from wrangler.jsonc, `wrangler secret` and,
// locally, .dev.vars.
declare module "cloudflare:workers" {
  export const env: Record<string, unknown> & {
    HYPERDRIVE: { connectionString: string };
    /** convt-billing's BillingRpc entrypoint (wrangler.jsonc `services`). */
    BILLING: import("@convt/billing/rpc").BillingRpc & {
      fetch(request: Request): Promise<Response>;
    };
  };
  export function waitUntil(promise: Promise<unknown>): void;
}
