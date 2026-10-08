import { readWebhookBody, WebhookBodyError } from "./webhook-body";
// convt-billing: the Worker that owns every write to billing state and every
// billing secret. Public surface: POST /webhooks/polar only. The site calls the
// BillingRpc entrypoint through a service binding. Crons drain the outbox, drive
// account deletions, and run the reconciler. See docs/p7-billing-plan.md, section 2.

import { WorkerEntrypoint } from "cloudflare:workers";
import {
  type BillingRpc as Rpc,
  ConfigError,
  analyticsEventsToCapture,
  captureEvent,
  createBillingService,
  emitAnalytics,
  currentProKey,
  currentProAccess,
  createPolarProvider,
  loadCatalog,
  loadSigningKey,
  readBillingEnv,
  type BillingEnv,
  type BillingService,
  validateCatalog,
} from "@convt/billing";
import { createDb } from "@convt/db";
import { logTransport, resendTransport, sequenzyTransport } from "@convt/mail";
import pg from "pg";

type Env = Record<string, unknown> & { HYPERDRIVE_BILLING: { connectionString: string } };
type Ctx = { waitUntil(p: Promise<unknown>): void };

let cached: { env: BillingEnv; key: Promise<CryptoKey>; service: BillingService } | null = null;

/** Settings, key and service for this isolate. Throws ConfigError on a bad setup. */
function setup(raw: Env) {
  if (cached) return cached;
  const env = readBillingEnv(raw);
  const catalog = loadCatalog(env.catalogEnv, env.desktopProduct);
  const key = loadSigningKey(env);
  key.catch(() => {});
  const service = createBillingService({
    connect: async () => {
      const client = new pg.Client({ connectionString: raw.HYPERDRIVE_BILLING.connectionString });
      await client.connect();
      return { db: createDb(client), close: () => client.end() };
    },
    provider: createPolarProvider({
      accessToken: env.polar.accessToken,
      baseUrl: env.polar.apiUrl,
      webhookSecret: env.polar.webhookSecret,
      catalog,
      portalOrigin: env.polar.portalOrigin,
    }),
    catalog,
    mail:
      env.mail.transport === "resend"
        ? resendTransport({ apiKey: env.mail.apiKey, baseUrl: env.mail.apiUrl })
        : env.mail.transport === "sequenzy"
          ? sequenzyTransport({ apiKey: env.mail.apiKey })
          : logTransport(),
    signingKey: () => key,
    captureAnalytics: (event) => captureEvent(env.posthog, event),
    config: {
      siteUrl: env.siteUrl,
      mailFrom: env.mail.from,
      alertEmail: env.alertEmail,
      downloadUrl: env.downloadUrl,
      budgetMs: 5000,
      checkoutCookie:
        env.env === "production" || env.env === "staging"
          ? "__Host-convt_checkout"
          : "convt_checkout",
    },
  });
  cached = { env, key, service };
  return cached;
}

const text = (status: number, body: string) =>
  new Response(body, {
    status,
    headers: { "content-type": "text/plain; charset=utf-8", "cache-control": "no-store" },
  });

export async function runCron(service: BillingService, cron: string) {
  if (cron === "*/10 * * * *") return { cleanup: await service.cleanupAuth() };
  if (cron === "*/15 * * * *") return { reconcile: await service.reconcileFrequent() };
  if (cron === "17 3 * * *") return { daily: await service.reconcileDaily() };
  return { outbox: await service.drainOutbox(), deletions: await service.runDeletions() };
}

async function handleFetch(request: Request, raw: Env, ctx: Ctx): Promise<Response> {
  const url = new URL(request.url);
  if (url.pathname === "/__billing/scheduled" && raw.ENV !== "development")
    return text(404, "not found");
  let s: ReturnType<typeof setup>;
  try {
    s = setup(raw);
    // A missing or malformed signing key is a 500 before any transaction.
    await s.key;
  } catch (e) {
    console.error(
      `[billing] not configured: ${e instanceof ConfigError ? e.message : "setup failed"}`,
    );
    return text(500, "billing is not configured");
  }
  if (url.pathname === "/webhooks/polar") {
    let raw: Uint8Array;
    try {
      raw = await readWebhookBody(request);
    } catch (e) {
      if (e instanceof WebhookBodyError) return text(e.status, e.message);
      return text(400, "invalid body");
    }
    const result = await s.service.handleWebhook(request.method, raw, request.headers);
    const pending = analyticsEventsToCapture(result.analytics, request.headers);
    if (pending.length)
      ctx.waitUntil(
        emitAnalytics((event) => captureEvent(s.env.posthog, event), pending).catch((e) =>
          console.warn("[billing] analytics", (e as Error).message),
        ),
      );
    if (result.drain)
      ctx.waitUntil(
        s.service
          .drainOutbox()
          .catch((e) => console.error("[billing] drain", (e as Error).message)),
      );
    return text(result.status, result.body);
  }
  // Local development only: run a cron by name (Wrangler's /__scheduled is not
  // reachable for an auxiliary Worker under the Vite plugin).
  if (url.pathname === "/__billing/scheduled" && s.env.env === "development") {
    const cron = url.searchParams.get("cron") ?? "* * * * *";
    return Response.json(await runCron(s.service, cron));
  }
  return text(404, "not found");
}

export default {
  fetch: handleFetch,

  async scheduled(controller: { cron: string }, raw: Env, ctx: Ctx) {
    const s = setup(raw);
    await s.key;
    ctx.waitUntil(runCron(s.service, controller.cron));
  },
};

/** What the site may call through its service binding (src/rpc.ts in packages/billing). */
export class BillingRpc extends WorkerEntrypoint<Env> implements Rpc {
  private get service() {
    return setup(this.env).service;
  }
  /** The same routes as the public handler; the site forwards webhooks here locally. */
  fetch(request: Request) {
    return handleFetch(request, this.env, this.ctx);
  }
  async health() {
    const problems: string[] = [];
    try {
      const s = setup(this.env);
      await s.key;
      problems.push(...validateCatalog(loadCatalog(s.env.catalogEnv, s.env.desktopProduct)));
    } catch (e) {
      problems.push(e instanceof Error ? e.message : "setup failed");
    }
    return { ok: problems.length === 0, problems };
  }
  createCheckout(input: Parameters<Rpc["createCheckout"]>[0]) {
    return this.service.createCheckout(input);
  }
  checkoutResult(input: Parameters<Rpc["checkoutResult"]>[0]) {
    return this.service.checkoutResult(input);
  }
  switchInterval(userId: string, to: "month" | "year") {
    return this.service.switchInterval(userId, to);
  }
  setCancel(userId: string, kind: "pro" | "api", cancel: boolean) {
    return this.service.setCancel(userId, kind, cancel);
  }
  portalUrl(userId: string) {
    return this.service.portalUrl(userId);
  }
  receiptUrl(userId: string, invoiceId: string) {
    return this.service.receiptUrl(userId, invoiceId);
  }
  card(userId: string) {
    return this.service.card(userId);
  }
  setSpendCap(userId: string, cents: number) {
    return this.service.setSpendCap(userId, cents);
  }
  multipleSubscriptionsAllowed() {
    return this.service.multipleSubscriptionsAllowed();
  }
  /** P8: the Pro key the desktop app renews to. A read; issuance stays with ingest. */
  currentProKey(userId: string) {
    return this.service.withCtx((c) => currentProKey(c.db, userId));
  }
  async currentProAccess(userId: string) {
    return this.service.withCtx((c) => currentProAccess(c, userId));
  }
  async requestDeletion(userId: string) {
    const d = await this.service.requestDeletion(userId);
    // Start right away; the minute cron resumes it if this is cut short.
    this.ctx.waitUntil(this.service.advanceDeletion(d.id).catch(() => {}));
    return d;
  }
}
