// The billing service: every operation convt-billing performs, each on its own
// database connection. The Worker's webhook route, crons and RPC entrypoint call
// it; the tests call it directly with a mock provider and an injected clock.

import type { Db } from "@convt/db";
import type { MailTransport } from "@convt/mail";

import * as actions from "./actions";
import type { Catalog, CatalogProduct } from "./catalog";
import { cleanupAuth } from "./cleanup";
import { checkoutResult, createCheckout } from "./checkout";
import type { BillingConfig, BillingContext, FaultPoint } from "./context";
import { advanceDeletion, deletionStatus, requestDeletion, runDeletions } from "./deletion";
import { drainOutbox, resolveOutbox } from "./outbox";
import type { BillingProvider } from "./provider";
import { reconcileDaily, reconcileFrequent } from "./reconcile";
import { handleWebhook } from "./webhook";

export type ServiceDeps = {
  connect: () => Promise<{ db: Db; close: () => Promise<void> }>;
  provider: BillingProvider;
  catalog: Catalog;
  mail: MailTransport;
  signingKey: () => Promise<CryptoKey>;
  config: BillingConfig;
  clock?: () => Date;
  fault?: (point: FaultPoint) => void | Promise<void>;
  log?: (line: string) => void;
};

export function createBillingService(deps: ServiceDeps) {
  const withCtx = async <T>(fn: (ctx: BillingContext) => Promise<T>): Promise<T> => {
    const { db, close } = await deps.connect();
    try {
      return await fn({
        db,
        provider: deps.provider,
        catalog: deps.catalog,
        clock: deps.clock ?? (() => new Date()),
        signingKey: deps.signingKey,
        mail: deps.mail,
        config: deps.config,
        fault: deps.fault,
        log: deps.log ?? ((l) => console.log(l)),
      });
    } finally {
      await close();
    }
  };

  return {
    withCtx,
    handleWebhook: (method: string, raw: Uint8Array, headers: Headers) =>
      withCtx((c) => handleWebhook(c, method, raw, headers)),
    drainOutbox: () => withCtx((c) => drainOutbox(c)),
    resolveOutbox: (id: string, decision: "sent" | "resend") =>
      withCtx((c) => resolveOutbox(c, id, decision)),
    reconcileFrequent: () => withCtx((c) => reconcileFrequent(c)),
    reconcileDaily: () => withCtx((c) => reconcileDaily(c)),
    cleanupAuth: () => withCtx((c) => cleanupAuth(c)),
    runDeletions: () => withCtx((c) => runDeletions(c)),

    createCheckout: (input: {
      product: CatalogProduct;
      user: { id: string; email: string } | null;
      spendCapCents?: number | null;
    }) => withCtx((c) => createCheckout(c, input)),
    checkoutResult: (input: {
      providerCheckoutId: string;
      cookie: string | null;
      sessionUserId: string | null;
      sync: boolean;
    }) => withCtx((c) => checkoutResult(c, input)),
    switchInterval: (userId: string, to: "month" | "year") =>
      withCtx((c) => actions.switchInterval(c, userId, to)),
    setCancel: (userId: string, kind: "pro" | "api", cancel: boolean) =>
      withCtx((c) => actions.setCancel(c, userId, kind, cancel)),
    portalUrl: (userId: string) => withCtx((c) => actions.portalUrl(c, userId)),
    receiptUrl: (userId: string, invoiceId: string) =>
      withCtx((c) => actions.receiptUrl(c, userId, invoiceId)),
    card: (userId: string) => withCtx((c) => actions.card(c, userId)),
    setSpendCap: (userId: string, cents: number) =>
      withCtx((c) => actions.setSpendCap(c, userId, cents)),
    multipleSubscriptionsAllowed: () =>
      withCtx(async (c) => {
        try {
          return (await c.provider.settings()).allowMultipleSubscriptions;
        } catch {
          return null;
        }
      }),
    requestDeletion: (userId: string) =>
      withCtx(async (c) => {
        const d = await requestDeletion(c, userId);
        return { id: d.id, status: d.status };
      }),
    advanceDeletion: (id: string) => withCtx((c) => advanceDeletion(c, id)),
    deletionStatus: (userId: string) => withCtx((c) => deletionStatus(c, userId)),
  };
}

export type BillingService = ReturnType<typeof createBillingService>;
