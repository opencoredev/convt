// Better Auth's tables, renamed to plural snake_case. Field names (the object keys)
// must stay what Better Auth's adapter expects; test/auth-tables.test.ts checks them
// against the pinned better-auth version.

import { sql } from "drizzle-orm";
import {
  bigint,
  boolean,
  check,
  index,
  integer,
  pgTable,
  text,
  uniqueIndex,
} from "drizzle-orm/pg-core";

import { timestamps, tstz } from "./columns";

export const users = pgTable(
  "users",
  {
    id: text().primaryKey(),
    name: text().notNull(),
    email: text().notNull(),
    emailVerified: boolean().notNull().default(false),
    image: text(),
    ...timestamps,
  },
  (t) => [
    uniqueIndex("users_email_key").on(t.email),
    check("users_email_normalized", sql`${t.email} = lower(btrim(${t.email}))`),
  ],
);

export const sessions = pgTable(
  "sessions",
  {
    id: text().primaryKey(),
    userId: text()
      .notNull()
      .references(() => users.id, { onDelete: "cascade" }),
    token: text().notNull(),
    expiresAt: tstz().notNull(),
    ipAddress: text(),
    userAgent: text(),
    ...timestamps,
  },
  (t) => [
    uniqueIndex("sessions_token_key").on(t.token),
    index("sessions_user_id_idx").on(t.userId),
  ],
);

export const accounts = pgTable(
  "accounts",
  {
    id: text().primaryKey(),
    userId: text()
      .notNull()
      .references(() => users.id, { onDelete: "cascade" }),
    /** `github` or `google`. */
    providerId: text().notNull(),
    /** The provider's user id. */
    accountId: text().notNull(),
    // Encrypted by Better Auth (`encryptOAuthTokens`).
    accessToken: text(),
    refreshToken: text(),
    idToken: text(),
    accessTokenExpiresAt: tstz(),
    refreshTokenExpiresAt: tstz(),
    scope: text(),
    /** Better Auth's credential field. Passwords are off, so it stays null. */
    password: text(),
    ...timestamps,
  },
  (t) => [
    uniqueIndex("accounts_provider_account_key").on(t.providerId, t.accountId),
    index("accounts_user_id_idx").on(t.userId),
  ],
);

export const verifications = pgTable(
  "verifications",
  {
    id: text().primaryKey(),
    // Unique on purpose: an OTP resend must replace the previous code, not add one.
    identifier: text().notNull(),
    /** Hashed OTP and the attempt count. */
    value: text().notNull(),
    expiresAt: tstz().notNull(),
    ...timestamps,
  },
  (t) => [
    uniqueIndex("verifications_identifier_key").on(t.identifier),
    index("verifications_expires_at_idx").on(t.expiresAt),
  ],
);

export const rateLimits = pgTable(
  "rate_limits",
  {
    id: text().primaryKey(),
    key: text().notNull(),
    count: integer().notNull(),
    /** Milliseconds since the epoch, as Better Auth writes it. */
    lastRequest: bigint({ mode: "number" }).notNull(),
  },
  (t) => [uniqueIndex("rate_limits_key_key").on(t.key)],
);

/** Buckets for sending sign-in codes: `email:<sha256>` and `ip:<ip>`. */
export const otpSendLimits = pgTable(
  "otp_send_limits",
  {
    key: text().primaryKey(),
    windowStart: tstz().notNull(),
    count: integer().notNull(),
    expiresAt: tstz().notNull(),
  },
  (t) => [index("otp_send_limits_expires_at_idx").on(t.expiresAt)],
);

export const devices = pgTable(
  "devices",
  {
    id: text().primaryKey(),
    userId: text()
      .notNull()
      .references(() => users.id, { onDelete: "cascade" }),
    name: text().notNull(),
    os: text().notNull(),
    appVersion: text(),
    /** SHA-256 of the device token. Null until desktop sign-in exists (P8). */
    tokenHash: text(),
    lastSeenAt: tstz(),
    revokedAt: tstz(),
    ...timestamps,
    // Added by migration 0006 (CNV-56).
    /**
     * The app's device hash as `trials.device_hash` stores it (peppered by the site),
     * recorded when the device asks for a trial.
     */
    deviceHash: text(),
  },
  (t) => [
    uniqueIndex("devices_token_hash_key").on(t.tokenHash),
    index("devices_device_hash_idx").on(t.deviceHash),
    index("devices_user_id_active_idx")
      .on(t.userId)
      .where(sql`${t.revokedAt} is null`),
  ],
);
