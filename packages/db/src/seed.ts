// Fixture accounts for local development and the e2e checks, all on the reserved
// `.test` domain. Dates are relative to the seed run. Every row has a fixed id
// derived from its fixture name, so seeding again updates the fixtures in place;
// usage rows are keyed by day, so a later seed adds the new days and keeps the old.
// License tokens are real: they are signed with the local dev key in
// .convt-dev/license.key (the dev-keys format), which a local build of the app and
// CLI embeds. Never point this at a real database: the CLI runs the ownership guard.

import { createHash } from "node:crypto";
import { chmodSync, existsSync, mkdirSync, readFileSync, writeFileSync } from "node:fs";
import { join } from "node:path";

import {
  base64urlEncode,
  encodeId128,
  hashApiKey,
  importSigningKey,
  parseSeed,
  publicKeyOf,
  sign,
  type IdPrefix,
} from "@convt/license";

import type { Db } from "./client";
import { devDir } from "./env";
import * as t from "./schema";

const day = 86_400_000;

/** A stable id for a fixture row. */
export function fixtureId(prefix: IdPrefix, key: string): string {
  const digest = createHash("sha256").update(`convt-seed:${key}`).digest();
  return `${prefix}_${encodeId128(new Uint8Array(digest.subarray(0, 16)))}`;
}

const isoDate = (d: Date) => d.toISOString().slice(0, 10);

function addYears(date: string, years: number): string {
  const [y, m, d] = date.split("-").map(Number);
  const out = new Date(Date.UTC(y + years, m - 1, d));
  // Feb 29 plus a year is Feb 28, not Mar 1.
  if (out.getUTCMonth() !== m - 1) out.setUTCDate(0);
  return isoDate(out);
}

/** The GitHub identity the OAuth mock signs in as `github-pro`; linked to pro@. */
export const proGithubAccountId = "100200300";

export const fixtures = {
  new: "new@convt.test",
  trial: "trial@convt.test",
  desktop: "desktop@convt.test",
  pro: "pro@convt.test",
  lapsed: "lapsed@convt.test",
  api: "api@convt.test",
  unclaimed: "unclaimed@convt.test",
  refunded: "refunded@convt.test",
  pastdue: "pastdue@convt.test",
  disputed: "disputed@convt.test",
  apipending: "apipending@convt.test",
} as const;

/**
 * The local catalog's provider ids (packages/billing/src/catalog.ts, environment
 * `local`, which the billing mock serves). A billing test checks they match.
 */
export const localPriceIds = {
  desktop: "price_local_desktop",
  pro_month: "price_local_pro_month",
  pro_year: "price_local_pro_year",
  api: "price_local_api",
} as const;

/** The provider customer id the seed gives a fixture user, which the billing mock knows. */
export const fixtureCustomerId = (k: string) => `seed_cus_${k}`;

/** The dev signing key, created in the dev-keys format if it does not exist. */
export async function devSigningKey(): Promise<CryptoKey> {
  const file = join(devDir, "license.key");
  if (!existsSync(file)) {
    mkdirSync(devDir, { recursive: true, mode: 0o700 });
    const seed = crypto.getRandomValues(new Uint8Array(32));
    writeFileSync(file, `${base64urlEncode(seed)}\n`, { mode: 0o600, flag: "wx" });
    chmodSync(file, 0o600);
    const key = await importSigningKey(seed);
    writeFileSync(join(devDir, "license.pub"), `${await publicKeyOf(key)}\n`);
    return key;
  }
  return importSigningKey(parseSeed(readFileSync(file, "utf8")));
}

type LicenseInput = {
  key: string;
  email: string;
  userId: string | null;
  plan: "desktop" | "pro";
  trial?: boolean;
  orderId?: string;
  subscriptionId?: string;
  invoiceId?: string;
  periodStart?: string;
  issuedOn: string;
  updatesUntil: string;
  revoked?: { at: Date; reason: "refunded" | "dispute_lost" };
};

export async function runSeed(db: Db, now: Date, signingKey?: CryptoKey): Promise<string> {
  const key = signingKey ?? (await devSigningKey());
  const today = isoDate(now);
  const at = (days: number) => new Date(now.getTime() + days * day);
  const dateAt = (days: number) => isoDate(at(days));
  const monthStart = new Date(Date.UTC(now.getUTCFullYear(), now.getUTCMonth(), 1));
  const counts = { users: 0, licenses: 0, usage: 0 };

  await db.transaction(async (tx) => {
    const user = async (k: keyof typeof fixtures, name: string, createdDaysAgo: number) => {
      const id = fixtureId("usr", k);
      await tx
        .insert(t.users)
        .values({
          id,
          name,
          email: fixtures[k],
          emailVerified: true,
          createdAt: at(-createdDaysAgo),
          updatedAt: now,
        })
        .onConflictDoUpdate({
          target: t.users.id,
          set: { name, email: fixtures[k], emailVerified: true, updatedAt: now },
        });
      counts.users++;
      return id;
    };

    const license = async (l: LicenseInput) => {
      const id = fixtureId("lic", l.key);
      const token = await sign(
        { id, email: l.email, plan: l.plan, issued: l.issuedOn, updates_until: l.updatesUntil },
        key,
      );
      const values = {
        id,
        userId: l.userId,
        email: l.email,
        plan: l.plan,
        trial: l.trial ?? false,
        orderId: l.orderId ?? null,
        subscriptionId: l.subscriptionId ?? null,
        invoiceId: l.invoiceId ?? null,
        periodStart: l.periodStart ?? null,
        issuedOn: l.issuedOn,
        updatesUntil: l.updatesUntil,
        token,
        revokedAt: l.revoked?.at ?? null,
        revokeReason: l.revoked?.reason ?? null,
        updatedAt: now,
      };
      await tx
        .insert(t.licenses)
        .values(values)
        .onConflictDoUpdate({ target: t.licenses.id, set: values });
      counts.licenses++;
      return id;
    };

    const order = async (
      k: string,
      userId: string | null,
      email: string,
      paidDaysAgo: number,
      refund?: { status: "refunded" | "partially_refunded"; cents: number; daysAgo: number },
    ) => {
      const id = fixtureId("ord", k);
      const values = {
        id,
        provider: "polar",
        providerOrderId: `seed_${k}`,
        providerCustomerId: userId ? fixtureCustomerId(k.split("-")[0]) : null,
        userId,
        email,
        product: "desktop",
        amountCents: 2900,
        currency: "usd",
        status: refund?.status ?? "paid",
        paidAt: at(-paidDaysAgo),
        billedAt: at(-paidDaysAgo),
        refundedCents: refund?.cents ?? 0,
        refundedAt: refund ? at(-refund.daysAgo) : null,
        updatedAt: now,
      };
      await tx
        .insert(t.orders)
        .values(values)
        .onConflictDoUpdate({ target: t.orders.id, set: values });
      return id;
    };

    const subscription = async (
      k: string,
      v: Omit<typeof t.subscriptions.$inferInsert, "id" | "providerSubscriptionId">,
    ) => {
      const id = fixtureId("sub", k);
      const values = {
        ...v,
        id,
        providerSubscriptionId: `seed_${k}`,
        providerCustomerId:
          v.providerCustomerId ?? (v.userId ? fixtureCustomerId(k.split("-")[0]) : null),
        trialEndsAt: v.trialEndsAt ?? null,
        canceledAt: v.canceledAt ?? null,
        endedAt: v.endedAt ?? null,
        spendCapCents: v.spendCapCents ?? null,
        cardSeenAt: v.cardSeenAt ?? null,
        updatedAt: now,
      };
      await tx
        .insert(t.subscriptions)
        .values(values)
        .onConflictDoUpdate({ target: t.subscriptions.id, set: values });
      return id;
    };

    const invoice = async (
      k: string,
      v: Omit<
        typeof t.invoices.$inferInsert,
        "id" | "providerInvoiceId" | "currency" | "billedAt" | "netCents"
      > & { netCents?: number },
    ) => {
      const paid = ["paid", "partially_refunded", "refunded"].includes(v.status);
      const values = {
        ...v,
        id: fixtureId("inv", k),
        providerInvoiceId: `seed_${k}`,
        currency: "usd",
        subscriptionId: v.subscriptionId ?? null,
        orderId: v.orderId ?? null,
        billedAt: v.issuedAt,
        paidAt: paid ? v.issuedAt : null,
        netCents: v.netCents ?? v.amountCents,
        refundedCents: v.refundedCents ?? 0,
        reason: v.reason ?? (v.orderId ? "purchase" : "subscription_cycle"),
        updatedAt: now,
      };
      await tx
        .insert(t.invoices)
        .values(values)
        .onConflictDoUpdate({ target: t.invoices.id, set: values });
      return values.id;
    };

    /** Pro coverage: the paid period an invoice bought. */
    const coverage = async (
      k: string,
      invoiceId: string,
      subscriptionId: string,
      product: "pro_month" | "pro_year",
      start: Date,
      end: Date,
      amountCents: number,
    ) => {
      const values = {
        id: fixtureId("cov", k),
        invoiceId,
        providerItemId: `seed_item_${k}`,
        subscriptionId,
        product,
        priceId: localPriceIds[product],
        periodStart: start,
        periodEnd: end,
        amountCents,
        kind: "period",
      };
      await tx
        .insert(t.paymentCoverage)
        .values(values)
        .onConflictDoUpdate({ target: t.paymentCoverage.id, set: values });
    };

    /** The provider customer for a fixture user, with the id the billing mock preloads. */
    const customer = async (k: string, userId: string, email: string) => {
      const values = {
        id: fixtureId("cus", k),
        userId,
        provider: "polar",
        providerCustomerId: fixtureCustomerId(k),
        email,
        updatedAt: now,
      };
      await tx
        .insert(t.billingCustomers)
        .values(values)
        .onConflictDoUpdate({ target: t.billingCustomers.id, set: values });
    };

    const device = async (
      k: string,
      userId: string,
      name: string,
      os: string,
      seenDaysAgo: number,
    ) => {
      const values = {
        id: fixtureId("dev", k),
        userId,
        name,
        os,
        appVersion: "0.1.0",
        lastSeenAt: at(-seenDaysAgo),
        revokedAt: null,
        updatedAt: now,
      };
      await tx
        .insert(t.devices)
        .values(values)
        .onConflictDoUpdate({ target: t.devices.id, set: values });
    };

    const apiKey = async (
      k: string,
      userId: string,
      name: string,
      createdDaysAgo: number,
      usedMinutesAgo: number,
    ) => {
      // Fixture keys are derived from the name, never shown, and only their hash is stored.
      const digest = createHash("sha256").update(`convt-seed-key:${k}`).digest("hex");
      const secret = `cvt_live_${digest.slice(0, 32)}`;
      const values = {
        id: fixtureId("key", k),
        userId,
        name,
        prefix: secret.slice(0, 17),
        secretHash: await hashApiKey(secret),
        createdAt: at(-createdDaysAgo),
        lastUsedAt: new Date(now.getTime() - usedMinutesAgo * 60_000),
        revokedAt: null,
        updatedAt: now,
      };
      await tx
        .insert(t.apiKeys)
        .values(values)
        .onConflictDoUpdate({ target: t.apiKeys.id, set: values });
      return values.id;
    };

    /** One usage row per day: `perDay[0]` is 29 days ago, the last is today. */
    const usage = async (
      k: string,
      userId: string,
      subscriptionId: string,
      keyId: string,
      perDay: number[],
    ) => {
      for (let i = 0; i < perDay.length; i++) {
        const when = at(-(perDay.length - 1 - i));
        const occurredAt = new Date(
          Date.UTC(when.getUTCFullYear(), when.getUTCMonth(), when.getUTCDate(), 12),
        );
        if (occurredAt > now) occurredAt.setTime(now.getTime() - 60_000);
        const dayKey = isoDate(occurredAt);
        const inserted = await tx
          .insert(t.usageEvents)
          .values({
            id: fixtureId("use", `${k}:${dayKey}`),
            userId,
            subscriptionId,
            apiKeyId: keyId,
            jobId: `seed_${k}_${dayKey}`,
            kind: "api_conversion",
            quantity: perDay[i],
            amountCents: Math.round(perDay[i] * 0.05),
            occurredAt,
          })
          .onConflictDoNothing()
          .returning({ id: t.usageEvents.id });
        counts.usage += inserted.length;
      }
    };

    // new@: signed in, bought nothing.
    await user("new", "Nia", 1);

    // trial@: Pro monthly, trialing, ends in 3 days. No key: a trial is not paid
    // coverage, so the desktop app runs on its own local trial. The $0 trial order
    // and its coverage fund nothing.
    {
      const userId = await user("trial", "Theo", 4);
      await customer("trial", userId, fixtures.trial);
      const sub = await subscription("trial-pro", {
        provider: "polar",
        userId,
        email: fixtures.trial,
        kind: "pro",
        interval: "month",
        status: "trialing",
        trialEndsAt: at(3),
        currentPeriodStart: at(-4),
        currentPeriodEnd: at(3),
      });
      const inv = await invoice("trial-start", {
        provider: "polar",
        userId,
        email: fixtures.trial,
        subscriptionId: sub,
        description: "Pro, monthly (free trial)",
        amountCents: 0,
        status: "paid",
        issuedAt: at(-4),
        reason: "subscription_create",
      });
      await coverage("trial-start", inv, sub, "pro_month", at(-4), at(3), 0);
      // Earlier seeds gave trial@ a trial key; a license is never deleted, so an old
      // one stays in a reused database. Fresh databases have none.
    }

    // desktop@: one Desktop purchase and one Mac.
    {
      const userId = await user("desktop", "Dana", 45);
      await customer("desktop", userId, fixtures.desktop);
      const ord = await order("desktop", userId, fixtures.desktop, 45);
      await license({
        key: "desktop",
        email: fixtures.desktop,
        userId,
        plan: "desktop",
        orderId: ord,
        issuedOn: dateAt(-45),
        updatesUntil: addYears(dateAt(-45), 1),
      });
      await invoice("desktop", {
        provider: "polar",
        userId,
        email: fixtures.desktop,
        orderId: ord,
        description: "Desktop License, 12 months of updates",
        amountCents: 2900,
        status: "paid",
        issuedAt: at(-45),
      });
      await device("desktop-mac", userId, "Dana's MacBook Air", "macOS 26.0", 1);
    }

    // pro@: the old sample data. Pro yearly, an older Desktop key, two Macs, API with
    // two keys and 30 days of usage, three invoices, GitHub linked.
    {
      const userId = await user("pro", "Leo", 60);
      await customer("pro", userId, fixtures.pro);
      const pro = await subscription("pro-pro", {
        provider: "polar",
        userId,
        email: fixtures.pro,
        kind: "pro",
        interval: "year",
        status: "active",
        currentPeriodStart: at(-2),
        currentPeriodEnd: new Date(`${addYears(dateAt(-2), 1)}T00:00:00Z`),
      });
      const yearly = await invoice("pro-yearly", {
        provider: "polar",
        userId,
        email: fixtures.pro,
        subscriptionId: pro,
        description: "Pro, yearly",
        amountCents: 9600,
        status: "paid",
        issuedAt: at(-2),
        reason: "subscription_create",
      });
      await coverage(
        "pro-yearly",
        yearly,
        pro,
        "pro_year",
        at(-2),
        new Date(`${addYears(dateAt(-2), 1)}T00:00:00Z`),
        9600,
      );
      await license({
        key: "pro-pro",
        email: fixtures.pro,
        userId,
        plan: "pro",
        subscriptionId: pro,
        invoiceId: yearly,
        periodStart: dateAt(-2),
        issuedOn: dateAt(-2),
        updatesUntil: addYears(dateAt(-2), 1),
      });
      const ord = await order("pro-desktop", userId, fixtures.pro, 45);
      await license({
        key: "pro-desktop",
        email: fixtures.pro,
        userId,
        plan: "desktop",
        orderId: ord,
        issuedOn: dateAt(-45),
        updatesUntil: addYears(dateAt(-45), 1),
      });
      const api = await subscription("pro-api", {
        provider: "polar",
        userId,
        email: fixtures.pro,
        kind: "api",
        interval: null,
        status: "active",
        currentPeriodStart: monthStart,
        spendCapCents: 10_000,
        cardSeenAt: at(-43),
      });
      const production = await apiKey("pro-production", userId, "Production", 43, 2);
      await apiKey("pro-local", userId, "Local testing", 25, 6 * 24 * 60);
      await usage(
        "pro",
        userId,
        api,
        production,
        [
          224, 260, 177, 71, 59, 307, 342, 277, 360, 325, 94, 83, 372, 413, 390, 348, 425, 106, 89,
          401, 472, 437, 455, 407, 118, 100, 496, 520, 629, 655,
        ],
      );
      await invoice("pro-api-usage", {
        provider: "polar",
        userId,
        email: fixtures.pro,
        subscriptionId: api,
        description: "API usage, last month",
        amountCents: 418,
        status: "paid",
        issuedAt: monthStart,
        reason: "subscription_meter_cycle",
      });
      await invoice("pro-desktop", {
        provider: "polar",
        userId,
        email: fixtures.pro,
        orderId: ord,
        description: "Desktop License, 12 months of updates",
        amountCents: 2900,
        status: "paid",
        issuedAt: at(-45),
      });
      await device("pro-mbp", userId, "Leo's MacBook Pro", "macOS 26.1", 0);
      await device("pro-mini", userId, "Studio Mac mini", "macOS 15.6", 6);
      await tx
        .insert(t.accounts)
        .values({
          id: fixtureId("acc", "pro-github"),
          userId,
          providerId: "github",
          accountId: proGithubAccountId,
          scope: "user:email",
          createdAt: at(-30),
          updatedAt: now,
        })
        .onConflictDoUpdate({ target: t.accounts.id, set: { userId, updatedAt: now } });
    }

    // lapsed@: Pro ended last month; the last key covers builds to that day.
    {
      const userId = await user("lapsed", "Lars", 120);
      await customer("lapsed", userId, fixtures.lapsed);
      const ended = at(-20);
      const sub = await subscription("lapsed-pro", {
        provider: "polar",
        userId,
        email: fixtures.lapsed,
        kind: "pro",
        interval: "month",
        status: "canceled",
        currentPeriodStart: at(-50),
        currentPeriodEnd: ended,
        canceledAt: ended,
        endedAt: ended,
      });
      const paid = await invoice("lapsed-paid", {
        provider: "polar",
        userId,
        email: fixtures.lapsed,
        subscriptionId: sub,
        description: "Pro, monthly",
        amountCents: 1200,
        status: "paid",
        issuedAt: at(-50),
      });
      await coverage("lapsed-paid", paid, sub, "pro_month", at(-50), ended, 1200);
      await license({
        key: "lapsed-pro",
        email: fixtures.lapsed,
        userId,
        plan: "pro",
        subscriptionId: sub,
        invoiceId: paid,
        periodStart: dateAt(-50),
        issuedOn: dateAt(-50),
        updatesUntil: dateAt(-20),
      });
      await invoice("lapsed-past-due", {
        provider: "polar",
        userId,
        email: fixtures.lapsed,
        subscriptionId: sub,
        description: "Pro, monthly (payment failed)",
        amountCents: 1200,
        status: "uncollectible",
        issuedAt: at(-20),
        netCents: 1200,
      });
    }

    // api@: API only, one key, usage and some failed jobs.
    {
      const userId = await user("api", "Ada", 40);
      await customer("api", userId, fixtures.api);
      const api = await subscription("api-api", {
        provider: "polar",
        userId,
        email: fixtures.api,
        kind: "api",
        interval: null,
        status: "active",
        currentPeriodStart: monthStart,
        spendCapCents: 5_000,
        cardSeenAt: at(-38),
      });
      const keyId = await apiKey("api-main", userId, "Backend", 38, 30);
      await usage(
        "api",
        userId,
        api,
        keyId,
        [
          12, 30, 41, 0, 0, 22, 35, 50, 44, 61, 0, 0, 70, 66, 52, 49, 58, 0, 0, 80, 77, 64, 59, 71,
          0, 0, 90, 85, 102, 96,
        ],
      );
      for (let i = 0; i < 6; i++) {
        const values = {
          id: fixtureId("job", `api-failed-${i}`),
          userId,
          source: "api",
          apiKeyId: keyId,
          status: "failed",
          inputFormat: "docx",
          targetFormat: "pdf",
          errorCode: "conversion_failed",
          attempt: 3,
          createdAt: at(-(i * 4 + 1)),
          finishedAt: at(-(i * 4 + 1)),
          expiresAt: at(-(i * 4)),
          updatedAt: now,
        };
        await tx
          .insert(t.cloudJobs)
          .values(values)
          .onConflictDoUpdate({ target: t.cloudJobs.id, set: values });
      }
    }

    // refunded@: a Desktop purchase refunded in full; the key shows as revoked.
    {
      const userId = await user("refunded", "Rui", 30);
      await customer("refunded", userId, fixtures.refunded);
      const ord = await order("refunded-desktop", userId, fixtures.refunded, 30, {
        status: "refunded",
        cents: 2900,
        daysAgo: 5,
      });
      await invoice("refunded-desktop", {
        provider: "polar",
        userId,
        email: fixtures.refunded,
        orderId: ord,
        description: "Desktop License, 12 months of updates",
        amountCents: 2900,
        status: "refunded",
        refundedCents: 2900,
        issuedAt: at(-30),
      });
      await license({
        key: "refunded-desktop",
        email: fixtures.refunded,
        userId,
        plan: "desktop",
        orderId: ord,
        issuedOn: dateAt(-30),
        updatesUntil: addYears(dateAt(-30), 1),
        revoked: { at: at(-5), reason: "refunded" },
      });
    }

    // pastdue@: Pro monthly whose renewal failed. Last month's paid key stays.
    {
      const userId = await user("pastdue", "Pia", 70);
      await customer("pastdue", userId, fixtures.pastdue);
      const sub = await subscription("pastdue-pro", {
        provider: "polar",
        userId,
        email: fixtures.pastdue,
        kind: "pro",
        interval: "month",
        status: "past_due",
        currentPeriodStart: at(-2),
        currentPeriodEnd: at(28),
      });
      const paid = await invoice("pastdue-paid", {
        provider: "polar",
        userId,
        email: fixtures.pastdue,
        subscriptionId: sub,
        description: "Pro, monthly",
        amountCents: 1200,
        status: "paid",
        issuedAt: at(-32),
      });
      await coverage("pastdue-paid", paid, sub, "pro_month", at(-32), at(-2), 1200);
      await license({
        key: "pastdue-pro",
        email: fixtures.pastdue,
        userId,
        plan: "pro",
        subscriptionId: sub,
        invoiceId: paid,
        periodStart: dateAt(-32),
        issuedOn: dateAt(-32),
        updatesUntil: dateAt(-2),
      });
      await invoice("pastdue-failed", {
        provider: "polar",
        userId,
        email: fixtures.pastdue,
        subscriptionId: sub,
        description: "Pro, monthly (payment failed)",
        amountCents: 1200,
        status: "open",
        issuedAt: at(-2),
      });
    }

    // disputed@: a Desktop purchase whose chargeback was lost; the key is revoked.
    {
      const userId = await user("disputed", "Dov", 50);
      await customer("disputed", userId, fixtures.disputed);
      const ord = await order("disputed-desktop", userId, fixtures.disputed, 50);
      await invoice("disputed-desktop", {
        provider: "polar",
        userId,
        email: fixtures.disputed,
        orderId: ord,
        description: "Desktop License, 12 months of updates",
        amountCents: 2900,
        status: "paid",
        issuedAt: at(-50),
      });
      const values = {
        id: fixtureId("dsp", "disputed"),
        provider: "polar",
        providerDisputeId: "seed_dsp_disputed",
        orderId: ord,
        status: "lost",
        amountCents: 2900,
        closed: true,
        updatedAt: now,
      };
      await tx
        .insert(t.disputes)
        .values(values)
        .onConflictDoUpdate({ target: t.disputes.id, set: values });
      await license({
        key: "disputed-desktop",
        email: fixtures.disputed,
        userId,
        plan: "desktop",
        orderId: ord,
        issuedOn: dateAt(-50),
        updatesUntil: addYears(dateAt(-50), 1),
        revoked: { at: at(-8), reason: "dispute_lost" },
      });
    }

    // apipending@: API enrollment started, but no card has been seen yet.
    {
      const userId = await user("apipending", "Ari", 2);
      await customer("apipending", userId, fixtures.apipending);
      await subscription("apipending-api", {
        provider: "polar",
        userId,
        email: fixtures.apipending,
        kind: "api",
        interval: null,
        status: "active",
        currentPeriodStart: monthStart,
        spendCapCents: 2_000,
      });
    }

    // unclaimed@: a Desktop purchase with no account yet, for the claim tests.
    {
      const ord = await order("unclaimed", null, fixtures.unclaimed, 10);
      await license({
        key: "unclaimed",
        email: fixtures.unclaimed,
        userId: null,
        plan: "desktop",
        orderId: ord,
        issuedOn: dateAt(-10),
        updatesUntil: addYears(dateAt(-10), 1),
      });
      await invoice("unclaimed", {
        provider: "polar",
        userId: null,
        email: fixtures.unclaimed,
        orderId: ord,
        description: "Desktop License, 12 months of updates",
        amountCents: 2900,
        status: "paid",
        issuedAt: at(-10),
      });
    }
  });

  return `${counts.users} users, ${counts.licenses} licenses, ${counts.usage} new usage rows (as of ${today})`;
}
