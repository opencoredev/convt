import { describe, expect, test } from "bun:test";

import {
  apiSpendLine,
  billingCardCopy,
  billingHasNoPlan,
  showGetDesktop,
} from "../../src/lib/billing-display";
import {
  accountView,
  apiKeyView,
  billingView,
  browserLabel,
  checkoutGiveUp,
  checkoutView,
  lastUsedLabel,
  licenseView,
  licensesView,
  maskLicense,
  overviewView,
  seenLabel,
  settingsView,
  type LicenseRow,
  type SubscriptionRow,
} from "../../src/server/views";

const now = new Date("2026-10-04T12:00:00Z");
const days = (n: number) => new Date(now.getTime() + n * 86_400_000);
const user = { id: "usr_1", name: "", email: "dana@convt.test", emailVerified: true, image: null };

const sub = (v: Partial<SubscriptionRow>): SubscriptionRow => ({
  id: "sub_1",
  kind: "pro",
  interval: "year",
  status: "active",
  trialEndsAt: null,
  currentPeriodEnd: new Date("2027-10-02T00:00:00Z"),
  cancelAtPeriodEnd: false,
  endedAt: null,
  spendCapCents: null,
  createdAt: days(-2),
  ...v,
});

const lic = (v: Partial<LicenseRow>): LicenseRow => ({
  id: "lic_00000000000000000000007q4m",
  plan: "pro",
  trial: false,
  issuedOn: "2026-10-02",
  updatesUntil: "2027-10-02",
  revokedAt: null,
  orderPaidAt: null,
  subscriptionInterval: "year",
  ...v,
});

const desktop = lic({
  id: "lic_000000000000000000000k2pd",
  plan: "desktop",
  issuedOn: "2026-08-20",
  updatesUntil: "2027-08-20",
  orderPaidAt: new Date("2026-08-20T10:00:00Z"),
  subscriptionInterval: null,
});

describe("labels", () => {
  test("the header name falls back to the email's local part", () => {
    expect(accountView(user).name).toBe("dana");
    expect(accountView({ ...user, name: " Dana " }).name).toBe("Dana");
  });

  test("masked keys show only the end of the id", () => {
    expect(maskLicense("lic_00000000000000000000007q4m")).toBe("CNVT-••••-••••-7Q4M");
  });

  test("seen and last-used", () => {
    expect(seenLabel(now, now)).toBe("today");
    expect(seenLabel(days(-1), now)).toBe("yesterday");
    expect(seenLabel(new Date("2026-09-28T08:00:00Z"), now)).toBe("Sep 28");
    expect(seenLabel(null, now)).toBe("never");
    expect(lastUsedLabel(new Date(now.getTime() - 2 * 60_000), now)).toBe("2 minutes ago");
    expect(lastUsedLabel(new Date(now.getTime() - 3 * 3_600_000), now)).toBe("3 hours ago");
    expect(lastUsedLabel(new Date("2026-09-28T08:00:00Z"), now)).toBe("Sep 28, 2026");
    expect(lastUsedLabel(null, now)).toBe("Never");
  });

  test("license lines", () => {
    expect(licenseView(desktop, 0, now)).toMatchObject({
      product: "Desktop",
      updatesLabel: "Updates until Aug 20, 2027",
      detail: "Desktop License, bought Aug 20, 2026",
      revoked: false,
    });
    expect(licenseView(lic({}), 2, now)).toMatchObject({
      product: "Pro",
      detail: "Pro, yearly",
      activeMacs: 2,
      updatesLabel: "Updates included",
    });
    expect(licenseView(lic({ trial: true, updatesUntil: "2026-10-07" }), 0, now).updatesLabel).toBe(
      "Trial until Oct 7, 2026",
    );
    expect(
      licenseView(lic({ updatesUntil: "2026-09-14", subscriptionInterval: "month" }), 0, now),
    ).toMatchObject({
      updatesLabel: "Builds up to Sep 14, 2026",
      detail: "Pro, monthly",
    });
    expect(licenseView(lic({ revokedAt: days(-1) }), 0, now)).toMatchObject({
      revoked: true,
      updatesLabel:
        "Refunded on Oct 3, 2026. It won't be renewed or reissued. Copies already activated offline keep working.",
    });
  });

  test("API keys are masked from their prefix", () => {
    expect(
      apiKeyView(
        {
          id: "key_1",
          name: "Production",
          prefix: "cvt_live_8f3a2b1c",
          createdAt: days(-40),
          lastUsedAt: now,
        },
        now,
      ),
    ).toEqual({
      id: "key_1",
      name: "Production",
      maskedKey: "cvt_live_8f3a2b1c••••",
      created: "2026-08-25",
      lastUsed: "Just now",
    });
  });

  test("browser names", () => {
    expect(
      browserLabel(
        "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/18.0 Safari/605.1.15",
      ),
    ).toBe("Safari on macOS");
    expect(
      browserLabel(
        "Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/140.0.0.0 Safari/537.36",
      ),
    ).toBe("Chrome on Linux");
    expect(
      browserLabel(
        "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/140.0.0.0 Safari/537.36 Edg/140.0",
      ),
    ).toBe("Edge on Windows");
    expect(
      browserLabel(
        "Mozilla/5.0 (iPhone; CPU iPhone OS 18_0 like Mac OS X) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/18.0 Mobile/15E148 Safari/604.1",
      ),
    ).toBe("Safari on iOS");
    expect(browserLabel(null)).toBe("Browser");
  });
});

describe("overview per state", () => {
  const base = {
    user,
    devices: [],
    apiMonth: { count: 0, since: new Date("2026-10-01T00:00:00Z") },
    apiKeyCount: 0,
    now,
  };

  test("new: no plan, license or API", () => {
    const o = overviewView({ ...base, subscriptions: [], licenses: [] });
    expect(o).toMatchObject({ state: "new", plan: null, license: null, api: null });
  });

  test("trial", () => {
    const o = overviewView({
      ...base,
      subscriptions: [
        sub({
          interval: "month",
          status: "trialing",
          trialEndsAt: days(3),
          currentPeriodEnd: days(3),
        }),
      ],
      licenses: [lic({ trial: true, updatesUntil: "2026-10-07", subscriptionInterval: "month" })],
    });
    expect(o.state).toBe("trial");
    expect(o.plan).toEqual({
      name: "Pro trial",
      priceLabel: "Then $12/mo",
      meta: "Trial ends Oct 7, 2026",
    });
    expect(o.license?.detail).toBe("Pro, free trial");
  });

  test("desktop", () => {
    const o = overviewView({
      ...base,
      subscriptions: [],
      licenses: [desktop],
      devices: [{ id: "dev_1", name: "Mac", os: "macOS 26", lastSeenAt: now }],
    });
    expect(o.state).toBe("desktop");
    expect(o.plan).toEqual({
      name: "Desktop",
      priceLabel: "$29, paid once",
      meta: "Updates until Aug 20, 2027",
    });
    expect(o.license?.activeMacs).toBe(1);
    expect(o.macs).toEqual([{ id: "dev_1", name: "Mac", os: "macOS 26", lastSeen: "today" }]);
  });

  test("pro with API and an older Desktop key", () => {
    const o = overviewView({
      ...base,
      subscriptions: [sub({}), sub({ id: "sub_api", kind: "api", interval: null })],
      licenses: [lic({}), desktop],
      apiMonth: { count: 1284, since: new Date("2026-10-01T00:00:00Z") },
      apiKeyCount: 2,
    });
    expect(o.state).toBe("pro");
    expect(o.plan).toEqual({
      name: "Pro",
      priceLabel: "$8/mo, billed yearly",
      meta: "Renews Oct 2, 2027",
    });
    expect(o.license?.product).toBe("Pro");
    expect(o.api).toEqual({ conversionsThisMonth: 1284, since: "2026-10-01", keyCount: 2 });
  });

  test("lapsed", () => {
    const o = overviewView({
      ...base,
      subscriptions: [
        sub({ interval: "month", status: "canceled", endedAt: new Date("2026-09-14T00:00:00Z") }),
      ],
      licenses: [lic({ updatesUntil: "2026-09-14", subscriptionInterval: "month" })],
    });
    expect(o.state).toBe("pro_lapsed");
    expect(o.plan).toEqual({ name: "Pro", priceLabel: "Not renewed", meta: "Ended Sep 14, 2026" });
    expect(o.license?.updatesLabel).toBe("Builds up to Sep 14, 2026");
  });

  test("api only", () => {
    const o = overviewView({
      ...base,
      subscriptions: [sub({ kind: "api", interval: null })],
      licenses: [],
      apiKeyCount: 1,
    });
    expect(o).toMatchObject({ state: "api_only", plan: null, license: null });
    expect(o.api?.keyCount).toBe(1);
  });

  test("the Macs count only on the license the app uses", () => {
    const view = licensesView(
      [sub({})],
      [lic({}), desktop],
      [{ id: "d", name: "M", os: "macOS", lastSeenAt: now }],
      now,
    );
    expect(view.licenses.map((l) => [l.product, l.activeMacs])).toEqual([
      ["Pro", 1],
      ["Desktop", 0],
    ]);
  });
});

describe("billing per state", () => {
  const invoices = [
    {
      id: "inv_1",
      description: "Pro, yearly",
      amountCents: 9600,
      issuedAt: new Date("2026-10-02T00:00:00Z"),
      status: "paid",
    },
  ];
  const desktopInvoice = (amountCents: number) => ({
    id: "inv_desk",
    description: "Desktop License, 12 months of updates",
    amountCents,
    issuedAt: new Date("2026-10-07T00:00:00Z"),
    status: "paid",
  });
  const bv = (
    subscriptions: SubscriptionRow[],
    inv = [] as typeof invoices,
    openApiCheckout = false,
    licenses: LicenseRow[] = [],
  ) =>
    billingView({
      user,
      subscriptions,
      licenses,
      invoices: inv,
      card: null,
      openApiCheckout,
      now,
    });

  test("no purchase", () => {
    const b = bv([]);
    expect(b).toMatchObject({
      plan: null,
      hadPro: false,
      ownsDesktop: false,
      api: { state: "none", spendCapCents: null, endsOn: null },
      card: null,
      receiptEmail: "dana@convt.test",
      invoices: [],
    });
    expect(billingHasNoPlan(b)).toBe(true);
    expect(showGetDesktop(b)).toBe(true);
    expect(billingCardCopy(b)).toBe("No card on file.");
  });

  test("Desktop paid", () => {
    const b = bv([], [desktopInvoice(2900)], false, [desktop]);
    expect(b.plan).toEqual({
      kind: "desktop",
      name: "Desktop (lifetime)",
      status: "active",
      interval: null,
      cancelsOn: null,
      summary:
        "Paid once. Updates until Aug 20, 2027. Your license is on this account and works offline.",
    });
    expect(b.ownsDesktop).toBe(true);
    expect(b.invoices).toEqual([
      {
        id: "inv_desk",
        date: "2026-10-07",
        description: "Desktop License, 12 months of updates",
        amountCents: 2900,
        statusLabel: null,
      },
    ]);
    expect(billingHasNoPlan(b)).toBe(false);
    expect(showGetDesktop(b)).toBe(false);
    expect(billingCardCopy(b)).toBe("No card needed.");
  });

  test("Desktop $0 order", () => {
    const b = bv([], [desktopInvoice(0)], false, [desktop]);
    expect(b.plan).toMatchObject({
      kind: "desktop",
      name: "Desktop (lifetime)",
      status: "active",
    });
    expect(b.ownsDesktop).toBe(true);
    expect(b.invoices[0]?.amountCents).toBe(0);
    expect(billingHasNoPlan(b)).toBe(false);
    expect(showGetDesktop(b)).toBe(false);
    expect(billingCardCopy(b)).toBe("No card needed.");
  });

  test("active yearly keeps the design's summary", () => {
    const b = bv([sub({})], invoices);
    expect(b.plan).toEqual({
      kind: "pro",
      name: "Pro, yearly",
      status: "active",
      interval: "year",
      cancelsOn: null,
      summary:
        "$96 a year. Renews Oct 2, 2027. Includes the desktop app on your Macs, every update while you're subscribed, and API access.",
    });
    expect(b.ownsDesktop).toBe(false);
    expect(showGetDesktop(b)).toBe(true);
    expect(b.invoices).toEqual([
      {
        id: "inv_1",
        date: "2026-10-02",
        description: "Pro, yearly",
        amountCents: 9600,
        statusLabel: null,
      },
    ]);
  });

  test("Pro trialing", () => {
    const b = bv([sub({ interval: "month", status: "trialing", trialEndsAt: days(3) })]);
    expect(b.plan).toMatchObject({
      kind: "pro",
      name: "Pro, monthly",
      status: "trialing",
      interval: "month",
    });
    expect(b.plan?.summary).toStartWith("Free until Oct 7, 2026, then $12 a month.");
    expect(b.ownsDesktop).toBe(false);
    expect(billingHasNoPlan(b)).toBe(false);
    expect(showGetDesktop(b)).toBe(true);
    expect(billingCardCopy(b)).toBe("No card on file.");
  });

  test("trialing, past due, cancelling and lapsed", () => {
    expect(
      bv([sub({ interval: "month", status: "trialing", trialEndsAt: days(3) })]).plan?.summary,
    ).toStartWith("Free until Oct 7, 2026, then $12 a month.");
    expect(
      bv([
        sub({
          interval: "month",
          status: "trialing",
          trialEndsAt: days(3),
          cancelAtPeriodEnd: true,
        }),
      ]).plan?.summary,
    ).toBe("Free until Oct 7, 2026, then the trial ends. You won't be charged.");
    expect(bv([sub({ status: "past_due" })]).plan?.status).toBe("past_due");
    const canceling = bv([sub({ cancelAtPeriodEnd: true })]).plan;
    expect(canceling?.summary).toContain("won't renew");
    expect(canceling?.cancelsOn).toBe("2027-10-02");
    const lapsed = bv([
      sub({ interval: "month", status: "canceled", endedAt: new Date("2026-09-14T00:00:00Z") }),
    ]);
    expect(lapsed.plan).toMatchObject({
      name: "Pro, monthly",
      status: "canceled",
      cancelsOn: null,
    });
    expect(lapsed.plan?.summary).toStartWith("Ended Sep 14, 2026.");
    expect(lapsed.hadPro).toBe(true);
  });

  test("an unfinished checkout is not a plan", () => {
    expect(bv([sub({ status: "incomplete", currentPeriodEnd: null })]).plan).toBeNull();
  });

  test("refunded and failed invoices are labeled", () => {
    const b = bv(
      [],
      [
        { ...invoices[0], status: "refunded" },
        { ...invoices[0], id: "inv_2", status: "open" },
      ],
    );
    expect(b.invoices.map((i) => i.statusLabel)).toEqual(["Refunded", "Payment failed"]);
  });

  test("API enrollment: pending, enrolled, payment failed, ended", () => {
    const api = (v: Partial<SubscriptionRow>) =>
      sub({ kind: "api", interval: null, spendCapCents: 5000, ...v });
    expect(bv([], [], true).api.state).toBe("pending");
    expect(billingHasNoPlan(bv([], [], true))).toBe(false);
    expect(bv([api({ cardSeenAt: null })]).api.state).toBe("pending");
    expect(bv([api({ status: "incomplete" })]).api.state).toBe("pending");
    const enrolled = bv([api({ cardSeenAt: days(-1) })]);
    expect(enrolled.plan).toBeNull();
    expect(enrolled.api).toEqual({ state: "enrolled", spendCapCents: 5000, endsOn: null });
    expect(billingHasNoPlan(enrolled)).toBe(false);
    expect(apiSpendLine(enrolled.api)).toBe("Spend cap $50.00 a month");
    expect(bv([api({ cardSeenAt: days(-1), cancelAtPeriodEnd: true })]).api.endsOn).toBe(
      "2027-10-02",
    );
    expect(bv([api({ status: "past_due", cardSeenAt: days(-1) })]).api.state).toBe(
      "payment_failed",
    );
    expect(bv([api({ status: "canceled", endedAt: days(-1) })]).api.state).toBe("ended");
    expect(billingHasNoPlan(bv([api({ status: "canceled", endedAt: days(-1) })]))).toBe(true);
  });

  test("a Pro trial plus enrolled API is never an empty plan", () => {
    const b = bv([
      sub({
        interval: "month",
        status: "trialing",
        trialEndsAt: days(3),
        currentPeriodEnd: days(3),
      }),
      sub({
        id: "sub_api",
        kind: "api",
        interval: null,
        spendCapCents: 2500,
        cardSeenAt: days(-1),
      }),
    ]);
    expect(b.plan).toMatchObject({
      name: "Pro, monthly",
      status: "trialing",
    });
    expect(b.plan?.summary).toStartWith("Free until Oct 7, 2026");
    expect(b.api).toEqual({ state: "enrolled", spendCapCents: 2500, endsOn: null });
    expect(billingHasNoPlan(b)).toBe(false);
    expect(apiSpendLine(b.api)).toBe("Spend cap $25.00 a month");
  });

  test("an incomplete Polar trial with a future trial end still shows as a trial", () => {
    const b = bv([
      sub({
        interval: "month",
        status: "incomplete",
        trialEndsAt: days(3),
        currentPeriodEnd: days(3),
      }),
    ]);
    expect(b.plan?.status).toBe("trialing");
    expect(b.plan?.summary).toStartWith("Free until Oct 7, 2026");
    expect(billingHasNoPlan(b)).toBe(false);
  });

  test("empty and API-ended accounts still have no plan; Desktop does not", () => {
    expect(billingHasNoPlan(bv([]))).toBe(true);
    expect(
      billingHasNoPlan({
        plan: null,
        api: { state: "none", spendCapCents: null, endsOn: null },
      }),
    ).toBe(true);
    expect(
      billingHasNoPlan({
        plan: null,
        api: { state: "ended", spendCapCents: 2500, endsOn: null },
      }),
    ).toBe(true);
    expect(billingHasNoPlan(bv([], [desktopInvoice(0)], false, [desktop]))).toBe(false);
  });

  test("Pro wins over Desktop when both exist", () => {
    const b = bv(
      [sub({ interval: "month", status: "trialing", trialEndsAt: days(3) })],
      [],
      false,
      [desktop],
    );
    expect(b.plan).toMatchObject({ kind: "pro", status: "trialing" });
    expect(b.ownsDesktop).toBe(true);
    expect(showGetDesktop(b)).toBe(false);
  });
});

describe("revoked licenses", () => {
  test("a refunded or disputed key shows the badge and the date instead of the update window", () => {
    const refunded = licenseView(
      lic({
        plan: "desktop",
        revokedAt: new Date("2026-10-05T10:00:00Z"),
        revokeReason: "refunded",
      }),
      0,
      now,
    );
    expect(refunded).toMatchObject({
      revoked: true,
      revokedBadge: "REFUNDED",
      updatesLabel:
        "Refunded on Oct 5, 2026. It won't be renewed or reissued. Copies already activated offline keep working.",
    });
    const disputed = licenseView(
      lic({
        plan: "desktop",
        revokedAt: new Date("2026-10-05T10:00:00Z"),
        revokeReason: "dispute_lost",
      }),
      0,
      now,
    );
    expect(disputed.revokedBadge).toBe("DISPUTED");
    expect(disputed.updatesLabel).toStartWith("Disputed on Oct 5, 2026.");
  });
});

describe("settings", () => {
  test("methods, sessions and devices", () => {
    const view = settingsView({
      user,
      accounts: [{ id: "acc_1", providerId: "github", accountId: "100200300" }],
      sessions: [
        {
          id: "ses_other",
          userAgent: "Mozilla/5.0 (X11; Linux x86_64) Chrome/140.0",
          createdAt: days(-3),
          updatedAt: days(-1),
        },
        {
          id: "ses_me",
          userAgent: "Mozilla/5.0 (Macintosh; Mac OS X 10_15_7) Version/18.0 Safari/605.1.15",
          createdAt: now,
          updatedAt: now,
        },
      ],
      devices: [
        {
          id: "dev_1",
          name: "Studio Mac mini",
          os: "macOS 15.6",
          lastSeenAt: new Date("2026-09-28T10:00:00Z"),
        },
      ],
      currentSessionId: "ses_me",
      now,
    });
    expect(view.methods.map((m) => [m.id, m.identity, m.removable, m.accountId])).toEqual([
      ["email", "dana@convt.test", false, null],
      ["github", "ID 100200300", true, "acc_1"],
      ["google", null, true, null],
      ["apple", null, true, null],
    ]);
    expect(view.sessions).toEqual([
      {
        id: "ses_me",
        type: "web",
        name: "Safari on macOS",
        current: true,
        kind: "Web dashboard",
        lastSeen: "Active now",
      },
      {
        id: "ses_other",
        type: "web",
        name: "Chrome on Linux",
        current: false,
        kind: "Web dashboard",
        lastSeen: "Seen yesterday",
      },
      {
        id: "dev_1",
        type: "device",
        name: "Studio Mac mini",
        current: false,
        kind: "convt app, macOS 15.6",
        lastSeen: "Seen Sep 28",
      },
    ]);
  });
});

test("the activation link carries the token unchanged", async () => {
  const { activationUrl } = await import("../../src/lib/activate");
  const token = "eyJpZCI6ImxpY18xIn0.c2lnbmF0dXJlLV8";
  expect(activationUrl(token)).toBe(`convt://activate?key=${token}`);
  expect(decodeURIComponent(new URL(activationUrl(token)).searchParams.get("key")!)).toBe(token);
});

describe("checkout success", () => {
  test("a Pro trial stays a trial view, including after the page gives up polling", () => {
    expect(checkoutView({ state: "trial", product: "pro_month", allowTrial: true })).toEqual({
      state: "trial",
      product: "pro",
      allowTrial: true,
    });
    expect(checkoutGiveUp("pro", true)).toEqual({
      state: "trial",
      product: "pro",
      allowTrial: true,
    });
  });

  test("a paid Desktop checkout that times out still says the key is coming by email", () => {
    expect(checkoutView({ state: "pending", product: "desktop" })).toEqual({
      state: "pending",
      product: "desktop",
      allowTrial: false,
    });
    expect(checkoutGiveUp("desktop", false)).toEqual({ state: "email" });
    expect(checkoutGiveUp("pro", false)).toEqual({ state: "email" });
  });
});
