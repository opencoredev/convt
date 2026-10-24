// Reads and writes for the dashboard and settings pages. Every function takes the
// user id from the caller's verified session (never from the request body) and
// filters by it, so one account cannot read another's rows.

import { and, asc, desc, eq, gt, isNull, ne, sql } from "drizzle-orm";

import type { Db } from "../client";
import * as t from "../schema";

const dayMs = 86_400_000;

export const utcDay = (d: Date) =>
  new Date(Date.UTC(d.getUTCFullYear(), d.getUTCMonth(), d.getUTCDate()));
export const utcMonthStart = (d: Date) =>
  new Date(Date.UTC(d.getUTCFullYear(), d.getUTCMonth(), 1));

export async function getUser(db: Db, userId: string) {
  const [user] = await db.select().from(t.users).where(eq(t.users.id, userId));
  return user ?? null;
}

export function userSubscriptions(db: Db, userId: string) {
  return db
    .select()
    .from(t.subscriptions)
    .where(eq(t.subscriptions.userId, userId))
    .orderBy(desc(t.subscriptions.createdAt));
}

/** Licenses without their tokens, newest first. */
export function userLicenses(db: Db, userId: string) {
  return db
    .select({
      id: t.licenses.id,
      plan: t.licenses.plan,
      trial: t.licenses.trial,
      orderId: t.licenses.orderId,
      subscriptionId: t.licenses.subscriptionId,
      periodStart: t.licenses.periodStart,
      issuedOn: t.licenses.issuedOn,
      updatesUntil: t.licenses.updatesUntil,
      revokedAt: t.licenses.revokedAt,
      revokeReason: t.licenses.revokeReason,
      createdAt: t.licenses.createdAt,
      orderPaidAt: t.orders.paidAt,
      subscriptionInterval: t.subscriptions.interval,
    })
    .from(t.licenses)
    .leftJoin(t.orders, eq(t.orders.id, t.licenses.orderId))
    .leftJoin(t.subscriptions, eq(t.subscriptions.id, t.licenses.subscriptionId))
    .where(eq(t.licenses.userId, userId))
    .orderBy(desc(t.licenses.issuedOn), desc(t.licenses.createdAt));
}

export function activeDevices(db: Db, userId: string) {
  return db
    .select()
    .from(t.devices)
    .where(and(eq(t.devices.userId, userId), isNull(t.devices.revokedAt)))
    .orderBy(desc(t.devices.lastSeenAt));
}

/** The full token for "Copy key" and "Activate", or null if it is not this user's or is revoked. */
export async function getLicenseToken(
  db: Db,
  userId: string,
  licenseId: string,
): Promise<string | null> {
  const [row] = await db
    .select({ token: t.licenses.token })
    .from(t.licenses)
    .where(
      and(
        eq(t.licenses.id, licenseId),
        eq(t.licenses.userId, userId),
        isNull(t.licenses.revokedAt),
      ),
    );
  return row?.token ?? null;
}

/** An API checkout started in the last two hours that has not finished: enrollment is pending. */
export async function openApiCheckout(db: Db, userId: string, now: Date) {
  const [row] = await db
    .select({ id: t.checkouts.id, createdAt: t.checkouts.createdAt })
    .from(t.checkouts)
    .where(
      and(
        eq(t.checkouts.userId, userId),
        eq(t.checkouts.product, "api"),
        sql`${t.checkouts.status} in ('created', 'open')`,
        gt(t.checkouts.createdAt, new Date(now.getTime() - 2 * 3600_000)),
      ),
    )
    .orderBy(desc(t.checkouts.createdAt))
    .limit(1);
  return row ?? null;
}

/** An account deletion that has not finished. */
export async function openDeletion(db: Db, userId: string) {
  const [row] = await db
    .select({
      id: t.accountDeletions.id,
      status: t.accountDeletions.status,
      createdAt: t.accountDeletions.createdAt,
    })
    .from(t.accountDeletions)
    .where(and(eq(t.accountDeletions.userId, userId), ne(t.accountDeletions.status, "done")));
  return row ?? null;
}

/** Polar customer id we can open a portal session for (account-linked or a claimed guest order). */
export async function userPolarCustomerId(db: Db, userId: string): Promise<string | null> {
  const rows = await db.execute<{ provider_customer_id: string }>(sql`
    select provider_customer_id from billing_customers
    where provider = 'polar' and user_id = ${userId} and deleted_at is null
    union all
    select provider_customer_id from orders
    where user_id = ${userId} and provider_customer_id is not null
    union all
    select provider_customer_id from subscriptions
    where user_id = ${userId} and provider_customer_id is not null
    limit 1`);
  return rows.rows[0]?.provider_customer_id ?? null;
}

export function userInvoices(db: Db, userId: string) {
  return db
    .select({
      id: t.invoices.id,
      description: t.invoices.description,
      amountCents: t.invoices.amountCents,
      currency: t.invoices.currency,
      status: t.invoices.status,
      issuedAt: t.invoices.issuedAt,
      receiptUrl: t.invoices.receiptUrl,
    })
    .from(t.invoices)
    .where(eq(t.invoices.userId, userId))
    .orderBy(desc(t.invoices.issuedAt));
}

export function activeApiKeys(db: Db, userId: string) {
  return db
    .select({
      id: t.apiKeys.id,
      name: t.apiKeys.name,
      prefix: t.apiKeys.prefix,
      createdAt: t.apiKeys.createdAt,
      lastUsedAt: t.apiKeys.lastUsedAt,
    })
    .from(t.apiKeys)
    .where(and(eq(t.apiKeys.userId, userId), isNull(t.apiKeys.revokedAt)))
    .orderBy(asc(t.apiKeys.createdAt));
}

/**
 * API conversions per UTC day for the 30 days ending today, oldest first, with
 * corrections applied. Days with no usage are zero.
 */
export async function apiUsagePerDay(db: Db, userId: string, now: Date) {
  const start = new Date(utcDay(now).getTime() - 29 * dayMs);
  const rows = await db.execute<{ day: string; conversions: string }>(sql`
    select to_char(date_trunc('day', e.occurred_at at time zone 'UTC'), 'YYYY-MM-DD') as day,
           sum(e.quantity) as conversions
    from usage_events e
    left join usage_events o on o.id = e.corrects
    where e.user_id = ${userId}
      and (e.kind = 'api_conversion' or (e.kind = 'correction' and o.kind = 'api_conversion'))
      and e.occurred_at >= ${start} and e.occurred_at <= ${now}
    group by 1`);
  const byDay = new Map(rows.rows.map((r) => [r.day, Number(r.conversions)]));
  return Array.from({ length: 30 }, (_, i) => {
    const date = new Date(start.getTime() + i * dayMs).toISOString().slice(0, 10);
    return { date, conversions: byDay.get(date) ?? 0 };
  });
}

/** API conversions since the start of this UTC month. */
export async function apiConversionsThisMonth(db: Db, userId: string, now: Date) {
  const since = utcMonthStart(now);
  const rows = await db.execute<{ n: string | null }>(sql`
    select sum(e.quantity) as n
    from usage_events e
    left join usage_events o on o.id = e.corrects
    where e.user_id = ${userId}
      and (e.kind = 'api_conversion' or (e.kind = 'correction' and o.kind = 'api_conversion'))
      and e.occurred_at >= ${since} and e.occurred_at <= ${now}`);
  return { count: Number(rows.rows[0]?.n ?? 0), since };
}

export async function failedApiJobs(db: Db, userId: string, now: Date) {
  const since = new Date(now.getTime() - 30 * dayMs);
  const [row] = await db
    .select({ n: sql<number>`count(*)::int` })
    .from(t.cloudJobs)
    .where(
      and(
        eq(t.cloudJobs.userId, userId),
        eq(t.cloudJobs.source, "api"),
        eq(t.cloudJobs.status, "failed"),
        gt(t.cloudJobs.createdAt, since),
      ),
    );
  return row?.n ?? 0;
}

export function userAccounts(db: Db, userId: string) {
  return db
    .select({
      id: t.accounts.id,
      providerId: t.accounts.providerId,
      accountId: t.accounts.accountId,
      scope: t.accounts.scope,
      createdAt: t.accounts.createdAt,
    })
    .from(t.accounts)
    .where(eq(t.accounts.userId, userId));
}

/** Unexpired web sessions, newest activity first. Tokens are never returned. */
export function userSessions(db: Db, userId: string, now: Date) {
  return db
    .select({
      id: t.sessions.id,
      userAgent: t.sessions.userAgent,
      ipAddress: t.sessions.ipAddress,
      createdAt: t.sessions.createdAt,
      updatedAt: t.sessions.updatedAt,
    })
    .from(t.sessions)
    .where(and(eq(t.sessions.userId, userId), gt(t.sessions.expiresAt, now)))
    .orderBy(desc(t.sessions.updatedAt));
}

export async function revokeSession(db: Db, userId: string, sessionId: string): Promise<boolean> {
  const rows = await db
    .delete(t.sessions)
    .where(and(eq(t.sessions.id, sessionId), eq(t.sessions.userId, userId)))
    .returning({ id: t.sessions.id });
  return rows.length > 0;
}

/** Signs out every other web session and every device. */
export async function revokeOtherSessions(
  db: Db,
  userId: string,
  keepSessionId: string | null,
  now: Date,
) {
  const where = keepSessionId
    ? and(eq(t.sessions.userId, userId), ne(t.sessions.id, keepSessionId))
    : eq(t.sessions.userId, userId);
  const sessions = await db.delete(t.sessions).where(where).returning({ id: t.sessions.id });
  const devices = await revokeAllDevices(db, userId, now);
  return { sessions: sessions.length, devices };
}

export async function revokeDevice(
  db: Db,
  userId: string,
  deviceId: string,
  now: Date,
): Promise<boolean> {
  const rows = await db
    .update(t.devices)
    .set({ revokedAt: now, updatedAt: now })
    .where(
      and(eq(t.devices.id, deviceId), eq(t.devices.userId, userId), isNull(t.devices.revokedAt)),
    )
    .returning({ id: t.devices.id });
  return rows.length > 0;
}

export async function revokeAllDevices(db: Db, userId: string, now: Date): Promise<number> {
  const rows = await db
    .update(t.devices)
    .set({ revokedAt: now, updatedAt: now })
    .where(and(eq(t.devices.userId, userId), isNull(t.devices.revokedAt)))
    .returning({ id: t.devices.id });
  return rows.length;
}
