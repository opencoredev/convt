// PLACEHOLDER DATA. Nothing in this file is real.
//
// There is no account backend yet (plan P6 to P9). Every dashboard and account page
// reads from here through `src/lib/account.ts`. Names, emails, keys, usage numbers,
// the $4.18 invoice, the `cvt_live_` key prefix and the card are all made up to match
// the Paper design. Delete this file once the real API exists.

import type { AccountSettings, ApiOverview, Billing, Overview } from "./types";

import avatarUrl from "#/components/app/assets/placeholder-avatar.png";

const account = {
  name: "Leo",
  email: "leo@example.com",
  emailVerified: true,
  avatarUrl,
};

const macs = [
  { id: "mac_1", name: "Leo's MacBook Pro", os: "macOS 26.1", lastSeen: "today" },
  { id: "mac_2", name: "Studio Mac mini", os: "macOS 15.6", lastSeen: "Sep 28" },
];

export const placeholderOverview: Overview = {
  account,
  plan: { name: "Pro", priceLabel: "$8/mo, billed yearly", renewsOn: "2027-10-02" },
  license: {
    id: "lic_pro",
    product: "Pro",
    maskedKey: "CNVT-••••-••••-7Q4M",
    activeMacs: 2,
    updatesLabel: "Updates included",
    detail: "Pro, yearly",
  },
  api: { conversionsThisMonth: 1284, since: "2026-10-01", keyCount: 2 },
  macs,
};

/** Licenses on the account, newest first. */
export const placeholderLicenses = [
  placeholderOverview.license,
  {
    id: "lic_desktop",
    product: "Desktop" as const,
    maskedKey: "CNVT-••••-••••-K2PD",
    activeMacs: 0,
    updatesLabel: "Updates until Aug 20, 2027",
    detail: "Desktop License, bought Aug 20, 2026",
  },
];

export const placeholderMacs = macs;

export const placeholderBilling: Billing = {
  plan: {
    name: "Pro, yearly",
    status: "active",
    summary:
      "$96 a year. Renews Oct 2, 2027. Includes the desktop app on your Macs, every update while you're subscribed, and API access.",
    interval: "year",
  },
  card: { brand: "VISA", last4: "4242", expires: "09/28" },
  receiptEmail: "leo@example.com",
  invoices: [
    { id: "in_3", date: "2026-10-02", description: "Pro, yearly", amountCents: 9600 },
    { id: "in_2", date: "2026-10-01", description: "API usage, Sep 1 to Sep 30", amountCents: 418 },
    {
      id: "in_1",
      date: "2026-08-20",
      description: "Desktop License, 12 months of updates",
      amountCents: 2900,
    },
  ],
};

// Thirty days ending Oct 2, scaled so they add up to the 9,412 in the design.
const perDayCounts = [
  224, 260, 177, 71, 59, 307, 342, 277, 360, 325, 94, 83, 372, 413, 390, 348, 425, 106, 89, 401,
  472, 437, 455, 407, 118, 100, 496, 520, 629, 655,
];

export const placeholderApi: ApiOverview = {
  thisMonth: 1284,
  since: "2026-10-01",
  last30Days: 9412,
  failed: 37,
  perDay: perDayCounts.map((conversions, i) => ({
    date: new Date(Date.UTC(2026, 8, 3 + i)).toISOString().slice(0, 10),
    conversions,
  })),
  keys: [
    {
      id: "key_1",
      name: "Production",
      maskedKey: "cvt_live_8f3a••••••••",
      created: "2026-08-22",
      lastUsed: "2 minutes ago",
    },
    {
      id: "key_2",
      name: "Local testing",
      maskedKey: "cvt_live_21cd••••••••",
      created: "2026-09-09",
      lastUsed: "Sep 28, 2026",
    },
  ],
};

export const placeholderSettings: AccountSettings = {
  account,
  methods: [
    { id: "email", label: "Email link", identity: "leo@example.com", removable: false },
    { id: "github", label: "GitHub", identity: "leoisadev1", removable: true },
    { id: "google", label: "Google", identity: null, removable: true },
    { id: "apple", label: "Apple", identity: null, removable: true },
  ],
  sessions: [
    {
      id: "ses_1",
      name: "Safari on MacBook Pro",
      current: true,
      kind: "Web dashboard",
      lastSeen: "Active now",
    },
    {
      id: "ses_2",
      name: "Leo's MacBook Pro",
      current: false,
      kind: "convt app, macOS 26.1",
      lastSeen: "Seen today",
    },
    {
      id: "ses_3",
      name: "Studio Mac mini",
      current: false,
      kind: "convt app, macOS 15.6",
      lastSeen: "Seen Sep 28",
    },
  ],
};
