// Test harness: a disposable database (packages/db/scripts/test-db.sh), the billing
// mock on a loopback port reached through the real Polar SDK client, the mock's
// Resend for mail, a random signing key, and the billing clock following the
// mock's clock. Tests connect as convt_billing, like the Worker.

import { randomBytes } from "node:crypto";

import { connect, type Db } from "@convt/db";
import { freshDatabase, type TestDatabase } from "@convt/db/testing";
import { base64urlEncode, importSigningKey, newId, publicKeyOf } from "@convt/license";
import { resendTransport } from "@convt/mail";
import { createBillingMock, type BillingMock, type HeldDelivery } from "@convt/billing-mock";
import { sql } from "drizzle-orm";

import type { AnalyticsEvent } from "./analytics";
import { type CatalogProduct, loadCatalog } from "./catalog";
import type { FaultPoint } from "./context";
import { createPolarProvider } from "./polar";
import { createBillingService } from "./service";

export type Harness = Awaited<ReturnType<typeof createHarness>>;

export async function createHarness(opts: { startMs?: number } = {}) {
  const tdb: TestDatabase = await freshDatabase();
  const secret = `whsec_${randomBytes(32).toString("base64")}`;
  const token = `polar_oat_${randomBytes(8).toString("hex")}`;
  let mock!: BillingMock;
  const server = Bun.serve({ hostname: "127.0.0.1", port: 0, fetch: (r) => mock.fetch(r) });
  const base = `http://127.0.0.1:${server.port}`;
  mock = createBillingMock({
    publicUrl: base,
    accessToken: token,
    resendApiKey: "re_test",
    webhook: { secret, mode: "hold" },
    startMs: opts.startMs,
  });
  const catalog = loadCatalog("local");
  const provider = createPolarProvider({
    accessToken: token,
    baseUrl: base,
    webhookSecret: secret,
    catalog,
    portalOrigin: base,
  });
  const seed = randomBytes(32);
  const key = await importSigningKey(new Uint8Array(seed));
  const faults = new Map<FaultPoint, () => void | Promise<void>>();
  const logs: string[] = [];
  const analytics: AnalyticsEvent[] = [];
  let mailTimeoutMs = 2000;
  const mail = {
    name: "resend" as const,
    send: (e: Parameters<ReturnType<typeof resendTransport>["send"]>[0], k: string) =>
      resendTransport({ apiKey: "re_test", baseUrl: base, timeoutMs: mailTimeoutMs }).send(e, k),
  };
  const service = createBillingService({
    connect: async () => {
      const { client, db } = await connect(tdb.billingUrl);
      return { db, close: () => client.end() };
    },
    provider,
    catalog,
    mail,
    signingKey: async () => key,
    config: {
      siteUrl: "http://localhost:3000",
      mailFrom: "convt <hello@convt.test>",
      alertEmail: "alerts@convt.test",
      downloadUrl: "http://localhost:3000/download/mac",
      budgetMs: 5000,
      checkoutCookie: "convt_checkout",
    },
    clock: () => mock.now(),
    fault: async (p) => {
      const f = faults.get(p);
      if (f) await f();
    },
    log: (l) => logs.push(l),
    captureAnalytics: async (event) => {
      analytics.push(event);
    },
  });
  const owner = await tdb.open("owner");

  const bytes = (s: string) => new TextEncoder().encode(s);
  const h = {
    tdb,
    mock,
    base,
    secret,
    provider,
    catalog,
    service,
    key,
    publicKey: await publicKeyOf(key),
    seedText: base64urlEncode(new Uint8Array(seed)),
    logs,
    analytics,
    owner: owner.db as Db,
    setMailTimeout: (ms: number) => {
      mailTimeoutMs = ms;
    },
    /** Throws at a named point, `times` times (default once). */
    failAt(point: FaultPoint, times = 1, error = "injected fault") {
      let left = times;
      faults.set(point, () => {
        if (left-- > 0) throw new Error(error);
      });
    },
    onFault(point: FaultPoint, fn: () => void | Promise<void>) {
      faults.set(point, fn);
    },
    clearFaults: () => faults.clear(),
    async q<T = Record<string, unknown>>(query: ReturnType<typeof sql>): Promise<T[]> {
      return (await owner.db.execute(query)).rows as T[];
    },
    async user(email: string, verified = true) {
      const id = newId("usr");
      await owner.db.execute(
        sql`insert into users (id, name, email, email_verified) values (${id}, '', ${email}, ${verified})`,
      );
      return { id, email };
    },
    async deliver(
      d: HeldDelivery,
      opts: {
        secret?: string;
        scheme?: "standard" | "legacy";
        timestamp?: number;
        body?: string;
      } = {},
    ) {
      const headers = mock.sign(opts.body ? { ...d, body: opts.body } : d, opts);
      return service.handleWebhook("POST", bytes(opts.body ?? d.body), new Headers(headers));
    },
    /** Delivers every held event in order (or the given order). */
    async deliverAll(order?: (ds: HeldDelivery[]) => HeldDelivery[]) {
      const held = mock.takeHeld();
      const list = order ? order(held) : held;
      const out = [];
      for (const d of list) out.push(await h.deliver(d));
      return out;
    },
    /** Creates a checkout through the service and pays it in the mock. */
    async buy(
      product: CatalogProduct,
      user: { id: string; email: string } | null,
      opts: { card?: string; email?: string; cap?: number } = {},
    ) {
      const created = await service.createCheckout({
        product,
        user,
        spendCapCents: opts.cap ?? (product === "api" ? 2000 : null),
      });
      if (!created.ok) throw new Error(`checkout refused: ${created.refusal}`);
      const secret = created.url.split("/checkout/")[1];
      const co = mock.checkoutBySecret(secret)!;
      const paid = mock.completeCheckout(
        co.id,
        opts.card ?? "4242",
        opts.email ?? user?.email ?? "guest@convt.test",
      );
      return { ...created, providerCheckoutId: co.id, paid };
    },
    async counts() {
      const [r] = await h.q<Record<string, number>>(sql`
        select (select count(*)::int from orders) as orders, (select count(*)::int from invoices) as invoices,
          (select count(*)::int from subscriptions) as subscriptions, (select count(*)::int from licenses) as licenses,
          (select count(*)::int from email_outbox) as outbox, (select count(*)::int from webhook_events) as events,
          (select count(*)::int from payment_coverage) as coverage, (select count(*)::int from disputes) as disputes,
          (select count(*)::int from billing_customers) as customers, (select count(*)::int from billing_alerts) as alerts`);
      return r;
    },
    async close() {
      server.stop(true);
      await tdb.drop();
    },
  };
  return h;
}
