// The parts of the Workers runtime this Worker uses.
declare module "cloudflare:workers" {
  export class WorkerEntrypoint<E = unknown> {
    protected env: E;
    protected ctx: { waitUntil(promise: Promise<unknown>): void };
    constructor(ctx: unknown, env: E);
  }
}
