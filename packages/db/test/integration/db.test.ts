// Runs through scripts/test-db.sh: `bun run --cwd packages/db test:integration`.
// Tables are written as convt_owner (the seed) and read as convt_web, like the app.

import { afterAll, beforeAll, describe, expect, test } from "bun:test";

import { importSigningKey, importVerifyKey, publicKeyOf, verify } from "@convt/license";
import { eq, sql } from "drizzle-orm";

import { assertOwnedDatabase } from "../../src/guard";
import { appliedMigrations, migrationHashes } from "../../src/migrations";
import {
  activeApiKeys,
  activeDevices,
  apiConversionsThisMonth,
  apiUsagePerDay,
  claimPurchases,
  consumeSendBucket,
  deriveAccountState,
  failedApiJobs,
  getLicenseToken,
  revokeOtherSessions,
  userInvoices,
  userLicenses,
  userSubscriptions,
} from "../../src/queries";
import * as t from "../../src/schema";
import { fixtureId, fixtures, runSeed } from "../../src/seed";
import { freshDatabase, type TestDatabase } from "../../src/testing";

let now = new Date();

/** Awaits a query (Drizzle builders are thenables, not promises) and checks it fails. */
async function expectFails(query: PromiseLike<unknown>, pattern: RegExp) {
  let error: unknown = null;
  try {
    await query;
  } catch (e) {
    error = e;
  }
  expect(error).not.toBeNull();
  const text = `${(error as Error).message} ${String((error as { cause?: unknown }).cause ?? "")}`;
  expect(text).toMatch(pattern);
}
let tdb: TestDatabase;
let seedKey: CryptoKey;

beforeAll(async () => {
  tdb = await freshDatabase();
  seedKey = await importSigningKey(crypto.getRandomValues(new Uint8Array(32)));
  const { db } = await tdb.open("owner");
  now = new Date();
  await runSeed(db, now, seedKey);
});

afterAll(async () => {
  await tdb?.drop();
});

const uid = (k: keyof typeof fixtures) => fixtureId("usr", k);

async function web() {
  return (await tdb.open("web")).db;
}

describe("fixtures", () => {
  test("each fixture derives its state", async () => {
    const db = await web();
    const expected = {
      new: "new",
      trial: "trial",
      desktop: "desktop",
      pro: "pro",
      lapsed: "pro_lapsed",
      api: "api_only",
      refunded: "new",
      pastdue: "pro",
      disputed: "new",
      apipending: "api_only",
    } as const;
    for (const [k, state] of Object.entries(expected)) {
      const id = uid(k as keyof typeof fixtures);
      const subs = await userSubscriptions(db, id);
      const lics = await userLicenses(db, id);
      expect(`${k}: ${deriveAccountState(subs, lics, now)}`).toBe(`${k}: ${state}`);
    }
  });

  test("pro has its sample data", async () => {
    const db = await web();
    const id = uid("pro");
    expect((await userLicenses(db, id)).map((l) => l.plan)).toEqual(["pro", "desktop"]);
    expect((await activeDevices(db, id)).length).toBe(2);
    expect((await activeApiKeys(db, id)).map((k) => k.name)).toEqual([
      "Production",
      "Local testing",
    ]);
    expect((await userInvoices(db, id)).length).toBe(3);
    const perDay = await apiUsagePerDay(db, id, now);
    expect(perDay.length).toBe(30);
    expect(perDay.reduce((n, d) => n + d.conversions, 0)).toBe(9412);
    expect(perDay.at(-1)?.conversions).toBe(655);
    expect((await apiConversionsThisMonth(db, id, now)).count).toBeGreaterThan(0);
  });

  test("api has failed jobs; others have none", async () => {
    const db = await web();
    expect(await failedApiJobs(db, uid("api"), now)).toBe(6);
    expect(await failedApiJobs(db, uid("pro"), now)).toBe(0);
  });

  test("seeded tokens verify with the seed key and carry the license id", async () => {
    const db = await web();
    const [lic] = await userLicenses(db, uid("desktop"));
    const token = await getLicenseToken(db, uid("desktop"), lic.id);
    const vk = await importVerifyKey(await publicKeyOf(seedKey));
    const result = await verify(token!, vk);
    expect(result.ok && result.license.id).toBe(lic.id);
    expect(result.ok && result.license.email).toBe(fixtures.desktop);
  });

  test("seeding again changes nothing it should not", async () => {
    const { db } = await tdb.open("owner");
    const count = async () =>
      (
        await db.execute<{ n: number }>(
          sql`select (select count(*) from users) + (select count(*) from licenses) + (select count(*) from usage_events) + (select count(*) from invoices) as n`,
        )
      ).rows[0].n;
    const before = await count();
    const summary = await runSeed(db, now, seedKey);
    expect(summary).toContain("0 new usage rows");
    expect(await count()).toBe(before);
  });
});

describe("isolation", () => {
  test("two users never see each other's rows", async () => {
    const db = await web();
    const pro = uid("pro");
    const desktop = uid("desktop");
    const proLicenses = new Set((await userLicenses(db, pro)).map((l) => l.id));
    for (const l of await userLicenses(db, desktop)) expect(proLicenses.has(l.id)).toBe(false);
    const proInvoices = new Set((await userInvoices(db, pro)).map((i) => i.id));
    for (const i of await userInvoices(db, desktop)) expect(proInvoices.has(i.id)).toBe(false);
    expect((await activeApiKeys(db, desktop)).length).toBe(0);
    expect((await activeDevices(db, desktop)).every((d) => d.userId === desktop)).toBe(true);
    expect((await apiUsagePerDay(db, desktop, now)).every((d) => d.conversions === 0)).toBe(true);
  });

  test("getLicenseToken refuses another user's license", async () => {
    const db = await web();
    const [proLicense] = await userLicenses(db, uid("pro"));
    expect(await getLicenseToken(db, uid("desktop"), proLicense.id)).toBeNull();
    expect(await getLicenseToken(db, uid("pro"), proLicense.id)).not.toBeNull();
  });

  test("revoking other sessions touches only this user's rows", async () => {
    const { db: owner } = await tdb.open("owner");
    for (const [k, id] of [
      ["pro", "ses_a"],
      ["pro", "ses_b"],
      ["desktop", "ses_c"],
    ] as const) {
      await owner.insert(t.sessions).values({
        id,
        userId: uid(k),
        token: `tok_${id}`,
        expiresAt: new Date(now.getTime() + 86_400_000),
      });
    }
    const db = await web();
    const result = await revokeOtherSessions(db, uid("pro"), "ses_a", now);
    expect(result).toEqual({ sessions: 1, devices: 2 });
    const left = await owner.select({ id: t.sessions.id }).from(t.sessions);
    expect(left.map((s) => s.id).sort()).toEqual(["ses_a", "ses_c"]);
    expect((await activeDevices(db, uid("desktop"))).length).toBe(1);
  });
});

describe("license uniqueness", () => {
  test("a second Desktop key per order and a second Pro key per invoice or paid-through date fail; a reissue is allowed", async () => {
    const { db } = await tdb.open("owner");
    const [desktopLicense] = await db
      .select()
      .from(t.licenses)
      .where(eq(t.licenses.id, fixtureId("lic", "desktop")));
    const copy = { ...desktopLicense, id: "lic_dup_desktop", createdAt: now, updatedAt: now };
    await expectFails(db.insert(t.licenses).values(copy), /licenses_desktop_order_key/);
    await db
      .insert(t.licenses)
      .values({ ...copy, id: "lic_reissue_desktop", reissueOf: desktopLicense.id });

    const [proLicense] = await db
      .select()
      .from(t.licenses)
      .where(eq(t.licenses.id, fixtureId("lic", "pro-pro")));
    const proCopy = { ...proLicense, id: "lic_dup_pro", createdAt: now, updatedAt: now };
    // Same invoice: refused, whatever the dates.
    await expectFails(
      db.insert(t.licenses).values({ ...proCopy, updatesUntil: "2099-12-31" }),
      /licenses_pro_invoice_key/,
    );
    // Same paid-through date for the same period start, from another invoice: refused.
    await expectFails(
      db.insert(t.licenses).values({ ...proCopy, invoiceId: fixtureId("inv", "pro-api-usage") }),
      /licenses_pro_period_until_key/,
    );
    await db
      .insert(t.licenses)
      .values({ ...proCopy, id: "lic_reissue_pro", reissueOf: proLicense.id });
  });

  test("the cases P6's period index refused now pass", async () => {
    const { db } = await tdb.open("owner");
    const sub = fixtureId("sub", "pastdue-pro");
    const [base] = await db
      .select()
      .from(t.licenses)
      .where(eq(t.licenses.id, fixtureId("lic", "pastdue-pro")));
    const extra = async (key: string) =>
      (
        await db
          .insert(t.invoices)
          .values({
            id: `inv_${key}`,
            providerInvoiceId: `p_${key}`,
            email: fixtures.pastdue,
            subscriptionId: sub,
            description: "x",
            amountCents: 1200,
            currency: "usd",
            status: "paid",
            issuedAt: now,
            billedAt: now,
            paidAt: now,
          })
          .returning({ id: t.invoices.id })
      )[0].id;
    // A period whose end moved (a switch) without its start moving: a later key.
    await db.insert(t.licenses).values({
      ...base,
      id: "lic_end_moved",
      invoiceId: await extra("end_moved"),
      updatesUntil: "2099-01-01",
    });
    // Two periods that start on the same UTC date with different ends.
    await db.insert(t.licenses).values({
      ...base,
      id: "lic_same_start",
      invoiceId: await extra("same_start"),
      updatesUntil: "2099-02-01",
    });
  });

  test("the source check rejects a Desktop key without an order", async () => {
    const { db } = await tdb.open("owner");
    const [lic] = await db
      .select()
      .from(t.licenses)
      .where(eq(t.licenses.id, fixtureId("lic", "desktop")));
    await expectFails(
      db.insert(t.licenses).values({ ...lic, id: "lic_bad", orderId: null, reissueOf: null }),
      /licenses_source_check/,
    );
  });
});

describe("claiming", () => {
  test("only a verified account with the email claims; everything claims at once", async () => {
    const { db: owner } = await tdb.open("owner");
    const db = await web();
    const id = "usr_claimtest00000000000000000";
    await db
      .insert(t.users)
      .values({ id, name: "", email: fixtures.unclaimed, emailVerified: false });
    expect(await claimPurchases(db, id)).toEqual({
      orders: 0,
      subscriptions: 0,
      licenses: 0,
      invoices: 0,
    });
    await db.update(t.users).set({ emailVerified: true }).where(eq(t.users.id, id));
    expect(await claimPurchases(db, id)).toEqual({
      orders: 1,
      subscriptions: 0,
      licenses: 1,
      invoices: 1,
    });
    expect((await userLicenses(db, id)).length).toBe(1);
    // A second claim finds nothing new, and other fixtures are untouched.
    expect(await claimPurchases(db, id)).toEqual({
      orders: 0,
      subscriptions: 0,
      licenses: 0,
      invoices: 0,
    });
    const [desktopOrder] = await owner
      .select()
      .from(t.orders)
      .where(eq(t.orders.id, fixtureId("ord", "desktop")));
    expect(desktopOrder.userId).toBe(uid("desktop"));
  });

  test("an unknown user claims nothing", async () => {
    expect(await claimPurchases(await web(), "usr_nobody")).toEqual({
      orders: 0,
      subscriptions: 0,
      licenses: 0,
      invoices: 0,
    });
  });
});

describe("usage facts are append-only", () => {
  const forbidden = /permission denied|append-only|never deleted/;

  for (const role of ["server", "web"] as const) {
    test(`as convt_${role}: rewrites and deletes fail`, async () => {
      const { db } = await tdb.open(role);
      const [row] = await db
        .select()
        .from(t.usageEvents)
        .where(eq(t.usageEvents.userId, uid("api")))
        .limit(1);
      for (const change of [{ quantity: 1 }, { amountCents: 0 }, { jobId: "other" }]) {
        await expectFails(
          db.update(t.usageEvents).set(change).where(eq(t.usageEvents.id, row.id)),
          forbidden,
        );
      }
      await expectFails(db.delete(t.usageEvents).where(eq(t.usageEvents.id, row.id)), forbidden);
    });
  }

  test("reported_at can be set once by convt_server, never twice", async () => {
    const { db } = await tdb.open("server");
    const [row] = await db
      .select()
      .from(t.usageEvents)
      .where(eq(t.usageEvents.userId, uid("api")))
      .limit(1);
    await db
      .update(t.usageEvents)
      .set({ reportedAt: now, providerEventId: "evt_1" })
      .where(eq(t.usageEvents.id, row.id));
    await expectFails(
      db.update(t.usageEvents).set({ reportedAt: new Date() }).where(eq(t.usageEvents.id, row.id)),
      /append-only/,
    );
    await expectFails(
      db
        .update(t.usageEvents)
        .set({ providerEventId: "evt_2" })
        .where(eq(t.usageEvents.id, row.id)),
      /append-only/,
    );
  });

  test("the owner cannot delete either", async () => {
    const { db } = await tdb.open("owner");
    await expectFails(db.delete(t.usageEvents), /never deleted/);
  });

  test("a correction row is accepted, and a job's usage is recorded once", async () => {
    const { db } = await tdb.open("server");
    const [row] = await db
      .select()
      .from(t.usageEvents)
      .where(eq(t.usageEvents.userId, uid("api")))
      .limit(1);
    await db.insert(t.usageEvents).values({
      id: "use_correction_1",
      userId: row.userId,
      jobId: row.jobId,
      kind: "correction",
      quantity: -1,
      amountCents: -5,
      corrects: row.id,
      occurredAt: now,
    });
    await expectFails(
      db
        .insert(t.usageEvents)
        .values({ ...row, id: "use_dup", reportedAt: null, providerEventId: null }),
      /usage_events_job_kind_key/,
    );
  });

  test("deleting the user nulls user_id and keeps the rows", async () => {
    const { db } = await tdb.open("owner");
    const before = await db
      .select({ id: t.usageEvents.id })
      .from(t.usageEvents)
      .where(eq(t.usageEvents.userId, uid("api")));
    expect(before.length).toBeGreaterThan(0);
    await db.delete(t.users).where(eq(t.users.id, uid("api")));
    const ids = before.map((r) => r.id);
    const after = await db.execute<{ n: number; with_user: number }>(
      sql`select count(*)::int as n, count(user_id)::int as with_user from usage_events where id in ${ids}`,
    );
    expect(after.rows[0]).toEqual({ n: ids.length, with_user: 0 });
    const subs = await db
      .select()
      .from(t.subscriptions)
      .where(eq(t.subscriptions.id, fixtureId("sub", "api-api")));
    expect(subs[0].userId).toBeNull();
  });

  test("convt_web and convt_billing cannot delete financial rows", async () => {
    for (const role of ["web", "billing"] as const) {
      const { db } = await tdb.open(role);
      for (const table of [
        t.orders,
        t.licenses,
        t.invoices,
        t.subscriptions,
        t.paymentCoverage,
        t.disputes,
        t.checkouts,
        t.billingCustomers,
      ]) {
        await expectFails(db.delete(table), /permission denied/);
      }
    }
  });
});

describe("billing grants", () => {
  test("convt_web cannot change any financial field", async () => {
    const db = await web();
    const attempts: Array<[string, PromiseLike<unknown>]> = [
      ["order status", db.update(t.orders).set({ status: "refunded" })],
      ["order amount", db.update(t.orders).set({ amountCents: 1 })],
      ["subscription period", db.update(t.subscriptions).set({ currentPeriodEnd: now })],
      ["subscription status", db.update(t.subscriptions).set({ status: "active" })],
      ["spend cap", db.update(t.subscriptions).set({ spendCapCents: 1 })],
      ["invoice amounts", db.update(t.invoices).set({ netCents: 0, refundedCents: 0 })],
      ["license token", db.update(t.licenses).set({ token: "x" })],
      ["license revocation", db.update(t.licenses).set({ revokedAt: null, revokeReason: null })],
      ["coverage row", db.update(t.paymentCoverage).set({ amountCents: 1 })],
      [
        "insert coverage",
        db.insert(t.paymentCoverage).values({
          id: "cov_x",
          invoiceId: fixtureId("inv", "pro-yearly"),
          providerItemId: "x",
          subscriptionId: fixtureId("sub", "pro-pro"),
          product: "pro_year",
          priceId: "x",
          periodStart: now,
          periodEnd: new Date(now.getTime() + 1000),
          amountCents: 1,
          kind: "period",
        }),
      ],
      [
        "insert license",
        db.insert(t.licenses).values({
          id: "lic_x",
          email: "a@b.c",
          plan: "desktop",
          orderId: fixtureId("ord", "desktop"),
          issuedOn: "2026-01-01",
          updatesUntil: "2027-01-01",
          token: "x",
        }),
      ],
      ["dispute status", db.update(t.disputes).set({ status: "won" })],
      ["checkout nonce", db.update(t.checkouts).set({ keyDisclosedAt: now })],
      ["outbox", db.update(t.emailOutbox).set({ status: "sent" })],
      ["webhook events", db.update(t.webhookEvents).set({ status: "processed" })],
      ["deletions", db.update(t.accountDeletions).set({ status: "done" })],
    ];
    for (const [name, query] of attempts) {
      let error = "";
      try {
        await query;
      } catch (e) {
        error = `${(e as Error).message} ${String((e as { cause?: unknown }).cause ?? "")}`;
      }
      expect({ name, denied: /permission denied/.test(error) }).toEqual({ name, denied: true });
    }
  });

  test("convt_web reads the billing tables it shows", async () => {
    const db = await web();
    await db.select().from(t.checkouts);
    await db.select().from(t.billingCustomers);
    await db.select().from(t.disputes);
    await db.select().from(t.accountDeletions);
    await expectFails(db.select().from(t.webhookEvents), /permission denied/);
    await expectFails(db.select().from(t.emailOutbox), /permission denied/);
  });

  test("convt_billing cannot rewrite an issued license or undo a revocation", async () => {
    const { db } = await tdb.open("billing");
    await expectFails(
      db
        .update(t.licenses)
        .set({ token: "x" })
        .where(eq(t.licenses.id, fixtureId("lic", "desktop"))),
      /do not change once issued/,
    );
    await expectFails(
      db
        .update(t.licenses)
        .set({ updatesUntil: "2099-01-01" })
        .where(eq(t.licenses.id, fixtureId("lic", "pro-pro"))),
      /do not change once issued/,
    );
    await expectFails(
      db
        .update(t.licenses)
        .set({ revokedAt: null, revokeReason: null })
        .where(eq(t.licenses.id, fixtureId("lic", "refunded-desktop"))),
      /do not change once issued/,
    );
    // Revoking is allowed once.
    await db
      .update(t.licenses)
      .set({ revokedAt: now, revokeReason: "refunded" })
      .where(eq(t.licenses.id, fixtureId("lic", "unclaimed")));
  });

  test("delete_user is not executable by convt_web and refuses while a subscription is live", async () => {
    const db = await web();
    await expectFails(
      db.execute(sql`select delete_user(${uid("pro")}, 'del_x')`),
      /permission denied/,
    );
    const { db: billing } = await tdb.open("billing");
    await billing
      .insert(t.accountDeletions)
      .values({ id: "del_pro", userId: uid("pro"), status: "deleting" });
    await expectFails(
      billing.execute(sql`select delete_user(${uid("pro")}, 'del_pro')`),
      /still has a live subscription/,
    );
    await expectFails(
      billing.execute(sql`select delete_user(${uid("desktop")}, 'del_pro')`),
      /not in deleting/,
    );
    // With the subscriptions ended, it deletes the user and keeps the financial rows.
    await billing
      .update(t.subscriptions)
      .set({ status: "canceled", endedAt: now })
      .where(eq(t.subscriptions.userId, uid("pro")));
    await billing.execute(sql`select delete_user(${uid("pro")}, 'del_pro')`);
    const { db: owner } = await tdb.open("owner");
    expect(
      (
        await owner
          .select()
          .from(t.users)
          .where(eq(t.users.id, uid("pro")))
      ).length,
    ).toBe(0);
    const [ord] = await owner
      .select()
      .from(t.orders)
      .where(eq(t.orders.id, fixtureId("ord", "pro-desktop")));
    expect({ userId: ord.userId, email: ord.email }).toEqual({ userId: null, email: fixtures.pro });
    const [del] = await owner
      .select()
      .from(t.accountDeletions)
      .where(eq(t.accountDeletions.id, "del_pro"));
    expect(del.status).toBe("done");
  });
});

describe("migration 0001 shapes", () => {
  test("every mapped status inserts, and paid statuses need paid_at", async () => {
    const { db } = await tdb.open("billing");
    for (const status of t.orderStatuses) {
      const paid = ["paid", "partially_refunded", "refunded"].includes(status);
      await db.insert(t.orders).values({
        id: `ord_s_${status}`,
        providerOrderId: `p_s_${status}`,
        email: "s@convt.test",
        product: "desktop",
        amountCents: 2900,
        currency: "usd",
        status,
        billedAt: now,
        paidAt: paid ? now : null,
      });
    }
    await expectFails(
      db.insert(t.orders).values({
        id: "ord_paid_no_paid_at",
        providerOrderId: "p_paid_no_paid_at",
        email: "s@convt.test",
        product: "desktop",
        amountCents: 2900,
        currency: "usd",
        status: "paid",
        billedAt: now,
      }),
      /orders_paid_at_check/,
    );
    for (const status of t.invoiceStatuses) {
      const paid = ["paid", "partially_refunded", "refunded"].includes(status);
      await db.insert(t.invoices).values({
        id: `inv_s_${status}`,
        providerInvoiceId: `p_s_${status}`,
        email: "s@convt.test",
        description: "x",
        amountCents: 0,
        currency: "usd",
        status,
        issuedAt: now,
        billedAt: now,
        paidAt: paid ? now : null,
      });
    }
    for (const status of t.subscriptionStatuses) {
      await db.insert(t.subscriptions).values({
        id: `sub_s_${status}`,
        providerSubscriptionId: `p_s_${status}`,
        email: "s@convt.test",
        kind: "pro",
        interval: "month",
        status,
      });
    }
    await expectFails(
      db.insert(t.subscriptions).values({
        id: "sub_api_no_cap",
        providerSubscriptionId: "p_api_no_cap",
        email: "s@convt.test",
        kind: "api",
        status: "active",
      }),
      /subscriptions_spend_cap_check/,
    );
  });
});

describe("send buckets", () => {
  test("count within the window, restart after it, and hold under concurrency", async () => {
    const db = await web();
    const start = new Date("2026-10-04T00:00:00Z");
    expect(await consumeSendBucket(db, "email:x", 900_000, start)).toBe(1);
    expect(await consumeSendBucket(db, "email:x", 900_000, new Date(start.getTime() + 1000))).toBe(
      2,
    );
    expect(
      await consumeSendBucket(db, "email:x", 900_000, new Date(start.getTime() + 900_000)),
    ).toBe(1);
    const clients = await Promise.all(Array.from({ length: 20 }, () => tdb.open("web")));
    const counts = await Promise.all(
      clients.map(({ db }) => consumeSendBucket(db, "email:y", 900_000, start)),
    );
    expect(counts.sort((a, b) => a - b)).toEqual(Array.from({ length: 20 }, (_, i) => i + 1));
  });
});

describe("migrations and the guard", () => {
  test("the template recorded every migration with its file hash", async () => {
    const { client } = await tdb.open("owner");
    const applied = await appliedMigrations(client);
    expect(applied).toEqual(migrationHashes().map((m) => ({ hash: m.hash, when: m.when })));
  });

  test("the guard refuses a database it does not own", () => {
    expect(() => assertOwnedDatabase(tdb.ownerUrl)).toThrow(/guard|services\.env/);
  });
});
