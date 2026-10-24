// A local stand-in for the parts of Polar (API version 2026-10) and Resend that
// convt-billing calls. Development and tests only; main.ts refuses to bind
// anything but loopback. Payloads are built as the SDK's own model types, so the
// compiler checks their shape, and the tests parse every one of them.
//
// Polar API:  /v1/checkouts, /v1/orders (+ /invoice), /v1/subscriptions (update,
//             revoke), /v1/customers/external/{id}/payment-methods,
//             /v1/customer-sessions, /v1/refunds, /v1/disputes, /v1/organizations,
//             /v1/products, with page-number pagination.
// Pages:      /checkout/{client_secret} (Pay, Decline, Abandon), /portal/{token}
// Resend:     POST /emails
// Sequenzy:   POST /api/v1/subscribers, PATCH and DELETE /api/v1/subscribers/external
// Admin:      /admin/* moves the clock, ends trials, renews, fails renewals,
//             refunds, opens and closes disputes, changes settings, and controls
//             webhook delivery (duplicate, delay, reorder, drop, forge, hold).

import { randomBytes, randomUUID } from "node:crypto";

import type { models } from "@polar-sh/sdk/2026-10";

import { abandonedPage, checkoutPage, failedPage, portalPage } from "./pages";
import { createResend, type ResendFault } from "./resend";
import { createSequenzy } from "./sequenzy";
import { deliveryHeaders, type Scheme } from "./sign";

export type ProductKey = "desktop" | "pro_month" | "pro_year" | "api";

export type ProductDef = {
  productId: string;
  priceId: string;
  name: string;
  /** Fixed price in cents; null for the metered API product. */
  amount: number | null;
  /** Metered unit price, a decimal string of cents as Polar sends it. */
  unitAmount?: string;
  interval: "month" | "year" | null;
  trialDays: number | null;
};

/** The organization's products in the `local` environment (packages/billing/src/catalog.ts). */
export const localProducts: Record<ProductKey, ProductDef> = {
  desktop: {
    productId: "prod_local_desktop",
    priceId: "price_local_desktop",
    name: "convt Desktop",
    amount: 2900,
    interval: null,
    trialDays: null,
  },
  pro_month: {
    productId: "prod_local_pro_month",
    priceId: "price_local_pro_month",
    name: "convt Pro (monthly)",
    amount: 1200,
    interval: "month",
    trialDays: 7,
  },
  pro_year: {
    productId: "prod_local_pro_year",
    priceId: "price_local_pro_year",
    name: "convt Pro (yearly)",
    amount: 9600,
    interval: "year",
    trialDays: 7,
  },
  api: {
    productId: "prod_local_api",
    priceId: "price_local_api",
    name: "convt API",
    amount: null,
    unitAmount: "1",
    interval: "month",
    trialDays: null,
  },
};

const ORG_ID = "org_local_convt";
const METER_ID = "meter_local_api_conversion";
const dayMs = 86_400_000;

type Card = {
  id: string;
  brand: string;
  last4: string;
  expMonth: number;
  expYear: number;
  createdAt: string;
};
type Cust = {
  id: string;
  createdAt: string;
  modifiedAt: string | null;
  email: string;
  externalId: string | null;
  cards: Card[];
  balance: number;
  decline: boolean;
  hadTrial: boolean;
  deletedAt: string | null;
};
type Co = {
  id: string;
  createdAt: string;
  modifiedAt: string | null;
  status: models.CheckoutStatus;
  clientSecret: string;
  product: ProductKey;
  allowTrial: boolean;
  externalCustomerId: string | null;
  customerEmail: string | null;
  metadata: Record<string, string | number | boolean>;
  successUrl: string;
  returnUrl: string | null;
  expiresAt: string;
  customerId: string | null;
  subscriptionId: string | null;
  message: string | null;
};
type Item = {
  id: string;
  createdAt: string;
  label: string;
  amount: number;
  proration: boolean;
  priceId: string | null;
  start: string | null;
  end: string | null;
};
type Ord = {
  id: string;
  createdAt: string;
  modifiedAt: string | null;
  status: models.OrderStatus;
  customerId: string;
  product: ProductKey;
  reason: models.OrderBillingReason;
  subscriptionId: string | null;
  checkoutId: string | null;
  items: Item[];
  subtotal: number;
  applied: number;
  refunded: number;
  currency: string;
  discountId: string | null;
  discountAmount: number;
  metadata: Record<string, string | number | boolean>;
};
type Sub = {
  id: string;
  /** Set only by tests through mutateQuietly; the flows never apply a discount. */
  discountId?: string | null;
  createdAt: string;
  modifiedAt: string | null;
  product: ProductKey;
  status: models.SubscriptionStatus;
  periodStart: string;
  periodEnd: string;
  trialStart: string | null;
  trialEnd: string | null;
  cancelAtPeriodEnd: boolean;
  canceledAt: string | null;
  startedAt: string | null;
  endsAt: string | null;
  endedAt: string | null;
  pastDueAt: string | null;
  customerId: string;
  checkoutId: string | null;
  metadata: Record<string, string | number | boolean>;
  pending: { id: string; createdAt: string; appliesAt: string; product: ProductKey } | null;
};
type Refund = {
  id: string;
  createdAt: string;
  modifiedAt: string | null;
  amount: number;
  orderId: string;
  subscriptionId: string | null;
  customerId: string;
  reason: models.RefundReason;
  disputeId: string | null;
};
type Dispute = {
  id: string;
  createdAt: string;
  modifiedAt: string | null;
  status: models.DisputeStatus;
  amount: number;
  orderId: string;
};

export type HeldDelivery = { id: string; type: string; body: string };

export type WebhookConfig = {
  url: string | null;
  secret: string;
  scheme: Scheme;
  /** `auto` delivers right away (with retries); `hold` keeps events until a test takes them. */
  mode: "auto" | "hold";
  dropNext: number;
  duplicate: number;
  delayMs: number;
  reorder: boolean;
  forgeNext: number;
  retryBaseMs: number;
};

export type MockOptions = {
  /** Where the mock listens; links in API answers use publicUrl. */
  publicUrl: string;
  accessToken: string;
  resendApiKey: string;
  /** The marketing key convt-billing sends to the Sequenzy routes. */
  sequenzyApiKey?: string;
  webhook: Partial<WebhookConfig> & { secret: string };
  mailpitUrl?: string;
  /** Replaces POSTing deliveries to webhook.url (in-process tests). */
  sink?: (body: string, headers: Record<string, string>) => Promise<number>;
  startMs?: number;
};

class HttpError extends Error {
  constructor(
    readonly status: number,
    readonly body: unknown,
  ) {
    super(`http ${status}`);
  }
}

const json = (status: number, body: unknown) =>
  new Response(JSON.stringify(body), { status, headers: { "content-type": "application/json" } });

export function createBillingMock(options: MockOptions) {
  const publicUrl = options.publicUrl.replace(/\/$/, "");
  const products: Record<ProductKey, ProductDef> = structuredClone(localProducts);
  let offsetMs = options.startMs !== undefined ? options.startMs - Date.now() : 0;
  const nowMs = () => Date.now() + offsetMs;
  let lastMicros = 0;

  /** A strictly increasing timestamp with microseconds, like Polar's. */
  function stamp(): string {
    const m = Math.max(nowMs() * 1000, lastMicros + 1);
    lastMicros = m;
    return fmt(m);
  }
  function fmt(micros: number): string {
    const ms = Math.floor(micros / 1000);
    const base = new Date(ms).toISOString().slice(0, 19);
    const frac = String(micros % 1_000_000).padStart(6, "0");
    return `${base}.${frac}Z`;
  }
  const atMs = (ms: number) => fmt(ms * 1000);
  const ms = (iso: string) => Date.parse(iso);
  const addInterval = (iso: string, interval: "month" | "year", n = 1) => {
    const d = new Date(ms(iso));
    if (interval === "month") d.setUTCMonth(d.getUTCMonth() + n);
    else d.setUTCFullYear(d.getUTCFullYear() + n);
    return atMs(d.getTime());
  };

  const settings = {
    allowMultipleSubscriptions: true,
    preventTrialAbuse: true,
    prorationBehavior: "invoice" as models.PublicSubscriptionProrationBehavior,
    emails: {
      subscription_trial_conversion_reminder: false,
      subscription_past_due: false,
    } as Partial<models.OrganizationCustomerEmailSettings>,
    orgModifiedAt: null as string | null,
  };

  const customers = new Map<string, Cust>();
  const checkouts = new Map<string, Co>();
  const orders = new Map<string, Ord>();
  const subs = new Map<string, Sub>();
  const refunds = new Map<string, Refund>();
  const disputes = new Map<string, Dispute>();
  const sessions = new Map<string, { customerId: string; returnUrl: string | null }>();

  const sequenzy = createSequenzy({ apiKey: options.sequenzyApiKey ?? "sqz_test" });
  const resend = createResend({
    apiKey: options.resendApiKey,
    now: nowMs,
    mailpitUrl: options.mailpitUrl,
  });

  // ---------------------------------------------------------------- webhooks

  const webhook: WebhookConfig = {
    url: null,
    scheme: "standard",
    mode: "auto",
    dropNext: 0,
    duplicate: 0,
    delayMs: 0,
    reorder: false,
    forgeNext: 0,
    retryBaseMs: 1000,
    ...options.webhook,
  };
  const held: HeldDelivery[] = [];
  const log: Array<{
    id: string;
    type: string;
    status: number | "dropped" | "error";
    attempt: number;
  }> = [];
  const all: HeldDelivery[] = [];
  const inflight = new Set<Promise<void>>();

  function emit(type: string, data: unknown) {
    const body = JSON.stringify({ type, timestamp: stamp(), api_version: "2026-10", data });
    const d = { id: `msg_${randomBytes(12).toString("hex")}`, type, body };
    all.push(d);
    if (webhook.mode === "hold" || webhook.reorder) {
      held.push(d);
      return;
    }
    schedule(d);
  }

  function schedule(d: HeldDelivery) {
    if (webhook.dropNext > 0) {
      webhook.dropNext--;
      log.push({ id: d.id, type: d.type, status: "dropped", attempt: 0 });
      return;
    }
    const copies = 1 + webhook.duplicate;
    for (let i = 0; i < copies; i++) {
      const p = (async () => {
        if (webhook.delayMs) await Bun.sleep(webhook.delayMs);
        await deliverWithRetries(d);
      })();
      inflight.add(p);
      p.finally(() => inflight.delete(p));
    }
  }

  async function sendOnce(d: HeldDelivery, attempt: number): Promise<number> {
    let secret = webhook.secret;
    if (webhook.forgeNext > 0) {
      webhook.forgeNext--;
      secret = `whsec_${randomBytes(24).toString("base64")}`;
    }
    // Signed with wall time, as Polar does: the mock clock only moves business time.
    const headers = deliveryHeaders(
      secret,
      webhook.scheme,
      d.id,
      Math.floor(Date.now() / 1000),
      d.body,
    );
    let status: number;
    try {
      if (options.sink) status = await options.sink(d.body, headers);
      else if (webhook.url) {
        const res = await fetch(webhook.url, {
          method: "POST",
          headers,
          body: d.body,
          redirect: "manual",
          signal: AbortSignal.timeout(10_000),
        });
        status = res.status;
      } else status = 0;
    } catch {
      log.push({ id: d.id, type: d.type, status: "error", attempt });
      return 0;
    }
    log.push({ id: d.id, type: d.type, status, attempt });
    return status;
  }

  async function deliverWithRetries(d: HeldDelivery) {
    // Polar retries up to 10 times with exponential backoff.
    for (let attempt = 0; attempt <= 10; attempt++) {
      const status = await sendOnce(d, attempt);
      if (status >= 200 && status < 300) return;
      if (!webhook.url && !options.sink) return;
      await Bun.sleep(webhook.retryBaseMs * 2 ** attempt);
    }
  }

  /** Delivers held events; `order` may permute them. */
  async function flush(order?: (events: HeldDelivery[]) => HeldDelivery[]) {
    const events = held.splice(0);
    const list = order ? order(events) : webhook.reorder ? shuffle(events) : events;
    for (const d of list) schedule(d);
    await settle();
  }

  async function settle() {
    while (inflight.size) await Promise.allSettled(inflight);
  }

  function shuffle<T>(xs: T[]): T[] {
    const a = [...xs];
    for (let i = a.length - 1; i > 0; i--) {
      const j = Math.floor(Math.random() * (i + 1));
      [a[i], a[j]] = [a[j], a[i]];
    }
    return a;
  }

  // ---------------------------------------------------------------- rendering

  const meter: models.Meter = {
    metadata: {},
    created_at: "2026-09-01T00:00:00.000000Z",
    modified_at: null,
    id: METER_ID,
    name: "api_conversion",
    unit: "scalar",
    filter: { conjunction: "and", clauses: [] },
    aggregation: { func: "count" },
    organization_id: ORG_ID,
  };

  function priceOf(key: ProductKey): models.ProductPrice {
    const p = products[key];
    const base = {
      created_at: "2026-09-01T00:00:00.000000Z",
      modified_at: null,
      id: p.priceId,
      source: "catalog" as const,
      price_currency: "usd",
      tax_behavior: null,
      is_archived: false,
      product_id: p.productId,
    };
    if (p.amount === null) {
      return {
        ...base,
        amount_type: "metered_unit",
        cap_amount: null,
        meter_id: METER_ID,
        meter: {
          id: METER_ID,
          name: "api_conversion",
          unit: "scalar",
          custom_label: null,
          custom_multiplier: null,
        },
        unit_amount: p.unitAmount ?? "1",
      };
    }
    return { ...base, amount_type: "fixed", price_amount: p.amount };
  }

  function productBase(key: ProductKey) {
    const p = products[key];
    return {
      id: p.productId,
      created_at: "2026-09-01T00:00:00.000000Z",
      modified_at: null,
      trial_interval: p.trialDays ? ("day" as const) : null,
      trial_interval_count: p.trialDays,
      name: p.name,
      description: null,
      visibility: "public" as const,
      recurring_interval: p.interval,
      recurring_interval_count: p.interval ? 1 : null,
      meter_interval: null,
      meter_interval_count: null,
      is_recurring: p.interval !== null,
      is_archived: false,
      organization_id: ORG_ID,
    };
  }

  function renderProduct(key: ProductKey): models.Product {
    return {
      ...productBase(key),
      metadata: {},
      is_deletable: false,
      prices: [priceOf(key)],
      benefits: [],
      medias: [],
      attached_custom_fields: [],
    };
  }

  function renderCustomer(c: Cust): models.CustomerIndividual {
    return {
      id: c.id,
      created_at: c.createdAt,
      modified_at: c.modifiedAt,
      metadata: {},
      external_id: c.externalId,
      email: c.email,
      email_verified: false,
      type: "individual",
      name: null,
      billing_name: null,
      billing_address: null,
      tax_id: null,
      locale: null,
      organization_id: ORG_ID,
      default_payment_method_id: c.cards[0]?.id ?? null,
      deleted_at: c.deletedAt,
      first_user_event_at: null,
      avatar_url: null,
    };
  }

  function subBase(s: Sub) {
    const p = products[s.product];
    return {
      created_at: s.createdAt,
      modified_at: s.modifiedAt,
      id: s.id,
      amount: p.amount ?? 0,
      currency: "usd",
      recurring_interval: (p.interval ?? "month") as models.RecurringInterval,
      recurring_interval_count: 1,
      status: s.status,
      current_period_start: s.periodStart,
      current_period_end: s.periodEnd,
      current_meter_period_start: p.amount === null ? s.periodStart : null,
      current_meter_period_end: p.amount === null ? s.periodEnd : null,
      trial_start: s.trialStart,
      trial_end: s.trialEnd,
      cancel_at_period_end: s.cancelAtPeriodEnd,
      canceled_at: s.canceledAt,
      started_at: s.startedAt,
      ends_at: s.endsAt,
      ended_at: s.endedAt,
      past_due_at: s.pastDueAt,
      pause_at_period_end: false,
      paused_at: null,
      resumes_at: null,
      customer_id: s.customerId,
      product_id: p.productId,
      discount_id: s.discountId ?? null,
      checkout_id: s.checkoutId,
      seats: null,
      units: null,
      customer_cancellation_reason: null,
      customer_cancellation_comment: null,
      metadata: s.metadata,
    };
  }

  function renderSub(s: Sub): models.Subscription {
    const c = customers.get(s.customerId)!;
    return {
      ...subBase(s),
      custom_field_data: {},
      customer: renderCustomer(c),
      product: renderProduct(s.product),
      discount: null,
      prices: [priceOf(s.product)],
      meters:
        products[s.product].amount === null
          ? [
              {
                created_at: s.createdAt,
                modified_at: null,
                id: `sm_${s.id}`,
                consumed_units: 0,
                credited_units: 0,
                amount: 0,
                meter_id: METER_ID,
                meter,
              },
            ]
          : [],
      pending_update: s.pending
        ? {
            created_at: s.pending.createdAt,
            modified_at: null,
            id: s.pending.id,
            applies_at: s.pending.appliesAt,
            product_id: products[s.pending.product].productId,
            seats: null,
            units: null,
          }
        : null,
    };
  }

  function orderAmounts(o: Ord) {
    const net = o.subtotal - o.discountAmount;
    return { net, total: net, due: Math.max(0, net - o.applied) };
  }

  function renderOrder(o: Ord): models.Order {
    const c = customers.get(o.customerId)!;
    const s = o.subscriptionId ? subs.get(o.subscriptionId) : null;
    const { net, total, due } = orderAmounts(o);
    const paid =
      o.status === "paid" || o.status === "partially_refunded" || o.status === "refunded";
    return {
      id: o.id,
      created_at: o.createdAt,
      modified_at: o.modifiedAt,
      status: o.status,
      paid,
      subtotal_amount: o.subtotal,
      discount_amount: o.discountAmount,
      net_amount: net,
      tax_amount: 0,
      total_amount: total,
      applied_balance_amount: o.applied,
      due_amount: paid ? 0 : due,
      refunded_amount: o.refunded,
      refunded_tax_amount: 0,
      currency: o.currency,
      billing_reason: o.reason,
      billing_name: null,
      billing_address: null,
      invoice_number: `MOCK-${o.id.slice(0, 8).toUpperCase()}`,
      is_invoice_generated: paid,
      receipt_number: paid ? `R-${o.id.slice(0, 8)}` : null,
      seats: null,
      units: null,
      customer_id: o.customerId,
      product_id: products[o.product].productId,
      discount_id: o.discountId,
      subscription_id: o.subscriptionId,
      checkout_id: o.checkoutId,
      next_payment_attempt_at: null,
      metadata: o.metadata,
      custom_field_data: {},
      platform_fee_amount: 0,
      platform_fee_currency: null,
      customer: renderCustomer(c),
      product: { ...productBase(o.product), metadata: {} },
      discount: null,
      subscription: s ? subBase(s) : null,
      items: o.items.map((i) => ({
        created_at: i.createdAt,
        modified_at: null,
        id: i.id,
        label: i.label,
        amount: i.amount,
        tax_amount: 0,
        proration: i.proration,
        product_price_id: i.priceId,
        start_timestamp: i.start,
        end_timestamp: i.end,
      })),
      description: products[o.product].name,
      refundable_amount: Math.max(0, total - o.refunded),
      refundable_tax_amount: 0,
    };
  }

  function renderCheckout(co: Co): models.Checkout {
    const p = products[co.product];
    const trial = co.allowTrial && p.trialDays !== null;
    return {
      id: co.id,
      created_at: co.createdAt,
      modified_at: co.modifiedAt,
      custom_field_data: {},
      payment_processor: "stripe",
      status: co.status,
      client_secret: co.clientSecret,
      url: `${publicUrl}/checkout/${co.clientSecret}`,
      expires_at: co.expiresAt,
      success_url: co.successUrl,
      return_url: co.returnUrl,
      embed_origin: null,
      amount: p.amount ?? 0,
      seats: null,
      min_seats: null,
      max_seats: null,
      units: null,
      min_units: null,
      max_units: null,
      discount_amount: 0,
      net_amount: p.amount ?? 0,
      tax_amount: null,
      tax_behavior: null,
      total_amount: p.amount ?? 0,
      currency: "usd",
      allow_trial: co.allowTrial,
      active_trial_interval: trial ? "day" : null,
      active_trial_interval_count: trial ? p.trialDays : null,
      trial_end: trial ? atMs(nowMs() + (p.trialDays ?? 0) * dayMs) : null,
      organization_id: ORG_ID,
      product_id: p.productId,
      product_price_id: p.priceId,
      discount_id: null,
      allow_discount_codes: false,
      require_billing_address: false,
      is_discount_applicable: false,
      is_free_product_price: false,
      is_payment_required: p.amount !== null && !trial,
      is_payment_setup_required: p.interval !== null,
      is_payment_form_required: true,
      customer_id: co.customerId,
      is_business_customer: false,
      customer_name: null,
      customer_email: co.customerEmail,
      customer_ip_address: null,
      customer_billing_name: null,
      customer_billing_address: null,
      customer_tax_id: null,
      locale: null,
      payment_method_type: null,
      payment_processor_metadata: {},
      billing_address_fields: {
        country: "disabled",
        state: "disabled",
        city: "disabled",
        postal_code: "disabled",
        line1: "disabled",
        line2: "disabled",
      },
      trial_interval: null,
      trial_interval_count: null,
      metadata: co.metadata,
      external_customer_id: co.externalCustomerId,
      products: [
        { ...productBase(co.product), prices: [priceOf(co.product)], benefits: [], medias: [] },
      ],
      product: {
        ...productBase(co.product),
        prices: [priceOf(co.product)],
        benefits: [],
        medias: [],
      },
      product_price: priceOf(co.product),
      prices: { [p.productId]: [priceOf(co.product)] },
      discount: null,
      subscription_id: co.subscriptionId,
      attached_custom_fields: [],
      customer_metadata: {},
    };
  }

  function renderRefund(r: Refund): models.Refund {
    const d = r.disputeId ? disputes.get(r.disputeId)! : null;
    return {
      created_at: r.createdAt,
      modified_at: r.modifiedAt,
      id: r.id,
      metadata: {},
      status: "succeeded",
      reason: r.reason,
      amount: r.amount,
      tax_amount: 0,
      currency: "usd",
      organization_id: ORG_ID,
      order_id: r.orderId,
      subscription_id: r.subscriptionId,
      customer_id: r.customerId,
      revoke_benefits: false,
      dispute: d ? disputeBase(d) : null,
    };
  }

  function disputeBase(d: Dispute) {
    return {
      created_at: d.createdAt,
      modified_at: d.modifiedAt,
      id: d.id,
      status: d.status,
      resolved: d.status === "lost" || d.status === "won" || d.status === "prevented",
      closed: d.status === "lost" || d.status === "won" || d.status === "prevented",
      amount: d.amount,
      tax_amount: 0,
      currency: "usd",
      reason: "fraudulent",
      evidence_due_by: null,
      past_due: false,
      order_id: d.orderId,
      payment_id: `pay_${d.orderId}`,
    };
  }

  function renderDispute(d: Dispute): models.Dispute {
    const o = orders.get(d.orderId)!;
    return {
      ...disputeBase(d),
      customer: renderCustomer(customers.get(o.customerId)!),
      case_id: null,
    };
  }

  function renderOrg(): models.Organization {
    return {
      created_at: "2026-09-01T00:00:00.000000Z",
      modified_at: settings.orgModifiedAt,
      id: ORG_ID,
      name: "convt (local)",
      slug: "convt-local",
      avatar_url: null,
      proration_behavior: settings.prorationBehavior,
      allow_customer_updates: true,
      email: null,
      website: null,
      socials: [],
      status: "active",
      details_submitted_at: null,
      onboarding_resubmission_requested_at: null,
      sso_enforced: false,
      default_presentment_currency: "usd",
      default_tax_behavior: "exclusive",
      feature_settings: null,
      subscription_settings: {
        allow_multiple_subscriptions: settings.allowMultipleSubscriptions,
        proration_behavior: settings.prorationBehavior,
        benefit_revocation_grace_period: 0,
        prevent_trial_abuse: settings.preventTrialAbuse,
        allow_customer_updates: true,
      },
      customer_email_settings: {
        order_confirmation: true,
        payment_method_expiration_reminder: true,
        subscription_cancellation: true,
        subscription_confirmation: true,
        subscription_cycled: true,
        subscription_cycled_after_trial: true,
        subscription_past_due: false,
        subscription_paused: true,
        subscription_resumed: true,
        subscription_renewal_reminder: true,
        subscription_revoked: true,
        subscription_trial_conversion_reminder: false,
        subscription_uncanceled: true,
        subscription_updated: true,
        ...settings.emails,
      },
      customer_portal_settings: {
        usage: { show: true },
        subscription: { update_seats: false, update_plan: false },
      },
      dispute_settings: { auto_accept_below_amount: null },
      embed_hosts: [],
      account_id: null,
      payout_account_id: null,
      capabilities: {
        checkout_payments: true,
        subscription_renewals: true,
        payouts: false,
        refunds: true,
        api_access: true,
        dashboard_access: true,
      },
    };
  }

  // ---------------------------------------------------------------- state changes

  function touch<T extends { modifiedAt: string | null }>(x: T): T {
    x.modifiedAt = stamp();
    return x;
  }

  function customerFor(externalId: string | null, email: string): Cust {
    let c = externalId
      ? [...customers.values()].find((x) => x.externalId === externalId)
      : undefined;
    // Polar keeps one customer per email in an organization.
    c ??= [...customers.values()].find((x) => x.email === email.toLowerCase() && !x.deletedAt);
    if (c) {
      if (externalId && !c.externalId) {
        c.externalId = externalId;
        touch(c);
        emit("customer.updated", renderCustomer(c));
      }
      return c;
    }
    c = {
      id: randomUUID(),
      createdAt: stamp(),
      modifiedAt: null,
      email: email.toLowerCase(),
      externalId,
      cards: [],
      balance: 0,
      decline: false,
      hadTrial: false,
      deletedAt: null,
    };
    customers.set(c.id, c);
    emit("customer.created", renderCustomer(c));
    return c;
  }

  function addCard(c: Cust, last4: string) {
    c.cards = [
      {
        id: randomUUID(),
        brand: "visa",
        last4,
        expMonth: 12,
        expYear: new Date(nowMs()).getUTCFullYear() + 3,
        createdAt: stamp(),
      },
    ];
  }

  function liveSubs(customerId: string) {
    return [...subs.values()].filter(
      (s) =>
        s.customerId === customerId &&
        ["trialing", "active", "past_due", "incomplete"].includes(s.status),
    );
  }

  function makeOrder(input: {
    customer: Cust;
    product: ProductKey;
    reason: models.OrderBillingReason;
    sub: Sub | null;
    checkoutId: string | null;
    items: Array<Omit<Item, "id" | "createdAt">>;
    metadata?: Record<string, string | number | boolean>;
    /** Whether the card is charged now; false leaves the order pending. */
    charge: boolean;
  }): Ord {
    const created = stamp();
    const subtotal = input.items.reduce((n, i) => n + i.amount, 0);
    let applied = 0;
    if (subtotal > 0 && input.customer.balance > 0) {
      applied = Math.min(input.customer.balance, subtotal);
    }
    const due = Math.max(0, subtotal - applied);
    const declined = due > 0 && (!input.charge || input.customer.decline);
    const o: Ord = {
      id: randomUUID(),
      createdAt: created,
      modifiedAt: null,
      status: declined ? "pending" : "paid",
      customerId: input.customer.id,
      product: input.product,
      reason: input.reason,
      subscriptionId: input.sub?.id ?? null,
      checkoutId: input.checkoutId,
      items: input.items.map((i) => ({ ...i, id: randomUUID(), createdAt: created })),
      subtotal,
      applied: declined ? 0 : applied,
      refunded: 0,
      currency: "usd",
      discountId: null,
      discountAmount: 0,
      metadata: input.metadata ?? {},
    };
    if (!declined) {
      input.customer.balance -= o.applied;
      // A negative total (a downgrade credit) goes to the customer's balance.
      if (subtotal < 0) input.customer.balance += -subtotal;
    }
    orders.set(o.id, o);
    emit("order.created", renderOrder(o));
    if (o.status === "paid") emit("order.paid", renderOrder(o));
    return o;
  }

  function periodItem(
    key: ProductKey,
    start: string,
    end: string,
    proration = false,
  ): Omit<Item, "id" | "createdAt"> {
    const p = products[key];
    return { label: p.name, amount: p.amount ?? 0, proration, priceId: p.priceId, start, end };
  }

  function emitSub(s: Sub, ...types: string[]) {
    for (const t of types) emit(t, renderSub(s));
  }

  function completeCheckout(
    co: Co,
    card: string,
    email: string,
  ): { ok: true } | { ok: false; message: string } {
    if (co.status !== "open") return { ok: false, message: `This checkout is ${co.status}.` };
    if (ms(co.expiresAt) < nowMs()) {
      co.status = "expired";
      touch(co);
      emit("checkout.expired", renderCheckout(co));
      return { ok: false, message: "This checkout expired." };
    }
    const c = customerFor(co.externalCustomerId, co.customerEmail ?? email);
    co.customerId = c.id;
    const p = products[co.product];
    if (card === "0002") {
      co.message = "Your card was declined. Try another card.";
      touch(co);
      emit("checkout.updated", renderCheckout(co));
      return { ok: false, message: co.message };
    }
    if (p.interval && !settings.allowMultipleSubscriptions && liveSubs(c.id).length > 0) {
      co.status = "failed";
      co.message = "You already have an active subscription.";
      touch(co);
      emit("checkout.updated", renderCheckout(co));
      return { ok: false, message: co.message };
    }
    addCard(c, card);
    c.decline = false;
    touch(c);
    const now = stamp();
    const metadata = { ...co.metadata };
    if (!p.interval) {
      makeOrder({
        customer: c,
        product: co.product,
        reason: "purchase",
        sub: null,
        checkoutId: co.id,
        items: [
          {
            label: p.name,
            amount: p.amount!,
            proration: false,
            priceId: p.priceId,
            start: null,
            end: null,
          },
        ],
        metadata,
        charge: true,
      });
    } else {
      const trial =
        co.allowTrial && p.trialDays !== null && !(settings.preventTrialAbuse && c.hadTrial);
      const trialEnd = trial ? atMs(nowMs() + p.trialDays! * dayMs) : null;
      const s: Sub = {
        id: randomUUID(),
        createdAt: now,
        modifiedAt: null,
        product: co.product,
        status: trial ? "trialing" : "active",
        periodStart: now,
        periodEnd: trial ? trialEnd! : addInterval(now, p.interval),
        trialStart: trial ? now : null,
        trialEnd,
        cancelAtPeriodEnd: false,
        canceledAt: null,
        startedAt: now,
        endsAt: null,
        endedAt: null,
        pastDueAt: null,
        customerId: c.id,
        checkoutId: co.id,
        metadata,
        pending: null,
      };
      if (trial) c.hadTrial = true;
      subs.set(s.id, s);
      co.subscriptionId = s.id;
      emitSub(s, "subscription.created");
      const items =
        p.amount === null
          ? []
          : [periodItem(co.product, s.periodStart, s.periodEnd)].map((i) =>
              trial ? { ...i, amount: 0 } : i,
            );
      makeOrder({
        customer: c,
        product: co.product,
        reason: "subscription_create",
        sub: s,
        checkoutId: co.id,
        items,
        metadata,
        charge: true,
      });
      if (!trial) emitSub(s, "subscription.active");
    }
    co.status = "succeeded";
    co.message = null;
    touch(co);
    emit("checkout.updated", renderCheckout(co));
    return { ok: true };
  }

  /** Starts the next period: after a trial, a renewal, or an ended cancellation. */
  function cycle(s: Sub, usageCents = 0) {
    if (s.status === "canceled" || s.endedAt) return;
    const c = customers.get(s.customerId)!;
    const fromTrial = s.status === "trialing";
    if (s.cancelAtPeriodEnd) {
      s.status = "canceled";
      s.endedAt = s.periodEnd;
      s.endsAt = s.periodEnd;
      s.canceledAt ??= stamp();
      touch(s);
      emitSub(s, "subscription.updated", "subscription.canceled", "subscription.revoked");
      return;
    }
    if (s.pending && ms(s.pending.appliesAt) <= ms(s.periodEnd)) {
      s.product = s.pending.product;
      s.pending = null;
    }
    const p = products[s.product];
    const ended = { start: s.periodStart, end: s.periodEnd };
    const start = s.periodEnd;
    const end = addInterval(start, p.interval ?? "month");
    s.periodStart = start;
    s.periodEnd = end;
    if (fromTrial) {
      s.trialEnd = s.trialEnd ?? start;
    }
    const o = makeOrder({
      customer: c,
      product: s.product,
      reason: p.amount === null ? "subscription_meter_cycle" : "subscription_cycle",
      sub: s,
      checkoutId: null,
      // A metered period bills the usage of the period that just ended.
      items:
        p.amount === null
          ? usageCents > 0
            ? [
                {
                  label: "API usage",
                  amount: usageCents,
                  proration: false,
                  priceId: p.priceId,
                  start: ended.start,
                  end: ended.end,
                },
              ]
            : []
          : [periodItem(s.product, start, end)],
      metadata: s.metadata,
      charge: true,
    });
    if (o.status === "pending") {
      s.status = "past_due";
      s.pastDueAt = stamp();
      touch(s);
      emitSub(s, "subscription.updated", "subscription.past_due");
    } else {
      s.status = "active";
      touch(s);
      emitSub(
        s,
        "subscription.updated",
        "subscription.cycled",
        ...(fromTrial ? ["subscription.active"] : []),
      );
    }
  }

  function retryPayment(s: Sub): boolean {
    const o = [...orders.values()].find((x) => x.subscriptionId === s.id && x.status === "pending");
    if (!o) return false;
    const c = customers.get(s.customerId)!;
    if (c.decline) return false;
    o.status = "paid";
    touch(o);
    emit("order.updated", renderOrder(o));
    emit("order.paid", renderOrder(o));
    s.status = "active";
    s.pastDueAt = null;
    touch(s);
    emitSub(s, "subscription.updated", "subscription.active");
    return true;
  }

  /** Ends trials and starts periods whose time has come on the mock clock. */
  function processDue() {
    for (const s of subs.values()) {
      let guard = 0;
      while (
        !s.endedAt &&
        s.status !== "canceled" &&
        s.status !== "past_due" &&
        ms(s.periodEnd) <= nowMs() &&
        guard++ < 24
      ) {
        cycle(s);
      }
    }
    for (const co of checkouts.values()) {
      if (co.status === "open" && ms(co.expiresAt) < nowMs()) {
        co.status = "expired";
        touch(co);
        emit("checkout.expired", renderCheckout(co));
      }
    }
  }

  function refund(
    o: Ord,
    amount: number,
    reason: models.RefundReason = "customer_request",
    disputeId: string | null = null,
  ) {
    const total = orderAmounts(o).net;
    const left = total - o.refunded;
    if (amount <= 0 || amount > left) throw new HttpError(400, { error: "RefundAmountTooHigh" });
    const r: Refund = {
      id: randomUUID(),
      createdAt: stamp(),
      modifiedAt: null,
      amount,
      orderId: o.id,
      subscriptionId: o.subscriptionId,
      customerId: o.customerId,
      reason,
      disputeId,
    };
    refunds.set(r.id, r);
    o.refunded += amount;
    o.status = o.refunded >= total ? "refunded" : "partially_refunded";
    touch(o);
    emit("refund.created", renderRefund(r));
    emit("order.refunded", renderOrder(o));
    emit("order.updated", renderOrder(o));
    return r;
  }

  function changeProduct(s: Sub, to: ProductKey, behavior: models.SubscriptionProrationBehavior) {
    const from = s.product;
    if (from === to) return;
    if (products[from].amount === null || products[to].amount === null)
      throw new HttpError(403, {
        error: "UpdateSubscriptionPlanNotAllowed",
        detail: "Metered plans cannot switch.",
      });
    if (s.status === "trialing") {
      s.product = to;
      touch(s);
      emitSub(s, "subscription.updated");
      return;
    }
    const intervalChange = products[from].interval !== products[to].interval;
    // An interval change promotes `prorate` to `invoice`.
    const mode = behavior === "prorate" && intervalChange ? "invoice" : behavior;
    if (mode === "next_period") {
      s.pending = { id: randomUUID(), createdAt: stamp(), appliesAt: s.periodEnd, product: to };
      touch(s);
      emitSub(s, "subscription.updated");
      return;
    }
    const c = customers.get(s.customerId)!;
    const now = nowMs();
    const remaining = Math.max(0, ms(s.periodEnd) - now) / (ms(s.periodEnd) - ms(s.periodStart));
    const credit = -Math.round(products[from].amount! * remaining);
    const start = atMs(now);
    const end = addInterval(start, products[to].interval!);
    const items: Array<Omit<Item, "id" | "createdAt">> = [
      {
        label: `Unused time on ${products[from].name}`,
        amount: credit,
        proration: true,
        priceId: products[from].priceId,
        start,
        end: s.periodEnd,
      },
      { ...periodItem(to, start, end, true) },
    ];
    const due =
      items.reduce((n, i) => n + i.amount, 0) -
      Math.min(
        c.balance,
        Math.max(
          0,
          items.reduce((n, i) => n + i.amount, 0),
        ),
      );
    // The update is applied only if the immediate payment (if any) succeeds.
    if (due > 0 && c.decline)
      throw new HttpError(402, { error: "PaymentFailed", detail: "Your card was declined." });
    s.product = to;
    s.periodStart = start;
    s.periodEnd = end;
    s.pending = null;
    touch(s);
    makeOrder({
      customer: c,
      product: to,
      reason: "subscription_update",
      sub: s,
      checkoutId: null,
      items,
      metadata: s.metadata,
      charge: true,
    });
    emitSub(s, "subscription.updated");
  }

  // ---------------------------------------------------------------- HTTP

  function page<T>(items: T[], url: URL): { items: T[]; pagination: models.Pagination } {
    const pageNo = Math.max(1, Number(url.searchParams.get("page") ?? 1));
    const limit = Math.min(100, Math.max(1, Number(url.searchParams.get("limit") ?? 10)));
    const total = items.length;
    return {
      items: items.slice((pageNo - 1) * limit, pageNo * limit),
      pagination: { total_count: total, max_page: Math.ceil(total / limit) },
    };
  }

  function sortBy<T>(
    items: T[],
    url: URL,
    keys: Record<string, (x: T) => string | number>,
    fallback: string,
  ) {
    const sorting = url.searchParams.getAll("sorting");
    const s = sorting[0] ?? fallback;
    const desc = s.startsWith("-");
    const key = keys[s.replace(/^-/, "")] ?? keys[fallback.replace(/^-/, "")];
    return [...items].sort((a, b) => {
      const x = key(a);
      const y = key(b);
      const c = x < y ? -1 : x > y ? 1 : 0;
      return desc ? -c : c;
    });
  }

  const notFound = () => new HttpError(404, { error: "ResourceNotFound", detail: "Not found" });

  /** Injected API failures: the next matching request answers `status`. */
  const apiFaults: Array<{ method: string; pattern: RegExp; status: number; times: number }> = [];

  async function api(req: Request, url: URL): Promise<Response> {
    if (req.headers.get("authorization") !== `Bearer ${options.accessToken}`)
      return json(401, { error: "Unauthorized", detail: "Invalid access token" });
    const f = apiFaults.find(
      (x) => x.times > 0 && x.method === req.method && x.pattern.test(url.pathname),
    );
    if (f) {
      f.times--;
      return json(f.status, { error: "MockFault", detail: `injected ${f.status}` });
    }
    const path = url.pathname;
    const method = req.method;
    const body =
      method === "GET" || method === "DELETE" ? null : await req.json().catch(() => ({}));
    let m: RegExpMatchArray | null;

    if (path === "/v1/organizations/" && method === "GET")
      return json(200, page([renderOrg()], url));
    if ((m = path.match(/^\/v1\/organizations\/([^/]+)$/)) && method === "GET") {
      if (m[1] !== ORG_ID) throw notFound();
      return json(200, renderOrg());
    }
    if (path === "/v1/products/" && method === "GET")
      return json(200, page((Object.keys(products) as ProductKey[]).map(renderProduct), url));

    if (path === "/v1/checkouts/" && method === "POST") {
      const b = body as models.CheckoutCreate;
      const key = (Object.keys(products) as ProductKey[]).find(
        (k) => products[k].productId === b.products?.[0],
      );
      if (!key || b.products.length !== 1)
        return json(422, {
          detail: [{ loc: ["body", "products"], msg: "Unknown product", type: "value_error" }],
        });
      if (b.currency && b.currency !== "usd")
        return json(422, {
          detail: [{ loc: ["body", "currency"], msg: "Unsupported currency", type: "value_error" }],
        });
      const created = stamp();
      const co: Co = {
        id: randomUUID(),
        createdAt: created,
        modifiedAt: null,
        status: "open",
        clientSecret: `polar_c_${randomBytes(16).toString("hex")}`,
        product: key,
        allowTrial: b.allow_trial ?? true,
        externalCustomerId: b.external_customer_id ?? null,
        customerEmail: b.customer_email?.toLowerCase() ?? null,
        metadata: b.metadata ?? {},
        successUrl: b.success_url ?? `${publicUrl}/`,
        returnUrl: b.return_url ?? null,
        expiresAt: atMs(nowMs() + 60 * 60_000),
        customerId: null,
        subscriptionId: null,
        message: null,
      };
      checkouts.set(co.id, co);
      emit("checkout.created", renderCheckout(co));
      return json(201, renderCheckout(co));
    }
    if (path === "/v1/checkouts/" && method === "GET") {
      let list = [...checkouts.values()];
      const ext = url.searchParams.getAll("external_customer_id");
      if (ext.length)
        list = list.filter((c) => c.externalCustomerId && ext.includes(c.externalCustomerId));
      const statuses = url.searchParams.getAll("status");
      if (statuses.length) list = list.filter((c) => statuses.includes(c.status));
      list = sortBy(list, url, { created_at: (c) => c.createdAt }, "-created_at");
      return json(200, page(list.map(renderCheckout), url));
    }
    if ((m = path.match(/^\/v1\/checkouts\/([^/]+)$/)) && method === "GET") {
      const co = checkouts.get(m[1]);
      if (!co) throw notFound();
      return json(200, renderCheckout(co));
    }

    if (path === "/v1/orders/" && method === "GET") {
      let list = [...orders.values()];
      const after = url.searchParams.get("created_after");
      if (after) list = list.filter((o) => ms(o.createdAt) >= ms(after));
      const checkoutIds = url.searchParams.getAll("checkout_id");
      if (checkoutIds.length)
        list = list.filter((o) => o.checkoutId && checkoutIds.includes(o.checkoutId));
      const statuses = url.searchParams.getAll("status");
      if (statuses.length) list = list.filter((o) => statuses.includes(o.status));
      const subIds = url.searchParams.getAll("subscription_id");
      if (subIds.length)
        list = list.filter((o) => o.subscriptionId && subIds.includes(o.subscriptionId));
      list = sortBy(list, url, { created_at: (o) => o.createdAt }, "-created_at");
      return json(200, page(list.map(renderOrder), url));
    }
    if ((m = path.match(/^\/v1\/orders\/([^/]+)\/invoice$/)) && method === "GET") {
      if (!orders.has(m[1]) && !m[1].startsWith("seed_")) throw notFound();
      return json(200, { url: `${publicUrl}/invoices/${encodeURIComponent(m[1])}.pdf` });
    }
    if ((m = path.match(/^\/v1\/orders\/([^/]+)$/)) && method === "GET") {
      const o = orders.get(m[1]);
      if (!o) throw notFound();
      return json(200, renderOrder(o));
    }

    if (path === "/v1/subscriptions/" && method === "GET") {
      let list = [...subs.values()];
      const ext = url.searchParams.getAll("external_customer_id");
      if (ext.length)
        list = list.filter((s) => ext.includes(customers.get(s.customerId)?.externalId ?? ""));
      list = sortBy(
        list,
        url,
        { started_at: (s) => s.startedAt ?? s.createdAt, ended_at: (s) => s.endedAt ?? "" },
        "-started_at",
      );
      return json(200, page(list.map(renderSub), url));
    }
    if ((m = path.match(/^\/v1\/subscriptions\/([^/]+)$/))) {
      const s = subs.get(m[1]);
      if (!s) throw notFound();
      if (method === "GET") return json(200, renderSub(s));
      if (method === "DELETE") {
        if (s.endedAt || s.status === "canceled")
          return json(403, {
            error: "AlreadyCanceledSubscription",
            detail: "This subscription is already revoked.",
          });
        s.status = "canceled";
        s.canceledAt ??= stamp();
        s.endedAt = stamp();
        s.endsAt = s.endedAt;
        s.cancelAtPeriodEnd = false;
        touch(s);
        emitSub(s, "subscription.updated", "subscription.canceled", "subscription.revoked");
        return json(200, renderSub(s));
      }
      if (method === "PATCH") {
        if (s.endedAt || s.status === "canceled")
          return json(403, {
            error: "AlreadyCanceledSubscription",
            detail: "This subscription is already revoked.",
          });
        const b = body as {
          product_id?: string;
          proration_behavior?: models.SubscriptionProrationBehavior;
          cancel_at_period_end?: boolean;
        };
        if (b.product_id) {
          const key = (Object.keys(products) as ProductKey[]).find(
            (k) => products[k].productId === b.product_id,
          );
          if (!key)
            return json(422, {
              detail: [
                { loc: ["body", "product_id"], msg: "Unknown product", type: "value_error" },
              ],
            });
          changeProduct(s, key, b.proration_behavior ?? settings.prorationBehavior);
          return json(200, renderSub(s));
        }
        if (typeof b.cancel_at_period_end === "boolean") {
          s.cancelAtPeriodEnd = b.cancel_at_period_end;
          s.canceledAt = b.cancel_at_period_end ? stamp() : null;
          s.endsAt = b.cancel_at_period_end ? s.periodEnd : null;
          touch(s);
          emitSub(
            s,
            "subscription.updated",
            b.cancel_at_period_end ? "subscription.canceled" : "subscription.uncanceled",
          );
          return json(200, renderSub(s));
        }
        return json(422, {
          detail: [{ loc: ["body"], msg: "Nothing to update", type: "value_error" }],
        });
      }
    }

    if (
      (m = path.match(/^\/v1\/customers\/external\/([^/]+)\/payment-methods$/)) &&
      method === "GET"
    ) {
      const c = [...customers.values()].find((x) => x.externalId === decodeURIComponent(m![1]));
      if (!c) throw notFound();
      // customers.listPaymentMethodsExternal answers ListResourcePaymentMethod.
      const items: models.PaymentMethod[] = c.cards.map((k, i) => ({
        id: k.id,
        created_at: k.createdAt,
        modified_at: null,
        processor: "stripe",
        customer_id: c.id,
        type: "card",
        method_metadata: {
          brand: k.brand,
          last4: k.last4,
          exp_month: k.expMonth,
          exp_year: k.expYear,
        },
        is_default: i === 0,
      }));
      return json(200, page(items, url));
    }
    if (path === "/v1/customer-sessions/" && method === "POST") {
      const b = body as {
        customer_id?: string;
        external_customer_id?: string;
        return_url?: string | null;
      };
      const c = b.customer_id
        ? customers.get(b.customer_id)
        : [...customers.values()].find((x) => x.externalId === b.external_customer_id);
      if (!c) throw notFound();
      const token = `polar_cst_${randomBytes(16).toString("hex")}`;
      sessions.set(token, { customerId: c.id, returnUrl: b.return_url ?? null });
      const session: models.CustomerSession = {
        created_at: stamp(),
        modified_at: null,
        id: randomUUID(),
        token,
        expires_at: atMs(nowMs() + 60 * 60_000),
        return_url: b.return_url ?? null,
        customer_portal_url: `${publicUrl}/portal/${token}`,
        customer_id: c.id,
        customer: renderCustomer(c),
      };
      return json(201, session);
    }

    if (path === "/v1/refunds/" && method === "GET") {
      let list = [...refunds.values()];
      const orderIds = url.searchParams.getAll("order_id");
      if (orderIds.length) list = list.filter((r) => orderIds.includes(r.orderId));
      list = sortBy(list, url, { created_at: (r) => r.createdAt }, "-created_at");
      return json(200, page(list.map(renderRefund), url));
    }
    if (path === "/v1/disputes/" && method === "GET") {
      let list = [...disputes.values()];
      const statuses = url.searchParams.getAll("status");
      if (statuses.length) list = list.filter((d) => statuses.includes(d.status));
      list = sortBy(list, url, { created_at: (d) => d.createdAt }, "-created_at");
      return json(200, page(list.map(renderDispute), url));
    }
    if ((m = path.match(/^\/v1\/disputes\/([^/]+)$/)) && method === "GET") {
      const d = disputes.get(m[1]);
      if (!d) throw notFound();
      return json(200, renderDispute(d));
    }
    throw notFound();
  }

  async function form(req: Request) {
    const text = await req.text();
    return new URLSearchParams(text);
  }

  async function pages(req: Request, url: URL): Promise<Response | null> {
    let m: RegExpMatchArray | null;
    if ((m = url.pathname.match(/^\/checkout\/([^/]+)$/)) && req.method === "GET") {
      const co = [...checkouts.values()].find((x) => x.clientSecret === m![1]);
      if (!co) return new Response("not found", { status: 404 });
      const p = products[co.product];
      const trial = co.allowTrial && p.trialDays !== null;
      const c = co.externalCustomerId
        ? [...customers.values()].find((x) => x.externalId === co.externalCustomerId)
        : null;
      const usedTrial = Boolean(c?.hadTrial && settings.preventTrialAbuse);
      return checkoutPage({
        secret: co.clientSecret,
        productName: p.name,
        priceLabel:
          p.amount === null
            ? `$${(Number(p.unitAmount) / 100).toFixed(2)} per conversion`
            : `$${(p.amount / 100).toFixed(2)}${p.interval ? ` / ${p.interval}` : ""}`,
        trialLabel:
          trial && !usedTrial ? `${p.trialDays}-day free trial, then charged automatically` : null,
        email: co.customerEmail ?? "",
        emailLocked: Boolean(co.customerEmail),
        status: co.status,
        message: co.message,
        metered: p.amount === null,
      });
    }
    if ((m = url.pathname.match(/^\/checkout\/([^/]+)\/confirm$/)) && req.method === "POST") {
      const co = [...checkouts.values()].find((x) => x.clientSecret === m![1]);
      if (!co) return new Response("not found", { status: 404 });
      const f = await form(req);
      const action = f.get("action");
      if (action === "abandon") return abandonedPage();
      const result = completeCheckout(
        co,
        action === "decline" ? "0002" : (f.get("card") ?? "4242"),
        f.get("email") ?? "",
      );
      if (!result.ok) {
        if (co.status === "failed") return failedPage(result.message);
        return Response.redirect(`${publicUrl}/checkout/${co.clientSecret}`, 303);
      }
      return new Response(null, {
        status: 303,
        headers: { location: co.successUrl.replace("{CHECKOUT_ID}", encodeURIComponent(co.id)) },
      });
    }
    if ((m = url.pathname.match(/^\/portal\/([^/]+)$/)) && req.method === "GET") {
      const sess = sessions.get(m[1]);
      if (!sess) return new Response("expired", { status: 404 });
      return renderPortal(m[1], null);
    }
    if ((m = url.pathname.match(/^\/portal\/([^/]+)\/card$/)) && req.method === "POST") {
      const sess = sessions.get(m[1]);
      if (!sess) return new Response("expired", { status: 404 });
      const c = customers.get(sess.customerId)!;
      addCard(c, "1881");
      c.decline = false;
      touch(c);
      for (const s of subs.values())
        if (s.customerId === c.id && s.status === "past_due") retryPayment(s);
      return renderPortal(m[1], "Card updated.");
    }
    if ((m = url.pathname.match(/^\/invoices\/(.+)\.pdf$/)) && req.method === "GET") {
      return new Response(minimalPdf(`Receipt ${decodeURIComponent(m[1])}`) as BodyInit, {
        headers: { "content-type": "application/pdf" },
      });
    }
    return null;
  }

  function renderPortal(token: string, message: string | null) {
    const sess = sessions.get(token)!;
    const c = customers.get(sess.customerId)!;
    const card = c.cards[0];
    return portalPage({
      token,
      email: c.email,
      card: card ? `Visa ending ${card.last4}, expires ${card.expMonth}/${card.expYear}` : null,
      subscriptions: [...subs.values()]
        .filter((s) => s.customerId === c.id)
        .map((s) => ({
          name: products[s.product].name,
          status: s.status,
          renews: s.endedAt
            ? `Ended ${s.endedAt.slice(0, 10)}`
            : `${s.cancelAtPeriodEnd ? "Ends" : "Renews"} ${s.periodEnd.slice(0, 10)}`,
        })),
      returnUrl: sess.returnUrl,
      message,
    });
  }

  // ---------------------------------------------------------------- admin

  function findSub(b: { subscription_id?: string; external_customer_id?: string }) {
    if (b.subscription_id) return subs.get(b.subscription_id) ?? null;
    if (b.external_customer_id) {
      const c = [...customers.values()].find((x) => x.externalId === b.external_customer_id);
      return (
        [...subs.values()]
          .filter((s) => s.customerId === c?.id && products[s.product].amount !== null)
          .at(-1) ?? null
      );
    }
    return null;
  }

  async function admin(req: Request, url: URL): Promise<Response> {
    const b = (req.method === "POST" ? await req.json().catch(() => ({})) : {}) as Record<
      string,
      unknown
    >;
    const name = url.pathname.slice("/admin/".length);
    switch (name) {
      case "state":
        return json(200, state());
      case "clock": {
        if (typeof b.set === "string") offsetMs = Date.parse(b.set) - Date.now();
        if (typeof b.advanceMs === "number") offsetMs += b.advanceMs;
        if (typeof b.advanceDays === "number") offsetMs += b.advanceDays * dayMs;
        processDue();
        return json(200, { now: new Date(nowMs()).toISOString() });
      }
      case "process-due":
        processDue();
        return json(200, { ok: true });
      case "trial-end":
      case "renew": {
        const s = findSub(b);
        if (!s) throw notFound();
        if (name === "trial-end" && s.status !== "trialing")
          throw new HttpError(409, { error: "not trialing" });
        offsetMs = Math.max(offsetMs, ms(s.periodEnd) - Date.now());
        cycle(s, typeof b.usage_cents === "number" ? b.usage_cents : 0);
        return json(200, renderSub(s));
      }
      case "card": {
        const c = [...customers.values()].find(
          (x) => x.externalId === b.external_customer_id || x.id === b.customer_id,
        );
        if (!c) throw notFound();
        c.decline = b.decline === true;
        return json(200, { decline: c.decline });
      }
      case "retry-payment": {
        const s = findSub(b);
        if (!s) throw notFound();
        return json(200, { paid: retryPayment(s) });
      }
      case "refund": {
        const o = orders.get(String(b.order_id));
        if (!o) throw notFound();
        const r = refund(
          o,
          typeof b.amount === "number" ? b.amount : orderAmounts(o).net - o.refunded,
        );
        return json(200, renderRefund(r));
      }
      case "dispute": {
        const action = String(b.action);
        if (action === "open") {
          const o = orders.get(String(b.order_id));
          if (!o) throw notFound();
          const d: Dispute = {
            id: randomUUID(),
            createdAt: stamp(),
            modifiedAt: null,
            status: "needs_response",
            amount: orderAmounts(o).net,
            orderId: o.id,
          };
          disputes.set(d.id, d);
          return json(200, renderDispute(d));
        }
        const d = disputes.get(String(b.dispute_id));
        if (!d) throw notFound();
        if (action === "lost" || action === "won" || action === "under_review") {
          d.status = action;
          touch(d);
        } else if (action === "prevented") {
          d.status = "prevented";
          touch(d);
          const o = orders.get(d.orderId)!;
          refund(o, orderAmounts(o).net - o.refunded, "dispute_prevention", d.id);
        }
        // Polar sends no dispute webhook: the reconciler finds disputes by listing them.
        return json(200, renderDispute(d));
      }
      case "settings": {
        if (typeof b.allow_multiple_subscriptions === "boolean")
          settings.allowMultipleSubscriptions = b.allow_multiple_subscriptions;
        if (typeof b.prevent_trial_abuse === "boolean")
          settings.preventTrialAbuse = b.prevent_trial_abuse;
        if (b.emails && typeof b.emails === "object") Object.assign(settings.emails, b.emails);
        settings.orgModifiedAt = stamp();
        emit("organization.updated", renderOrg());
        return json(200, renderOrg());
      }
      case "product": {
        const key = String(b.key) as ProductKey;
        if (!products[key]) throw notFound();
        if (typeof b.amount === "number") products[key].amount = b.amount;
        if (typeof b.unit_amount === "string") products[key].unitAmount = b.unit_amount;
        if (typeof b.price_id === "string") products[key].priceId = b.price_id;
        emit("product.updated", renderProduct(key));
        return json(200, renderProduct(key));
      }
      case "webhooks": {
        for (const k of [
          "url",
          "secret",
          "scheme",
          "mode",
          "dropNext",
          "duplicate",
          "delayMs",
          "reorder",
          "forgeNext",
          "retryBaseMs",
        ] as const) {
          if (k in b) (webhook as Record<string, unknown>)[k] = b[k];
        }
        return json(200, { ...webhook, secret: undefined, held: held.length, log: log.slice(-50) });
      }
      case "flush":
        await flush();
        return json(200, { log: log.slice(-50) });
      case "redeliver": {
        const d = all.find((x) => x.id === b.id);
        if (!d) throw notFound();
        await sendOnce(d, 99);
        return json(200, { ok: true });
      }
      case "resend-fault":
        resend.addFault(b as ResendFault);
        return json(200, { ok: true });
      case "complete-checkout": {
        const co = checkouts.get(String(b.checkout_id));
        if (!co) throw notFound();
        return json(200, completeCheckout(co, String(b.card ?? "4242"), String(b.email ?? "")));
      }
      case "preload": {
        preload(b as Preload);
        return json(200, { customers: customers.size, subscriptions: subs.size });
      }
      default:
        throw notFound();
    }
  }

  type Preload = {
    customers?: Array<{
      id: string;
      external_id: string | null;
      email: string;
      created_at?: string;
      no_card?: boolean;
    }>;
    subscriptions?: Array<{
      id: string;
      customer_id: string;
      product: ProductKey;
      status: models.SubscriptionStatus;
      current_period_start: string;
      current_period_end: string;
      trial_end?: string | null;
      cancel_at_period_end?: boolean;
      ended_at?: string | null;
      created_at?: string;
    }>;
  };

  /** Mirrors seeded fixture customers and subscriptions, so their buttons work locally. */
  function preload(p: Preload) {
    for (const c of p.customers ?? []) {
      if (customers.has(c.id)) continue;
      const rec: Cust = {
        id: c.id,
        createdAt: c.created_at ?? stamp(),
        modifiedAt: null,
        email: c.email,
        externalId: c.external_id,
        cards: [],
        balance: 0,
        decline: false,
        hadTrial: true,
        deletedAt: null,
      };
      if (!c.no_card) addCard(rec, "4242");
      customers.set(rec.id, rec);
    }
    for (const s of p.subscriptions ?? []) {
      if (subs.has(s.id) || !customers.has(s.customer_id)) continue;
      subs.set(s.id, {
        id: s.id,
        createdAt: s.created_at ?? s.current_period_start,
        modifiedAt: null,
        product: s.product,
        status: s.status,
        periodStart: s.current_period_start,
        periodEnd: s.current_period_end,
        trialStart: s.trial_end ? s.current_period_start : null,
        trialEnd: s.trial_end ?? null,
        cancelAtPeriodEnd: s.cancel_at_period_end ?? false,
        canceledAt: null,
        startedAt: s.current_period_start,
        endsAt: null,
        endedAt: s.ended_at ?? null,
        pastDueAt: s.status === "past_due" ? s.current_period_start : null,
        customerId: s.customer_id,
        checkoutId: null,
        metadata: {},
        pending: null,
      });
    }
  }

  function state() {
    return {
      now: new Date(nowMs()).toISOString(),
      settings: { ...settings },
      customers: [...customers.values()].map(renderCustomer),
      checkouts: [...checkouts.values()].map(renderCheckout),
      orders: [...orders.values()].map(renderOrder),
      subscriptions: [...subs.values()].map(renderSub),
      refunds: [...refunds.values()].map(renderRefund),
      disputes: [...disputes.values()].map(renderDispute),
      emails: resend.sent.map((e) => ({
        id: e.id,
        key: e.key,
        to: e.to,
        subject: e.subject,
        tags: e.tags,
      })),
      deliveries: log.slice(-200),
      held: held.map((h) => ({ id: h.id, type: h.type })),
      contacts: [...sequenzy.contacts.values()].map((c) => ({
        externalId: c.externalId,
        status: c.status,
        tags: c.tags,
        createdAt: c.createdAt,
        attributes: c.attributes,
      })),
    };
  }

  async function fetchHandler(req: Request): Promise<Response> {
    const url = new URL(req.url);
    try {
      if (url.pathname === "/health") return json(200, { ok: true });
      if (url.pathname === "/emails" && req.method === "POST") return await resend.handle(req);
      const contact = await sequenzy.handle(req, url);
      if (contact) return contact;
      if (url.pathname.startsWith("/v1/")) return await api(req, url);
      if (url.pathname.startsWith("/admin/")) return await admin(req, url);
      const p = await pages(req, url);
      if (p) return p;
      return json(404, { error: "ResourceNotFound" });
    } catch (e) {
      if (e instanceof HttpError) return json(e.status, e.body);
      console.error("[billing-mock]", e);
      return json(500, { error: "InternalServerError" });
    }
  }

  return {
    fetch: fetchHandler,
    /** The next `times` requests matching method and path answer `status`. */
    failApi(method: string, pattern: RegExp, status = 500, times = 1) {
      apiFaults.push({ method, pattern, status, times });
    },
    clearApiFaults: () => apiFaults.splice(0),
    resend,
    sequenzy,
    webhook,
    now: () => new Date(nowMs()),
    advance(msToAdd: number) {
      offsetMs += msToAdd;
      processDue();
    },
    setNow(d: Date) {
      offsetMs = d.getTime() - Date.now();
    },
    processDue,
    flush,
    settle,
    /** Takes the held deliveries, for tests that deliver by hand. */
    takeHeld: () => held.splice(0),
    allEvents: () => [...all],
    sign: (d: HeldDelivery, opts: { secret?: string; scheme?: Scheme; timestamp?: number } = {}) =>
      deliveryHeaders(
        opts.secret ?? webhook.secret,
        opts.scheme ?? webhook.scheme,
        d.id,
        opts.timestamp ?? Math.floor(Date.now() / 1000),
        d.body,
      ),
    state,
    completeCheckout(id: string, card = "4242", email = "") {
      const co = checkouts.get(id);
      if (!co) throw new Error(`no checkout ${id}`);
      return completeCheckout(co, card, email);
    },
    checkoutBySecret: (secret: string) =>
      [...checkouts.values()].find((c) => c.clientSecret === secret) ?? null,
    lastCheckout: () => [...checkouts.values()].at(-1) ?? null,
    customerByExternalId: (id: string) =>
      [...customers.values()].find((c) => c.externalId === id) ?? null,
    setDecline(externalId: string, decline: boolean) {
      const c = [...customers.values()].find((x) => x.externalId === externalId);
      if (!c) throw new Error(`no customer ${externalId}`);
      c.decline = decline;
    },
    subscription: (id: string) => {
      const s = subs.get(id);
      return s ? renderSub(s) : null;
    },
    order: (id: string) => {
      const o = orders.get(id);
      return o ? renderOrder(o) : null;
    },
    ordersFor: (subscriptionId: string) =>
      [...orders.values()].filter((o) => o.subscriptionId === subscriptionId).map(renderOrder),
    cycle(id: string) {
      const s = subs.get(id);
      if (!s) throw new Error(`no subscription ${id}`);
      offsetMs = Math.max(offsetMs, ms(s.periodEnd) - Date.now());
      cycle(s);
    },
    retryPayment(id: string) {
      return retryPayment(subs.get(id)!);
    },
    refund(orderId: string, amount?: number) {
      const o = orders.get(orderId)!;
      return renderRefund(refund(o, amount ?? orderAmounts(o).net - o.refunded));
    },
    openDispute(orderId: string) {
      const o = orders.get(orderId)!;
      const d: Dispute = {
        id: randomUUID(),
        createdAt: stamp(),
        modifiedAt: null,
        status: "needs_response",
        amount: orderAmounts(o).net,
        orderId,
      };
      disputes.set(d.id, d);
      return renderDispute(d);
    },
    closeDispute(id: string, status: "lost" | "won" | "prevented") {
      const d = disputes.get(id)!;
      d.status = status;
      touch(d);
      if (status === "prevented") {
        const o = orders.get(d.orderId)!;
        refund(o, orderAmounts(o).net - o.refunded, "dispute_prevention", d.id);
      }
      return renderDispute(d);
    },
    settings,
    products,
    /** Builds a raw order for edge cases the flows never produce (a discount, EUR, a zero charge). */
    craftOrder(input: {
      externalCustomerId: string | null;
      email: string;
      product: ProductKey;
      reason: models.OrderBillingReason;
      subscriptionId?: string | null;
      checkoutId?: string | null;
      items: Array<{
        amount: number;
        priceId: string | null;
        start?: string | null;
        end?: string | null;
        proration?: boolean;
      }>;
      currency?: string;
      discountAmount?: number;
      applyBalance?: number;
      metadata?: Record<string, string>;
      status?: models.OrderStatus;
    }) {
      const c = customerFor(input.externalCustomerId, input.email);
      const created = stamp();
      const o: Ord = {
        id: randomUUID(),
        createdAt: created,
        modifiedAt: null,
        status: input.status ?? "paid",
        customerId: c.id,
        product: input.product,
        reason: input.reason,
        subscriptionId: input.subscriptionId ?? null,
        checkoutId: input.checkoutId ?? null,
        items: input.items.map((i) => ({
          id: randomUUID(),
          createdAt: created,
          label: "crafted",
          amount: i.amount,
          proration: i.proration ?? false,
          priceId: i.priceId,
          start: i.start ?? null,
          end: i.end ?? null,
        })),
        subtotal: input.items.reduce((n, i) => n + i.amount, 0),
        applied: input.applyBalance ?? 0,
        refunded: 0,
        currency: input.currency ?? "usd",
        discountId: input.discountAmount ? randomUUID() : null,
        discountAmount: input.discountAmount ?? 0,
        metadata: input.metadata ?? {},
      };
      orders.set(o.id, o);
      emit("order.created", renderOrder(o));
      if (o.status === "paid") emit("order.paid", renderOrder(o));
      return renderOrder(o);
    },
    /** Changes an object without emitting a webhook, as an unseen provider-side change would. */
    mutateQuietly(
      kind: "order" | "subscription",
      id: string,
      change: (x: Record<string, unknown>) => void,
      keepVersion = false,
    ) {
      const target = (kind === "order" ? orders.get(id) : subs.get(id)) as unknown as Record<
        string,
        unknown
      > & { modifiedAt: string | null };
      if (!target) throw new Error(`no ${kind} ${id}`);
      change(target);
      if (!keepVersion) target.modifiedAt = stamp();
    },
    renderOrderRaw: (id: string) => renderOrder(orders.get(id)!),
  };
}

export type BillingMock = ReturnType<typeof createBillingMock>;

function minimalPdf(title: string): string {
  const text = title.replace(/[()\\]/g, "");
  const objs = [
    "<< /Type /Catalog /Pages 2 0 R >>",
    "<< /Type /Pages /Kids [3 0 R] /Count 1 >>",
    "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 300 144] /Contents 4 0 R /Resources << /Font << /F1 5 0 R >> >> >>",
    `<< /Length ${44 + text.length} >>\nstream\nBT /F1 14 Tf 24 72 Td (${text}) Tj ET\nendstream`,
    "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>",
  ];
  let out = "%PDF-1.4\n";
  const offsets: number[] = [];
  objs.forEach((o, i) => {
    offsets.push(out.length);
    out += `${i + 1} 0 obj\n${o}\nendobj\n`;
  });
  const xref = out.length;
  out += `xref\n0 ${objs.length + 1}\n0000000000 65535 f \n${offsets.map((o) => `${String(o).padStart(10, "0")} 00000 n \n`).join("")}`;
  out += `trailer\n<< /Size ${objs.length + 1} /Root 1 0 R >>\nstartxref\n${xref}\n%%EOF\n`;
  return out;
}
