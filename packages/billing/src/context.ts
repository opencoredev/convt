// What every billing operation gets: one database connection as convt_billing, the
// provider, the catalog, a clock, the signing key, mail, settings, and test hooks.

import { sql, type SQL } from "drizzle-orm";
import type { Db } from "@convt/db";
import { newId, type IdPrefix } from "@convt/license";
import type { MailTransport } from "@convt/mail";

import type { CaptureAnalytics } from "./analytics";
import type { Catalog } from "./catalog";
import type { BillingProvider } from "./provider";

/**
 * Named points where tests inject a crash: a thrown error at that point behaves
 * like the Worker dying there.
 */
export type FaultPoint =
  | "after-verify"
  | "after-license-insert"
  | "before-commit"
  | "after-commit"
  | "after-checkout-row"
  | "before-checkout-store"
  | "after-send-before-mark"
  | "before-delete-user"
  | "after-revoke";

export type BillingConfig = {
  /** https://convt.app, or the local dev origin. */
  siteUrl: string;
  mailFrom: string;
  alertEmail: string | null;
  downloadUrl: string;
  /** Per-delivery budget, well inside Polar's 10-second timeout. */
  budgetMs: number;
  /** The checkout cookie's name: `__Host-convt_checkout` in production. */
  checkoutCookie: string;
};

export type BillingContext = {
  db: Db;
  provider: BillingProvider;
  catalog: Catalog;
  clock: () => Date;
  signingKey: () => Promise<CryptoKey>;
  mail: MailTransport;
  config: BillingConfig;
  fault?: (point: FaultPoint) => void | Promise<void>;
  log: (line: string) => void;
  /** PostHog capture after a successful ingest. Tests record events this way. */
  captureAnalytics?: CaptureAnalytics;
};

export type Tx = Parameters<Parameters<Db["transaction"]>[0]>[0];
export type Q = Pick<Db, "execute">;

// Drizzle's node-postgres driver returns timestamptz as Postgres text
// ("2026-10-05 12:00:00.123456+00"). Raw queries here read them back as Dates.
const pgTimestamp = /^\d{4}-\d{2}-\d{2} \d{2}:\d{2}:\d{2}(\.\d+)?[+-]\d{2}(:\d{2})?$/;

export function pgDate(text: string): Date {
  const iso = text.replace(" ", "T").replace(/([+-]\d{2})$/, "$1:00");
  const d = new Date(iso);
  if (Number.isNaN(d.getTime())) throw new Error(`unreadable timestamp ${text}`);
  return d;
}

function revive(row: Record<string, unknown>) {
  for (const [k, v] of Object.entries(row))
    if (typeof v === "string" && pgTimestamp.test(v)) row[k] = pgDate(v);
  return row;
}

export async function rows<T>(q: Q, query: SQL): Promise<T[]> {
  return (await q.execute(query)).rows.map((r) => revive(r as Record<string, unknown>)) as T[];
}

export async function one<T>(q: Q, query: SQL): Promise<T | null> {
  return (await rows<T>(q, query))[0] ?? null;
}

export const id = (prefix: IdPrefix) => newId(prefix);

export async function fault(ctx: BillingContext, point: FaultPoint) {
  if (ctx.fault) await ctx.fault(point);
}

/** Transaction-scoped advisory locks, taken in a fixed (sorted) order. */
export async function lockKeys(tx: Q, keys: Iterable<string>) {
  for (const key of [...new Set(keys)].sort()) {
    await tx.execute(sql`select pg_advisory_xact_lock(hashtextextended(${key}, 0))`);
  }
}

export async function alert(q: Q, now: Date, kind: string, subject: string, detail: string) {
  await q.execute(sql`
    insert into billing_alerts (id, kind, subject, detail, created_at)
    values (${newId("alr")}, ${kind}, ${subject}, ${detail.slice(0, 300)}, ${now})
    on conflict (kind, subject) do nothing`);
}

export const isoDay = (d: Date | string) => new Date(d).toISOString().slice(0, 10);

/** The same day `years` later in UTC; 29 February becomes 28 February. */
export function addYears(day: string, years: number): string {
  const [y, m, d] = day.split("-").map(Number);
  const out = new Date(Date.UTC(y + years, m - 1, d));
  if (out.getUTCMonth() !== m - 1) out.setUTCDate(0);
  return isoDay(out);
}

/** a***@example.com */
export function maskEmail(email: string): string {
  const [local, domain] = email.split("@");
  if (!domain) return "***";
  return `${local.slice(0, 1)}***@${domain}`;
}
