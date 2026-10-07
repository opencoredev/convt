// Shapes the dashboard and account pages render. src/server/views.ts builds them
// from database rows; the pages only format them.

export type Account = {
  name: string;
  email: string;
  emailVerified: boolean;
  avatarUrl: string | null;
};

export type PlanSummary = {
  name: string;
  priceLabel: string;
  /** Short line under the card's divider, such as "Renews Oct 2, 2027". */
  meta: string;
};

export type License = {
  id: string;
  product: "Pro" | "Desktop";
  maskedKey: string;
  activeMacs: number;
  updatesLabel: string;
  /** Short line under the key, such as how the license was bought. */
  detail: string;
  revoked: boolean;
  /** `REFUNDED` or `DISPUTED` on a revoked key; null otherwise. */
  revokedBadge: "REFUNDED" | "DISPUTED" | null;
};

export type Mac = {
  id: string;
  name: string;
  os: string;
  /** Display text from the server, such as "today". */
  lastSeen: string;
};

/** Which overview a signed-in account sees (see deriveAccountState in packages/db). */
export type AccountState = "new" | "trial" | "desktop" | "pro" | "pro_lapsed" | "api_only";

export type Overview = {
  account: Account;
  state: AccountState;
  /** Null when the account has no plan: the card shows an empty line and pricing. */
  plan: PlanSummary | null;
  license: License | null;
  /** Null without API enrollment. `since` is an ISO date. */
  api: { conversionsThisMonth: number; since: string; keyCount: number } | null;
  macs: Mac[];
};

export type Invoice = {
  id: string;
  /** ISO date. */
  date: string;
  description: string;
  amountCents: number;
  /** "Refunded", "Payment failed" and similar; null when paid. */
  statusLabel: string | null;
};

/** API enrollment, from the newest API subscription (and an unfinished checkout). */
export type ApiEnrollment = {
  state: "none" | "pending" | "enrolled" | "payment_failed" | "ended";
  spendCapCents: number | null;
  /** ISO date when billing ends, if set to end. */
  endsOn: string | null;
};

export type Billing = {
  /** The current Pro subscription, a live Desktop license, or null. */
  plan: {
    kind: "pro" | "desktop";
    name: string;
    status: "active" | "trialing" | "past_due" | "canceled";
    summary: string;
    /** Null for a one-time Desktop license. */
    interval: "month" | "year" | null;
    /** ISO date the plan ends because it was set to cancel; null when it renews. */
    cancelsOn: string | null;
  } | null;
  /** True when the account had Pro before, so a new start has no trial. */
  hadPro: boolean;
  /** A live (not refunded or disputed) Desktop license. */
  ownsDesktop: boolean;
  api: ApiEnrollment;
  card: { brand: string; last4: string; expires: string } | null;
  receiptEmail: string;
  invoices: Invoice[];
};

/** `date` is an ISO date. */
export type UsageDay = { date: string; conversions: number };

export type ApiKey = {
  id: string;
  name: string;
  maskedKey: string;
  /** ISO date. */
  created: string;
  /** Display text from the server, such as "2 minutes ago". */
  lastUsed: string;
};

export type ApiOverview = {
  enrollment: ApiEnrollment;
  /** True when the provider's settings refuse a second subscription (enrollment is refused). */
  enrollBlocked: boolean;
  thisMonth: number;
  /** ISO date. */
  since: string;
  last30Days: number;
  failed: number;
  /** Oldest first. Days in the current month are highlighted in the chart. */
  perDay: UsageDay[];
  keys: ApiKey[];
};

export type SignInMethod = {
  id: "email" | "github" | "google" | "apple";
  label: string;
  identity: string | null;
  /** The verified email always signs in by code, so it cannot be removed. */
  removable: boolean;
  /** The account row to unlink, for GitHub and Google. */
  accountId: string | null;
};

export type Session = {
  id: string;
  /** A browser session or a desktop app (device). */
  type: "web" | "device";
  name: string;
  current: boolean;
  kind: string;
  lastSeen: string;
};

export type AccountSettings = {
  account: Account;
  methods: SignInMethod[];
  sessions: Session[];
  /** An account deletion in progress, if any. */
  deletion: { status: string; started: string } | null;
};
