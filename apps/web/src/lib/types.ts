// Shapes the dashboard and account pages render. The real account API (plan P6 to P9)
// should return these, so the pages do not change when it lands.

export type Account = {
  name: string;
  email: string;
  emailVerified: boolean;
  avatarUrl: string | null;
};

export type PlanSummary = {
  name: string;
  priceLabel: string;
  /** ISO date. */
  renewsOn: string;
};

export type License = {
  id: string;
  product: "Pro" | "Desktop";
  maskedKey: string;
  activeMacs: number;
  updatesLabel: string;
  /** Short line under the key, such as how the license was bought. */
  detail: string;
};

export type Mac = {
  id: string;
  name: string;
  os: string;
  /** Display text from the server, such as "today". */
  lastSeen: string;
};

export type Overview = {
  account: Account;
  plan: PlanSummary;
  license: License;
  /** `since` is an ISO date. */
  api: { conversionsThisMonth: number; since: string; keyCount: number };
  macs: Mac[];
};

export type Invoice = {
  id: string;
  /** ISO date. */
  date: string;
  description: string;
  amountCents: number;
};

export type Billing = {
  plan: {
    name: string;
    status: "active" | "trialing" | "past_due" | "canceled";
    summary: string;
    interval: "month" | "year";
  };
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
  /** The account must keep at least one method, so the email link cannot be removed. */
  removable: boolean;
};

export type Session = {
  id: string;
  name: string;
  current: boolean;
  kind: string;
  lastSeen: string;
};

export type AccountSettings = {
  account: Account;
  methods: SignInMethod[];
  sessions: Session[];
};
