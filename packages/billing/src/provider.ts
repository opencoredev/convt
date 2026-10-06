// The boundary between convt's billing logic and a payment provider. Everything
// above it speaks catalog names and normalized facts; only an adapter (polar.ts)
// knows the provider's ids, payloads and signature scheme. A Stripe adapter would
// produce the same facts.

import type { CatalogProduct, ProProduct, Proration } from "./catalog";
import type { Delivery, Rejected } from "./verify";

export type OrderStatus = "draft" | "pending" | "paid" | "partially_refunded" | "refunded" | "void";
export type OrderReason =
  | "purchase"
  | "subscription_create"
  | "subscription_cycle"
  | "subscription_update"
  | "subscription_meter_cycle";
export type SubscriptionStatus =
  | "incomplete"
  | "incomplete_expired"
  | "trialing"
  | "active"
  | "past_due"
  | "canceled"
  | "unpaid"
  | "paused";
export type DisputeStatus =
  | "prevented"
  | "early_warning"
  | "needs_response"
  | "under_review"
  | "lost"
  | "won";
export type CheckoutStatus = "open" | "expired" | "confirmed" | "succeeded" | "failed";

/** One order line. Pro lines with a catalog price become payment coverage. */
export type CoverageItem = {
  providerItemId: string;
  product: CatalogProduct | null;
  priceId: string | null;
  periodStart: string | null;
  periodEnd: string | null;
  /** Negative for a credit. */
  amountCents: number;
  proration: boolean;
};

type Versioned = {
  /** The provider's `modified_at`, or `created_at` when it was never modified. Full precision. */
  version: string;
  /** SHA-256 of the normalized fields. */
  hash: string;
};

export type SubscriptionFact = Versioned & {
  kind: "subscription";
  providerSubscriptionId: string;
  providerCustomerId: string;
  providerCheckoutId: string | null;
  /** Our `checkouts.id`, from the checkout's metadata. */
  checkoutRef: string | null;
  /** From the customer's external id; null for a customer we did not create. */
  userId: string | null;
  email: string | null;
  product: CatalogProduct | null;
  providerProductId: string;
  status: SubscriptionStatus;
  currency: string;
  discountId: string | null;
  trialEndsAt: string | null;
  currentPeriodStart: string | null;
  currentPeriodEnd: string | null;
  cancelAtPeriodEnd: boolean;
  canceledAt: string | null;
  endedAt: string | null;
  /** Undefined when the source does not carry it (a subscription embedded in an order). */
  pendingUpdate?: unknown;
};

export type OrderFact = Versioned & {
  kind: "order";
  providerOrderId: string;
  providerCustomerId: string;
  providerCheckoutId: string | null;
  checkoutRef: string | null;
  providerSubscriptionId: string | null;
  userId: string | null;
  email: string | null;
  product: CatalogProduct | null;
  providerProductId: string | null;
  reason: OrderReason;
  status: OrderStatus;
  subtotalCents: number;
  discountCents: number;
  discountId: string | null;
  netCents: number;
  appliedBalanceCents: number;
  refundedCents: number;
  currency: string;
  /** The provider's `created_at`. Feeds license dates; written once. */
  billedAt: string;
  description: string;
  items: CoverageItem[];
  /** The order's subscription as embedded in it: a second snapshot. */
  subscription: SubscriptionFact | null;
};

export type DisputeFact = Versioned & {
  kind: "dispute";
  providerDisputeId: string;
  providerOrderId: string;
  status: DisputeStatus;
  amountCents: number;
  closed: boolean;
};

export type CheckoutFact = Versioned & {
  kind: "checkout";
  providerCheckoutId: string;
  checkoutRef: string | null;
  status: CheckoutStatus;
  providerSubscriptionId: string | null;
};

export type CustomerFact = {
  kind: "customer";
  providerCustomerId: string;
  userId: string | null;
  email: string | null;
  deleted: boolean;
  version: string;
};

export type Hint =
  | { kind: "order"; id: string }
  | { kind: "subscription"; id: string }
  | { kind: "dispute"; id: string }
  | { kind: "settings" };

export type ProviderFacts = {
  orders: OrderFact[];
  subscriptions: SubscriptionFact[];
  disputes: DisputeFact[];
  checkouts: CheckoutFact[];
  customers: CustomerFact[];
  /** Objects to fetch before applying, such as a refund's order. */
  hints: Hint[];
  /** Alerts the payload itself raises: a paused or migrated subscription, a deleted customer. */
  alerts: Array<{ kind: string; subject: string; detail: string }>;
};

export type ParsedEvent =
  | { ok: true; type: string; facts: ProviderFacts; ignored: boolean }
  | { ok: false; type: string; reason: string };

export type CheckoutInput = {
  product: CatalogProduct;
  /** Our checkouts.id, carried in the provider's metadata. */
  checkoutRef: string;
  successUrl: string;
  allowTrial: boolean;
  externalCustomerId: string | null;
  email: string | null;
};

export type CardFact = { brand: string; last4: string; expMonth: number; expYear: number };

export type PaymentFailed = { paymentFailed: true; detail: string };

export type ScanKind = "orders" | "subscriptions" | "refunds" | "disputes";
export type ScanCursor = { page: number; limit?: number; createdAfter?: string };
export type ScanPage = {
  facts: ProviderFacts;
  page: number;
  maxPage: number;
};

export type OrganizationFacts = {
  allowMultipleSubscriptions: boolean;
  preventTrialAbuse: boolean;
  trialConversionEmail: boolean;
  pastDueEmail: boolean;
};

export type ProductFacts = Array<{
  productId: string;
  priceId: string;
  amountType: string;
  amountCents: number | null;
  unitAmount: string | null;
  currency: string;
  interval: string | null;
  archived: boolean;
}>;

export class ProviderError extends Error {
  constructor(
    message: string,
    readonly status: number | null,
    readonly code: string,
  ) {
    super(message);
    this.name = "ProviderError";
  }
}

export interface BillingProvider {
  name: "polar" | "stripe";
  verifyWebhook(raw: Uint8Array, headers: Headers, now: Date): Promise<Delivery | Rejected>;
  /** Strict UTF-8 JSON, validated, then normalized. Never throws. */
  parseEvent(delivery: Delivery): ParsedEvent;
  createCheckout(input: CheckoutInput): Promise<{ providerCheckoutId: string; url: string }>;
  getCheckout(id: string): Promise<CheckoutFact>;
  /** Orders created from one checkout, for the success page's sync. */
  checkoutOrders(providerCheckoutId: string): Promise<OrderFact[]>;
  getOrder(id: string): Promise<OrderFact>;
  getSubscription(id: string): Promise<SubscriptionFact>;
  getDispute(id: string): Promise<DisputeFact>;
  scan(kind: ScanKind, from: ScanCursor): Promise<ScanPage>;
  paymentMethods(userId: string): Promise<CardFact[]>;
  /** Every subscription the provider holds for this account (our user id as external id). */
  customerSubscriptions(userId: string): Promise<SubscriptionFact[]>;
  /** How many of this account's checkouts are still open at the provider. */
  openCheckouts(userId: string): Promise<number>;
  changeProduct(
    subscriptionId: string,
    product: ProProduct,
    proration: Proration,
  ): Promise<SubscriptionFact | PaymentFailed>;
  setCancelAtPeriodEnd(subscriptionId: string, value: boolean): Promise<SubscriptionFact>;
  /** Ends a subscription now, without a refund. "Already ended" resolves by fetching it. */
  revokeSubscription(subscriptionId: string): Promise<SubscriptionFact>;
  portalUrl(userId: string, returnUrl: string): Promise<string>;
  receiptUrl(providerOrderId: string): Promise<string>;
  settings(): Promise<OrganizationFacts>;
  products(): Promise<ProductFacts>;
  // P9: reportUsage(records), each record's id as the idempotency key.
}

export const emptyFacts = (): ProviderFacts => ({
  orders: [],
  subscriptions: [],
  disputes: [],
  checkouts: [],
  customers: [],
  hints: [],
  alerts: [],
});
