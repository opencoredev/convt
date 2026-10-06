// Request middleware: the Origin check, and one Postgres connection per request
// through Hyperdrive (locally, the container URL Wrangler reads from
// CLOUDFLARE_HYPERDRIVE_LOCAL_CONNECTION_STRING_HYPERDRIVE). The connection closes
// after the response, or in waitUntil once background work (such as Better Auth's
// email sending) has finished.

import { createDb } from "@convt/db";
import { createMiddleware } from "@tanstack/react-start";
import { env as rawEnv, waitUntil } from "cloudflare:workers";
import { LazyClient } from "./lazy-client";

import type { RequestScope } from "./auth";
import { readEnv, type AppEnv } from "./env";
import { isSameOriginRequest } from "./origin";

export type RequestContext = { scope: RequestScope; appEnv: AppEnv };

export const requestMiddleware = createMiddleware({ type: "request" }).server(
  async ({ next, request }) => {
    const appEnv = readEnv(rawEnv);
    const url = new URL(request.url);
    // In production convt.app/webhooks/* is routed to convt-billing and never
    // reaches this Worker. Locally the billing Worker has no port of its own, so
    // the dev server hands it webhooks and its cron trigger.
    if (url.pathname.startsWith("/webhooks/") || url.pathname.startsWith("/__billing/")) {
      if (appEnv.env === "production") return new Response("Not found", { status: 404 });
      return rawEnv.BILLING.fetch(request);
    }
    // Better Auth checks Origin on its own endpoints; OAuth providers may POST callbacks.
    // The desktop app's /api/device calls carry no cookies and no Origin; they
    // authenticate by one-time code and verifier or by device token.
    if (
      !url.pathname.startsWith("/api/auth/") &&
      !url.pathname.startsWith("/api/device/") &&
      !isSameOriginRequest(request, new URL(appEnv.authUrl).origin)
    ) {
      return new Response("Forbidden", { status: 403 });
    }
    const client = new LazyClient({ connectionString: rawEnv.HYPERDRIVE.connectionString });
    const pending: Promise<unknown>[] = [];
    const scope: RequestScope = {
      db: createDb(client),
      background: (work) => {
        pending.push(
          work.catch((e) =>
            console.error("[request] background task failed", e instanceof Error ? e.message : e),
          ),
        );
      },
    };
    const close = async () => {
      while (pending.length) await Promise.allSettled(pending.splice(0));
      await client.close();
    };
    try {
      const result = await next({ context: { scope, appEnv } satisfies RequestContext });
      if (pending.length) waitUntil(close());
      else await close();
      return result;
    } catch (e) {
      waitUntil(close());
      throw e;
    }
  },
);
