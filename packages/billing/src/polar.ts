// The Polar adapter: the only code that knows Polar's ids, payloads and signature
// scheme. It calls Polar through the official SDK client (API version 2026-10; a
// loopback baseUrl points it at tools/billing-mock locally) and validates every
// payload with zod schemas whose shapes are checked against the SDK's own model
// types at compile time (the `Conforms` lines below).

import { createHash } from "node:crypto";

import { createPolar, type models } from "@polar-sh/sdk/2026-10";
import { z } from "zod";

import {
  type Catalog,
  type CatalogProduct,
  productByPriceId,
  productByProviderId,
} from "./catalog";
import {
  type BillingProvider,
  type CardFact,
  type CheckoutFact,
  type CoverageItem,
  type DisputeFact,
  emptyFacts,
  type OrderFact,
  type ParsedEvent,
  type ProviderFacts,
  ProviderError,
  type ScanKind,
  type SubscriptionFact,
} from "./provider";
import { verifyWebhook } from "./verify";

// ------------------------------------------------------------------ schemas

const ts = z.string().regex(/^\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}(\.\d+)?(Z|[+-]\d{2}:\d{2})$/);
const int = z.number().int();
const metadata = z.record(z.string(), z.union([z.string(), z.number(), z.boolean()]));

const customer = z.object({
  id: z.string(),
  created_at: ts,
  modified_at: ts.nullable(),
  email: z.string().nullish(),
  external_id: z.string().nullish(),
  deleted_at: ts.nullable(),
});

const subscriptionStatus = z.enum([
  "incomplete",
  "incomplete_expired",
  "trialing",
  "active",
  "past_due",
  "canceled",
  "unpaid",
  "paused",
]);

const orderSubscription = z.object({
  id: z.string(),
  created_at: ts,
  modified_at: ts.nullable(),
  status: subscriptionStatus,
  currency: z.string(),
  current_period_start: ts,
  current_period_end: ts,
  trial_end: ts.nullable(),
  cancel_at_period_end: z.boolean(),
  canceled_at: ts.nullable(),
  ended_at: ts.nullable(),
  customer_id: z.string(),
  product_id: z.string(),
  discount_id: z.string().nullable(),
  checkout_id: z.string().nullable(),
  metadata,
});

const subscription = orderSubscription.extend({
  customer,
  pending_update: z
    .object({ id: z.string(), applies_at: ts, product_id: z.string().nullable() })
    .nullable(),
});

const orderItem = z.object({
  id: z.string(),
  label: z.string(),
  amount: int,
  proration: z.boolean(),
  product_price_id: z.string().nullable(),
  start_timestamp: ts.nullable(),
  end_timestamp: ts.nullable(),
});

const order = z.object({
  id: z.string(),
  created_at: ts,
  modified_at: ts.nullable(),
  status: z.enum(["draft", "pending", "paid", "refunded", "partially_refunded", "void"]),
  paid: z.boolean(),
  subtotal_amount: int,
  discount_amount: int,
  net_amount: int,
  applied_balance_amount: int,
  refunded_amount: int,
  currency: z.string(),
  billing_reason: z.enum([
    "purchase",
    "subscription_create",
    "subscription_cycle",
    "subscription_update",
    "subscription_meter_cycle",
  ]),
  customer_id: z.string(),
  product_id: z.string().nullable(),
  discount_id: z.string().nullable(),
  subscription_id: z.string().nullable(),
  checkout_id: z.string().nullable(),
  metadata,
  customer,
  subscription: orderSubscription.nullable(),
  items: z.array(orderItem),
  description: z.string(),
});

const checkout = z.object({
  id: z.string(),
  created_at: ts,
  modified_at: ts.nullable(),
  status: z.enum(["open", "expired", "confirmed", "succeeded", "failed"]),
  metadata,
  subscription_id: z.string().nullable(),
  url: z.string(),
});

const disputeStatus = z.enum([
  "prevented",
  "early_warning",
  "needs_response",
  "under_review",
  "lost",
  "won",
]);
const disputeBase = z.object({
  id: z.string(),
  created_at: ts,
  modified_at: ts.nullable(),
  status: disputeStatus,
  amount: int,
  closed: z.boolean(),
  order_id: z.string(),
});

const refund = z.object({
  id: z.string(),
  created_at: ts,
  modified_at: ts.nullable(),
  order_id: z.string(),
  amount: int,
  dispute: disputeBase.nullable(),
});

const organization = z.object({
  id: z.string(),
  subscription_settings: z.object({
    allow_multiple_subscriptions: z.boolean(),
    prevent_trial_abuse: z.boolean(),
  }),
  customer_email_settings: z.object({
    subscription_trial_conversion_reminder: z.boolean(),
    subscription_past_due: z.boolean(),
  }),
});

const envelope = z.object({ type: z.string(), timestamp: z.string(), data: z.unknown() });

// Compile-time conformance: every field these schemas read exists in the SDK's
// 2026-10 model with a compatible type. A schema field the SDK does not have, or
// a narrower type, fails `tsc`.
type Conforms<Sdk, Ours> = Sdk extends Ours ? true : never;
const conformance: [
  Conforms<models.Order, z.input<typeof order>>,
  Conforms<models.Subscription, z.input<typeof subscription>>,
  Conforms<models.OrderSubscription, z.input<typeof orderSubscription>>,
  Conforms<models.Checkout, z.input<typeof checkout>>,
  Conforms<models.Refund, z.input<typeof refund>>,
  Conforms<models.Dispute, z.input<typeof disputeBase>>,
  Conforms<models.CustomerIndividual, z.input<typeof customer>>,
  Conforms<models.Organization, z.input<typeof organization>>,
] = [true, true, true, true, true, true, true, true];
void conformance;

export const schemas = {
  order,
  subscription,
  orderSubscription,
  checkout,
  refund,
  dispute: disputeBase,
  customer,
  organization,
  envelope,
};

// ------------------------------------------------------------------ facts

const knownEventTypes = new Set<string>([
  "checkout.created",
  "checkout.updated",
  "checkout.expired",
  "customer.created",
  "customer.updated",
  "customer.deleted",
  "customer.state_changed",
  "customer_seat.assigned",
  "customer_seat.claimed",
  "customer_seat.revoked",
  "member.created",
  "member.updated",
  "member.deleted",
  "order.created",
  "order.updated",
  "order.paid",
  "order.refunded",
  "subscription.created",
  "subscription.updated",
  "subscription.active",
  "subscription.canceled",
  "subscription.uncanceled",
  "subscription.cycled",
  "subscription.revoked",
  "subscription.past_due",
  "subscription.paused",
  "subscription.resumed",
  "subscription.migrated",
  "refund.created",
  "refund.updated",
  "product.created",
  "product.updated",
  "discount.created",
  "discount.updated",
  "discount.deleted",
  "benefit.created",
  "benefit.updated",
  "benefit_grant.created",
  "benefit_grant.cycled",
  "benefit_grant.updated",
  "benefit_grant.revoked",
  "organization.updated",
]);

function hashOf(value: unknown): string {
  return createHash("sha256").update(JSON.stringify(value)).digest("hex");
}

const versionOf = (x: { created_at: string; modified_at: string | null }) =>
  x.modified_at ?? x.created_at;

const userIdOf = (externalId: string | null | undefined) =>
  externalId && /^usr_[0-9a-z]{26}$/.test(externalId) ? externalId : (externalId ?? null);

function checkoutRefOf(meta: Record<string, string | number | boolean>): string | null {
  const v = meta.convt_checkout;
  return typeof v === "string" && /^chk_[0-9a-z]{26}$/.test(v) ? v : null;
}

export function subscriptionFact(
  catalog: Catalog,
  s: z.infer<typeof orderSubscription> & {
    pending_update?: z.infer<typeof subscription>["pending_update"];
  },
  cust: z.infer<typeof customer>,
): SubscriptionFact {
  const normalized = {
    providerSubscriptionId: s.id,
    providerCustomerId: s.customer_id,
    providerCheckoutId: s.checkout_id,
    checkoutRef: checkoutRefOf(s.metadata),
    userId: userIdOf(cust.external_id),
    email: cust.email?.trim().toLowerCase() || null,
    product: productByProviderId(catalog, s.product_id),
    providerProductId: s.product_id,
    status: s.status,
    currency: s.currency.toLowerCase(),
    discountId: s.discount_id,
    trialEndsAt: s.trial_end,
    currentPeriodStart: s.current_period_start,
    currentPeriodEnd: s.current_period_end,
    cancelAtPeriodEnd: s.cancel_at_period_end,
    canceledAt: s.canceled_at,
    endedAt: s.ended_at,
  };
  return {
    kind: "subscription",
    ...normalized,
    // pending_update is not in the hash: a subscription embedded in an order lacks it.
    ...(s.pending_update !== undefined ? { pendingUpdate: s.pending_update } : {}),
    version: versionOf(s),
    hash: hashOf(normalized),
  };
}

export function orderFact(catalog: Catalog, o: z.infer<typeof order>): OrderFact {
  const items: CoverageItem[] = o.items.map((i) => ({
    providerItemId: i.id,
    product: productByPriceId(catalog, i.product_price_id),
    priceId: i.product_price_id,
    periodStart: i.start_timestamp,
    periodEnd: i.end_timestamp,
    amountCents: i.amount,
    proration: i.proration,
  }));
  const normalized = {
    providerOrderId: o.id,
    providerCustomerId: o.customer_id,
    providerCheckoutId: o.checkout_id,
    checkoutRef: checkoutRefOf(o.metadata),
    providerSubscriptionId: o.subscription_id,
    userId: userIdOf(o.customer.external_id),
    email: o.customer.email?.trim().toLowerCase() || null,
    product: productByProviderId(catalog, o.product_id),
    providerProductId: o.product_id,
    reason: o.billing_reason,
    status: o.status,
    subtotalCents: o.subtotal_amount,
    discountCents: o.discount_amount,
    discountId: o.discount_id,
    netCents: o.net_amount,
    appliedBalanceCents: o.applied_balance_amount,
    refundedCents: o.refunded_amount,
    currency: o.currency.toLowerCase(),
    billedAt: o.created_at,
    description: o.description,
    items,
  };
  return {
    kind: "order",
    ...normalized,
    subscription: o.subscription ? subscriptionFact(catalog, o.subscription, o.customer) : null,
    version: versionOf(o),
    hash: hashOf(normalized),
  };
}

export function disputeFact(d: z.infer<typeof disputeBase>): DisputeFact {
  const normalized = {
    providerDisputeId: d.id,
    providerOrderId: d.order_id,
    status: d.status,
    amountCents: d.amount,
    closed: d.closed,
  };
  return { kind: "dispute", ...normalized, version: versionOf(d), hash: hashOf(normalized) };
}

export function checkoutFact(c: z.infer<typeof checkout>): CheckoutFact {
  const normalized = {
    providerCheckoutId: c.id,
    checkoutRef: checkoutRefOf(c.metadata),
    status: c.status,
    providerSubscriptionId: c.subscription_id,
  };
  return { kind: "checkout", ...normalized, version: versionOf(c), hash: hashOf(normalized) };
}

function customerFacts(c: z.infer<typeof customer>, facts: ProviderFacts) {
  if (!c.external_id) return;
  facts.customers.push({
    kind: "customer",
    providerCustomerId: c.id,
    userId: userIdOf(c.external_id),
    email: c.email?.trim().toLowerCase() || null,
    deleted: c.deleted_at !== null,
    version: versionOf(c),
  });
}

function describeZod(e: z.ZodError): string {
  const first = e.issues[0];
  return `${first?.path.join(".") || "body"}: ${first?.code ?? "invalid"}`;
}

/** Strict UTF-8 is already done by the verifier; this parses, validates and normalizes. */
export function parsePolarEvent(catalog: Catalog, body: string): ParsedEvent {
  let raw: unknown;
  try {
    raw = JSON.parse(body);
  } catch {
    return { ok: false, type: "unknown", reason: "malformed: not JSON" };
  }
  const env = envelope.safeParse(raw);
  if (!env.success)
    return { ok: false, type: "unknown", reason: `malformed: ${describeZod(env.error)}` };
  const type = env.data.type;
  const facts = emptyFacts();
  const data = env.data.data;
  const parse = <T extends z.ZodType>(schema: T): z.infer<T> | string => {
    const r = schema.safeParse(data);
    return r.success ? r.data : `malformed: ${describeZod(r.error)}`;
  };
  if (!knownEventTypes.has(type)) return { ok: true, type, facts, ignored: true };

  if (type.startsWith("checkout.")) {
    const c = parse(checkout);
    if (typeof c === "string") return { ok: false, type, reason: c };
    facts.checkouts.push(checkoutFact(c));
    return { ok: true, type, facts, ignored: false };
  }
  if (type.startsWith("order.")) {
    const o = parse(order);
    if (typeof o === "string") return { ok: false, type, reason: o };
    customerFacts(o.customer, facts);
    facts.orders.push(orderFact(catalog, o));
    return { ok: true, type, facts, ignored: false };
  }
  if (type.startsWith("subscription.")) {
    const s = parse(subscription);
    if (typeof s === "string") return { ok: false, type, reason: s };
    customerFacts(s.customer, facts);
    facts.subscriptions.push(subscriptionFact(catalog, s, s.customer));
    if (type === "subscription.paused" || type === "subscription.migrated")
      facts.alerts.push({
        kind: type.replace(".", "_"),
        subject: s.id,
        detail: `Polar sent ${type}; convt never offers this`,
      });
    return { ok: true, type, facts, ignored: false };
  }
  if (type.startsWith("refund.")) {
    const r = parse(refund);
    if (typeof r === "string") return { ok: false, type, reason: r };
    facts.hints.push({ kind: "order", id: r.order_id });
    if (r.dispute) facts.disputes.push(disputeFact(r.dispute));
    return { ok: true, type, facts, ignored: false };
  }
  if (type === "customer.created" || type === "customer.updated" || type === "customer.deleted") {
    const c = parse(customer);
    if (typeof c === "string") return { ok: false, type, reason: c };
    customerFacts(c, facts);
    if (type === "customer.deleted")
      facts.alerts.push({
        kind: "customer_deleted",
        subject: c.id,
        detail: "a Polar customer was deleted",
      });
    return {
      ok: true,
      type,
      facts,
      ignored: facts.customers.length === 0 && facts.alerts.length === 0,
    };
  }
  if (type === "organization.updated") {
    facts.hints.push({ kind: "settings" });
    return { ok: true, type, facts, ignored: false };
  }
  return { ok: true, type, facts, ignored: true };
}

// ------------------------------------------------------------------ adapter

export type PolarOptions = {
  accessToken: string;
  /** Polar's API, or the billing mock locally. */
  baseUrl: string;
  webhookSecret: string;
  catalog: Catalog;
  /** The only origin a portal URL may have. */
  portalOrigin: string;
  timeoutSeconds?: number;
};

function statusOf(e: unknown): number | null {
  const s = (e as { statusCode?: unknown })?.statusCode;
  return typeof s === "number" ? s : null;
}

function wrap(op: string, e: unknown): never {
  if (e instanceof ProviderError) throw e;
  const status = statusOf(e);
  const name = (e as Error)?.name ?? "Error";
  throw new ProviderError(`polar ${op} failed: ${status ?? name}`, status, name);
}

export function createPolarProvider(options: PolarOptions): BillingProvider {
  const polar = createPolar({
    accessToken: options.accessToken,
    baseUrl: options.baseUrl.replace(/\/$/, ""),
    timeout: options.timeoutSeconds ?? 4,
  });
  const { catalog } = options;
  const call = async <T>(op: string, fn: () => Promise<T>): Promise<T> => {
    try {
      return await fn();
    } catch (e) {
      wrap(op, e);
    }
  };
  const strict = <T extends z.ZodType>(op: string, schema: T, value: unknown): z.infer<T> => {
    const r = schema.safeParse(value);
    if (!r.success)
      throw new ProviderError(`polar ${op}: ${describeZod(r.error)}`, null, "malformed");
    return r.data;
  };

  return {
    name: "polar",
    verifyWebhook: (raw, headers, now) => verifyWebhook(raw, headers, options.webhookSecret, now),
    parseEvent: (delivery) => parsePolarEvent(catalog, delivery.body),

    async createCheckout(input) {
      const entry = catalog.products[input.product];
      const co = await call("checkouts.create", () =>
        polar.checkouts.create({
          products: [entry.productId],
          allow_trial: input.allowTrial,
          external_customer_id: input.externalCustomerId,
          customer_email: input.email,
          metadata: { convt_checkout: input.checkoutRef },
          success_url: input.successUrl,
          currency: "usd",
        }),
      );
      return { providerCheckoutId: co.id, url: co.url };
    },
    async getCheckout(id) {
      return checkoutFact(
        strict(
          "checkouts.get",
          checkout,
          await call("checkouts.get", () => polar.checkouts.get(id)),
        ),
      );
    },
    async checkoutOrders(providerCheckoutId) {
      const list = await call("orders.list", () =>
        polar.orders.list({ checkout_id: providerCheckoutId, limit: 100, sorting: ["created_at"] }),
      );
      return list.items.map((o) => orderFact(catalog, strict("orders.list", order, o)));
    },
    async getOrder(id) {
      return orderFact(
        catalog,
        strict("orders.get", order, await call("orders.get", () => polar.orders.get(id))),
      );
    },
    async getSubscription(id) {
      const s = strict(
        "subscriptions.get",
        subscription,
        await call("subscriptions.get", () => polar.subscriptions.get(id)),
      );
      return subscriptionFact(catalog, s, s.customer);
    },
    async getDispute(id) {
      return disputeFact(
        strict(
          "disputes.get",
          disputeBase,
          await call("disputes.get", () => polar.disputes.get(id)),
        ),
      );
    },
    async scan(kind: ScanKind, from) {
      const facts = emptyFacts();
      const limit = from.limit ?? 100;
      if (kind === "orders") {
        const r = await call("orders.list", () =>
          polar.orders.list({
            page: from.page,
            limit,
            sorting: ["created_at"],
            created_after: from.createdAfter ?? null,
          }),
        );
        for (const o of r.items) {
          const parsed = strict("orders.list", order, o);
          customerFacts(parsed.customer, facts);
          facts.orders.push(orderFact(catalog, parsed));
        }
        return { facts, page: from.page, maxPage: r.pagination.max_page };
      }
      if (kind === "subscriptions") {
        const r = await call("subscriptions.list", () =>
          polar.subscriptions.list({ page: from.page, limit, sorting: ["started_at"] }),
        );
        for (const s of r.items) {
          const parsed = strict("subscriptions.list", subscription, s);
          facts.subscriptions.push(subscriptionFact(catalog, parsed, parsed.customer));
        }
        return { facts, page: from.page, maxPage: r.pagination.max_page };
      }
      if (kind === "refunds") {
        const r = await call("refunds.list", () =>
          polar.refunds.list({ page: from.page, limit, sorting: ["created_at"] }),
        );
        for (const x of r.items) {
          const parsed = strict("refunds.list", refund, x);
          facts.hints.push({ kind: "order", id: parsed.order_id });
          if (parsed.dispute) facts.disputes.push(disputeFact(parsed.dispute));
        }
        return { facts, page: from.page, maxPage: r.pagination.max_page };
      }
      const r = await call("disputes.list", () =>
        polar.disputes.list({ page: from.page, limit, sorting: ["created_at"] }),
      );
      for (const d of r.items)
        facts.disputes.push(disputeFact(strict("disputes.list", disputeBase, d)));
      return { facts, page: from.page, maxPage: r.pagination.max_page };
    },
    async paymentMethods(userId) {
      try {
        const r = await polar.customers.listPaymentMethodsExternal(userId, { limit: 10 });
        const cards: CardFact[] = [];
        for (const m of r.items) {
          if (m.type !== "card") continue;
          const meta = (m as models.CustomerPaymentMethodCard).method_metadata;
          cards.push({
            brand: meta.brand,
            last4: meta.last4,
            expMonth: meta.exp_month,
            expYear: meta.exp_year,
          });
        }
        return cards;
      } catch (e) {
        if (statusOf(e) === 404) return [];
        wrap("customers.listPaymentMethodsExternal", e);
      }
    },
    async customerSubscriptions(userId) {
      const r = await call("subscriptions.list", () =>
        polar.subscriptions.list({ external_customer_id: userId, limit: 100 }),
      );
      return r.items.map((x) => {
        const parsed = strict("subscriptions.list", subscription, x);
        return subscriptionFact(catalog, parsed, parsed.customer);
      });
    },
    async openCheckouts(userId) {
      const r = await call("checkouts.list", () =>
        polar.checkouts.list({ external_customer_id: userId, status: ["open"], limit: 100 }),
      );
      return r.pagination.total_count;
    },
    async changeProduct(subscriptionId, product, proration) {
      try {
        const s = await polar.subscriptions.update(subscriptionId, {
          product_id: catalog.products[product].productId,
          proration_behavior: proration,
        });
        const parsed = strict("subscriptions.update", subscription, s);
        return subscriptionFact(catalog, parsed, parsed.customer);
      } catch (e) {
        // Polar applies the change only if the immediate payment succeeds.
        if (statusOf(e) === 402) return { paymentFailed: true, detail: "payment failed" };
        wrap("subscriptions.update", e);
      }
    },
    async setCancelAtPeriodEnd(subscriptionId, value) {
      const s = await call("subscriptions.update", () =>
        polar.subscriptions.update(subscriptionId, { cancel_at_period_end: value }),
      );
      const parsed = strict("subscriptions.update", subscription, s);
      return subscriptionFact(catalog, parsed, parsed.customer);
    },
    async revokeSubscription(subscriptionId) {
      try {
        const s = strict(
          "subscriptions.revoke",
          subscription,
          await polar.subscriptions.revoke(subscriptionId),
        );
        return subscriptionFact(catalog, s, s.customer);
      } catch (e) {
        // Already ended: success, as long as a fetch confirms it.
        if (statusOf(e) === 403) return this.getSubscription(subscriptionId);
        wrap("subscriptions.revoke", e);
      }
    },
    async portalUrl(userId, returnUrl) {
      const session = await call("customerSessions.create", () =>
        polar.customerSessions.create({ external_customer_id: userId, return_url: returnUrl }),
      );
      const url = new URL(session.customer_portal_url);
      if (url.origin !== new URL(options.portalOrigin).origin)
        throw new ProviderError("the portal URL has an unexpected origin", null, "portal_origin");
      return url.toString();
    },
    async receiptUrl(providerOrderId) {
      return (await call("orders.invoice", () => polar.orders.invoice(providerOrderId))).url;
    },
    async settings() {
      const r = await call("organizations.list", () => polar.organizations.list({ limit: 1 }));
      const org = strict("organizations.list", organization, r.items[0]);
      return {
        allowMultipleSubscriptions: org.subscription_settings.allow_multiple_subscriptions,
        preventTrialAbuse: org.subscription_settings.prevent_trial_abuse,
        trialConversionEmail: org.customer_email_settings.subscription_trial_conversion_reminder,
        pastDueEmail: org.customer_email_settings.subscription_past_due,
      };
    },
    async products() {
      const r = await call("products.list", () => polar.products.list({ limit: 100 }));
      return r.items.flatMap((p) =>
        p.prices.map((price) => {
          const x = price as {
            id: string;
            amount_type?: string;
            price_amount?: number;
            unit_amount?: string;
            price_currency?: string;
            is_archived: boolean;
          };
          return {
            productId: p.id,
            priceId: x.id,
            amountType: x.amount_type ?? "legacy",
            amountCents: typeof x.price_amount === "number" ? x.price_amount : null,
            unitAmount: typeof x.unit_amount === "string" ? x.unit_amount : null,
            currency: x.price_currency ?? "usd",
            interval: p.recurring_interval,
            archived: x.is_archived || p.is_archived,
          };
        }),
      );
    },
  };
}

export type { CatalogProduct };
