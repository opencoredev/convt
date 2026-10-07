// Database rows to the shapes the pages render. Pure, so every label is unit-tested
// (test/unit/views.test.ts). Dates in labels are UTC, like src/lib/format.ts.

import {
  activeApiSubscription,
  currentProSubscription,
  deriveAccountState,
  type AccountState,
} from "@convt/db/queries";
import { maskApiKey } from "@convt/license";

import { formatDate, formatShortDate } from "#/lib/format";
import type { CheckoutResult } from "@convt/billing/rpc";

import type {
  Account,
  ApiEnrollment,
  ApiKey,
  Billing,
  Invoice,
  License,
  Mac,
  Overview,
  PlanSummary,
  Session,
  SignInMethod,
} from "#/lib/types";

const dayMs = 86_400_000;

export type UserRow = {
  id: string;
  name: string;
  email: string;
  emailVerified: boolean;
  image: string | null;
};

export type SubscriptionRow = {
  id: string;
  kind: string;
  interval: string | null;
  status: string;
  trialEndsAt: Date | null;
  currentPeriodEnd: Date | null;
  cancelAtPeriodEnd: boolean;
  endedAt: Date | null;
  spendCapCents: number | null;
  cardSeenAt?: Date | null;
  createdAt: Date;
};

export type LicenseRow = {
  id: string;
  plan: string;
  trial: boolean;
  issuedOn: string;
  updatesUntil: string;
  revokedAt: Date | null;
  revokeReason?: string | null;
  orderPaidAt: Date | null;
  subscriptionInterval: string | null;
};

export type DeviceRow = { id: string; name: string; os: string; lastSeenAt: Date | null };

export type InvoiceRow = {
  id: string;
  description: string;
  amountCents: number;
  issuedAt: Date;
  status?: string;
};

export type ApiKeyRow = {
  id: string;
  name: string;
  prefix: string;
  createdAt: Date;
  lastUsedAt: Date | null;
};

export type AccountRow = { id: string; providerId: string; accountId: string };

export type SessionRow = { id: string; userAgent: string | null; createdAt: Date; updatedAt: Date };

const iso = (d: Date) => d.toISOString();
const isoDay = (d: Date) => d.toISOString().slice(0, 10);

/** The name in the header: the profile name, else the part of the email before @. */
export function accountView(user: UserRow): Account {
  const name = user.name.trim() || user.email.split("@")[0];
  return { name, email: user.email, emailVerified: user.emailVerified, avatarUrl: user.image };
}

/** "today", "yesterday", or "Sep 28". */
export function seenLabel(when: Date | null, now: Date): string {
  if (!when) return "never";
  const days = Math.floor(utcDay(now) / dayMs) - Math.floor(utcDay(when) / dayMs);
  if (days <= 0) return "today";
  if (days === 1) return "yesterday";
  return formatShortDate(iso(when));
}

/** "Active now", "2 minutes ago", "3 hours ago", else a date. */
export function lastUsedLabel(when: Date | null, now: Date): string {
  if (!when) return "Never";
  const minutes = Math.floor((now.getTime() - when.getTime()) / 60_000);
  if (minutes < 1) return "Just now";
  if (minutes < 60) return `${minutes} minute${minutes === 1 ? "" : "s"} ago`;
  const hours = Math.floor(minutes / 60);
  if (hours < 24) return `${hours} hour${hours === 1 ? "" : "s"} ago`;
  return formatDate(iso(when));
}

const utcDay = (d: Date) => Date.UTC(d.getUTCFullYear(), d.getUTCMonth(), d.getUTCDate());

/** Only the last characters of the license id show; the token never leaves `getLicenseKey`. */
export function maskLicense(id: string): string {
  return `CNVT-••••-••••-${id.slice(-4).toUpperCase()}`;
}

export function licenseView(row: LicenseRow, activeMacs: number, now: Date): License {
  const product = row.plan === "pro" ? "Pro" : "Desktop";
  const until = formatDate(`${row.updatesUntil}T00:00:00Z`);
  const covered = `${row.updatesUntil}T23:59:59Z` >= iso(now);
  let updatesLabel: string;
  let detail: string;
  const disputed = row.revokeReason === "dispute_lost";
  if (row.revokedAt) {
    // Revocation changes the dashboard only: an activated key keeps working offline.
    updatesLabel = `${disputed ? "Disputed" : "Refunded"} on ${formatDate(iso(row.revokedAt))}. It won't be renewed or reissued. Copies already activated offline keep working.`;
  } else if (row.plan === "desktop") {
    updatesLabel = `Updates until ${until}`;
  } else if (row.trial) {
    updatesLabel = covered ? `Trial until ${until}` : `Trial ended ${until}`;
  } else {
    // A paid Pro key is renewed every billing period, so updates are simply included.
    updatesLabel = covered ? "Updates included" : `Builds up to ${until}`;
  }
  if (row.plan === "desktop") {
    detail = `Desktop License, bought ${formatDate(iso(row.orderPaidAt ?? new Date(`${row.issuedOn}T00:00:00Z`)))}`;
  } else if (row.trial) {
    detail = "Pro, free trial";
  } else {
    detail = `Pro, ${row.subscriptionInterval === "year" ? "yearly" : "monthly"}`;
  }
  return {
    id: row.id,
    product,
    maskedKey: maskLicense(row.id),
    activeMacs,
    updatesLabel,
    detail,
    revoked: row.revokedAt !== null,
    revokedBadge: row.revokedAt ? (disputed ? "DISPUTED" : "REFUNDED") : null,
  };
}

export function macView(device: DeviceRow, now: Date): Mac {
  return {
    id: device.id,
    name: device.name,
    os: device.os,
    lastSeen: seenLabel(device.lastSeenAt, now),
  };
}

const proPrice = {
  month: { per: "$12/mo", total: "$12 a month" },
  year: { per: "$8/mo, billed yearly", total: "$96 a year" },
};

/** The license the app would use: the current Pro or trial key, else the newest Desktop key. */
export function primaryLicense(state: AccountState, licenses: LicenseRow[]): LicenseRow | null {
  const live = licenses.filter((l) => !l.revokedAt);
  const pro = live.find((l) => l.plan === "pro");
  const desktop = live.find((l) => l.plan === "desktop");
  if (state === "pro" || state === "trial") return pro ?? desktop ?? null;
  if (state === "desktop") return desktop ?? null;
  if (state === "pro_lapsed") return pro ?? null;
  return null;
}

export function planSummary(
  state: AccountState,
  subscriptions: SubscriptionRow[],
  licenses: LicenseRow[],
  now: Date,
): PlanSummary | null {
  const pro = currentProSubscription(subscriptions, now);
  const interval = pro?.interval === "year" ? "year" : "month";
  switch (state) {
    case "pro": {
      const end = pro?.currentPeriodEnd;
      const meta = end
        ? `${pro?.cancelAtPeriodEnd ? "Ends" : "Renews"} ${formatDate(iso(end))}`
        : "Active";
      return {
        name: "Pro",
        priceLabel: proPrice[interval].per,
        meta: pro?.status === "past_due" ? "Payment failed" : meta,
      };
    }
    case "trial": {
      const end = pro?.trialEndsAt ?? pro?.currentPeriodEnd;
      return {
        name: "Pro trial",
        priceLabel: `Then ${proPrice[interval].per}`,
        meta: end ? `Trial ends ${formatDate(iso(end))}` : "Trial",
      };
    }
    case "desktop": {
      const desktop = licenses.find((l) => l.plan === "desktop" && !l.revokedAt);
      return {
        name: "Desktop",
        priceLabel: "$29, paid once",
        meta: desktop
          ? `Updates until ${formatDate(`${desktop.updatesUntil}T00:00:00Z`)}`
          : "Paid once",
      };
    }
    case "pro_lapsed": {
      const end = pro?.endedAt ?? pro?.currentPeriodEnd;
      return {
        name: "Pro",
        priceLabel: "Not renewed",
        meta: end ? `Ended ${formatDate(iso(end))}` : "Ended",
      };
    }
    default:
      return null;
  }
}

export function overviewView(input: {
  user: UserRow;
  subscriptions: SubscriptionRow[];
  licenses: LicenseRow[];
  devices: DeviceRow[];
  apiMonth: { count: number; since: Date };
  apiKeyCount: number;
  now: Date;
}): Overview {
  const { subscriptions, licenses, devices, now } = input;
  const state = deriveAccountState(subscriptions, licenses, now);
  const license = primaryLicense(state, licenses);
  const api = activeApiSubscription(subscriptions, now);
  return {
    account: accountView(input.user),
    state,
    plan: planSummary(state, subscriptions, licenses, now),
    license: license ? licenseView(license, devices.length, now) : null,
    api: api
      ? {
          conversionsThisMonth: input.apiMonth.count,
          since: isoDay(input.apiMonth.since),
          keyCount: input.apiKeyCount,
        }
      : null,
    macs: devices.map((d) => macView(d, now)),
  };
}

/** All licenses; the active devices count against the one the app would use. */
export function licensesView(
  subscriptions: SubscriptionRow[],
  licenses: LicenseRow[],
  devices: DeviceRow[],
  now: Date,
) {
  const state = deriveAccountState(subscriptions, licenses, now);
  const primary = primaryLicense(state, licenses);
  return {
    licenses: licenses.map((l) => licenseView(l, l.id === primary?.id ? devices.length : 0, now)),
    macs: devices.map((d) => macView(d, now)),
  };
}

const liveStatuses = new Set(["trialing", "active", "past_due", "unpaid", "incomplete"]);

/**
 * API enrollment: usable when the newest API subscription is active with a cap and
 * a card seen; pending while a checkout is open, the subscription is incomplete,
 * or no card has been seen; payment failed when past due; ended otherwise.
 */
export function apiEnrollment(
  subscriptions: SubscriptionRow[],
  openCheckout: boolean,
  now: Date,
): ApiEnrollment {
  const api = subscriptions
    .filter((s) => s.kind === "api")
    .sort((a, b) => b.createdAt.getTime() - a.createdAt.getTime())[0];
  const endedApi =
    !api || !liveStatuses.has(api.status) || (api.endedAt !== null && api.endedAt <= now);
  if (endedApi) {
    if (openCheckout) return { state: "pending", spendCapCents: null, endsOn: null };
    return {
      state: api ? "ended" : "none",
      spendCapCents: api?.spendCapCents ?? null,
      endsOn: null,
    };
  }
  const endsOn =
    api.cancelAtPeriodEnd && api.currentPeriodEnd ? isoDay(api.currentPeriodEnd) : null;
  if (api.status === "past_due" || api.status === "unpaid")
    return { state: "payment_failed", spendCapCents: api.spendCapCents, endsOn };
  if (api.status === "active" && api.spendCapCents !== null && api.cardSeenAt)
    return { state: "enrolled", spendCapCents: api.spendCapCents, endsOn };
  return { state: "pending", spendCapCents: api.spendCapCents, endsOn };
}

const invoiceStatusLabels: Record<string, string> = {
  refunded: "Refunded",
  partially_refunded: "Partly refunded",
  open: "Payment failed",
  uncollectible: "Payment failed",
  void: "Void",
};

/** Polar sometimes leaves a trial as `incomplete` until the first paid cycle. */
function openProTrial(subscriptions: SubscriptionRow[], now: Date): SubscriptionRow | null {
  const open = subscriptions
    .filter(
      (s) =>
        s.kind === "pro" &&
        (s.status === "trialing" || s.status === "incomplete") &&
        s.trialEndsAt !== null &&
        s.trialEndsAt > now &&
        (s.endedAt === null || s.endedAt > now),
    )
    .sort((a, b) => b.createdAt.getTime() - a.createdAt.getTime());
  return open[0] ?? null;
}

export function billingView(input: {
  user: UserRow;
  subscriptions: SubscriptionRow[];
  invoices: InvoiceRow[];
  card: Billing["card"];
  openApiCheckout: boolean;
  now: Date;
}): Billing {
  const { user, subscriptions, invoices, now } = input;
  const pro = currentProSubscription(subscriptions, now) ?? openProTrial(subscriptions, now);
  let plan: Billing["plan"] = null;
  if (pro) {
    const interval = pro.interval === "year" ? "year" : "month";
    const price = proPrice[interval].total;
    const end = pro.currentPeriodEnd ? formatDate(iso(pro.currentPeriodEnd)) : null;
    const includes =
      "Includes the desktop app on your computers, every update while you're subscribed, and API access.";
    const ended =
      pro.status === "canceled" ||
      pro.status === "unpaid" ||
      pro.status === "incomplete_expired" ||
      pro.status === "paused" ||
      (pro.endedAt !== null && pro.endedAt <= now);
    const onTrial =
      !ended &&
      (pro.status === "trialing" ||
        (pro.status === "incomplete" && pro.trialEndsAt !== null && pro.trialEndsAt > now));
    let status: NonNullable<Billing["plan"]>["status"];
    let summary: string;
    const cancelsOn =
      !ended && pro.cancelAtPeriodEnd && pro.currentPeriodEnd ? isoDay(pro.currentPeriodEnd) : null;
    if (ended) {
      status = "canceled";
      const on = formatDate(iso(pro.endedAt ?? pro.currentPeriodEnd ?? now));
      summary = `Ended ${on}. Your last key keeps working for every build released before then.`;
    } else if (onTrial) {
      status = "trialing";
      const trialEnd = pro.trialEndsAt ?? pro.currentPeriodEnd;
      const until = trialEnd ? formatDate(iso(trialEnd)) : "the trial ends";
      summary = pro.cancelAtPeriodEnd
        ? `Free until ${until}, then the trial ends. You won't be charged.`
        : `Free until ${until}, then ${price}. ${includes}`;
    } else if (pro.status === "past_due") {
      status = "past_due";
      summary = `${price}. The last payment didn't go through, so we'll try again. Update your card to keep Pro.`;
    } else {
      status = "active";
      summary = pro.cancelAtPeriodEnd
        ? `${price}. Ends ${end}, and won't renew. ${includes}`
        : `${price}. Renews ${end}. ${includes}`;
    }
    plan = {
      name: `Pro, ${interval === "year" ? "yearly" : "monthly"}`,
      status,
      summary,
      interval,
      cancelsOn,
    };
  }
  return {
    plan,
    hadPro: subscriptions.some((s) => s.kind === "pro"),
    api: apiEnrollment(subscriptions, input.openApiCheckout, now),
    card: input.card,
    receiptEmail: user.email,
    invoices: invoices.map((i): Invoice => ({
      id: i.id,
      date: isoDay(i.issuedAt),
      description: i.description,
      amountCents: i.amountCents,
      statusLabel: (i.status && invoiceStatusLabels[i.status]) ?? null,
    })),
  };
}

export type CheckoutView =
  | {
      state: "pending" | "trial" | "api_enrolled" | "failed" | "shown" | "not_found";
      product: "desktop" | "pro" | "api" | null;
      allowTrial: boolean;
    }
  | {
      state: "ready";
      product: "desktop" | "pro";
      maskedEmail: string;
      /** "Oct 5, 2027" */
      updatesUntil: string;
      token: string;
      licenseId: string;
    };

/** After the success page stops polling. A Pro trial issues no key. */
export type CheckoutGiveUp =
  | { state: "trial"; product: "pro"; allowTrial: true }
  | { state: "email" };

export function checkoutGiveUp(
  product: CheckoutView["product"] | undefined,
  allowTrial?: boolean,
): CheckoutGiveUp {
  if (product === "pro" && allowTrial) return { state: "trial", product: "pro", allowTrial: true };
  return { state: "email" };
}

const productGroup = (p: string | undefined) =>
  p === "desktop" ? "desktop" : p === "api" ? "api" : p ? "pro" : null;

/** The success page's states. The token appears only in `ready`. */
export function checkoutView(r: CheckoutResult): CheckoutView {
  if (r.state === "ready") {
    return {
      state: "ready",
      product: r.product === "desktop" ? "desktop" : "pro",
      maskedEmail: r.maskedEmail,
      updatesUntil: formatDate(`${r.updatesUntil}T00:00:00Z`),
      token: r.token,
      licenseId: r.licenseId,
    };
  }
  return { state: r.state, product: productGroup(r.product), allowTrial: r.allowTrial === true };
}

export function apiKeyView(key: ApiKeyRow, now: Date): ApiKey {
  return {
    id: key.id,
    name: key.name,
    maskedKey: maskApiKey(key.prefix),
    created: isoDay(key.createdAt),
    lastUsed: lastUsedLabel(key.lastUsedAt, now),
  };
}

/** "Chrome on macOS", from a user agent. */
export function browserLabel(userAgent: string | null): string {
  const ua = userAgent ?? "";
  const browser = /Edg\//.test(ua)
    ? "Edge"
    : /Firefox\//.test(ua)
      ? "Firefox"
      : /Chrome\//.test(ua) || /CriOS\//.test(ua)
        ? "Chrome"
        : /Safari\//.test(ua)
          ? "Safari"
          : "Browser";
  const os = /iPhone|iPad/.test(ua)
    ? "iOS"
    : /Android/.test(ua)
      ? "Android"
      : /Mac OS X|Macintosh/.test(ua)
        ? "macOS"
        : /Windows/.test(ua)
          ? "Windows"
          : /Linux|X11/.test(ua)
            ? "Linux"
            : null;
  return os ? `${browser} on ${os}` : browser;
}

const providerLabels = { github: "GitHub", google: "Google" } as const;

/**
 * The sign-in methods the settings page lists. An unconfigured provider can't be
 * connected, so it is hidden, but a method already linked stays listed so its owner
 * can see and remove it. Email always shows.
 */
export function visibleMethods(
  methods: SignInMethod[],
  available: Record<Exclude<SignInMethod["id"], "email">, boolean>,
): SignInMethod[] {
  return methods.filter((m) => m.id === "email" || available[m.id] || m.accountId !== null);
}

export function settingsView(input: {
  user: UserRow;
  accounts: AccountRow[];
  sessions: SessionRow[];
  devices: DeviceRow[];
  currentSessionId: string;
  deletion?: { status: string; createdAt: Date } | null;
  now: Date;
}) {
  const { user, accounts, now } = input;
  const methods: SignInMethod[] = [
    { id: "email", label: "Email link", identity: user.email, removable: false, accountId: null },
    ...(["github", "google"] as const).map((provider): SignInMethod => {
      const linked = accounts.find((a) => a.providerId === provider);
      return {
        id: provider,
        label: providerLabels[provider],
        // Better Auth keeps the provider's user id, not the handle.
        identity: linked ? `ID ${linked.accountId}` : null,
        removable: true,
        accountId: linked?.id ?? null,
      };
    }),
    { id: "apple", label: "Apple", identity: null, removable: true, accountId: null },
  ];
  const sessions: Session[] = [
    ...input.sessions
      .map((s): Session => {
        const current = s.id === input.currentSessionId;
        return {
          id: s.id,
          type: "web",
          name: browserLabel(s.userAgent),
          current,
          kind: "Web dashboard",
          lastSeen: current ? "Active now" : `Seen ${seenLabel(s.updatedAt, now)}`,
        };
      })
      .sort((a, b) => Number(b.current) - Number(a.current)),
    ...input.devices.map((d): Session => ({
      id: d.id,
      type: "device",
      name: d.name,
      current: false,
      kind: `convt app, ${d.os}`,
      lastSeen: `Seen ${seenLabel(d.lastSeenAt, now)}`,
    })),
  ];
  const deletion = input.deletion
    ? { status: input.deletion.status, started: iso(input.deletion.createdAt) }
    : null;
  return { account: accountView(user), methods, sessions, deletion };
}
