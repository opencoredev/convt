// bun run dev:sign-in <email> --config <dir> [--replace]. Development only.
//
// Signs a desktop build from source in to a seeded fixture account (`bun run
// db:seed`) without the browser: a source build on a Mac where the released app
// is installed never receives its own convt://auth link, and a headless server
// has no browser to approve in. It runs the same two steps the site runs,
// approveDevice and exchangeCode, against this checkout's own database, and
// writes the app's sign-in (email and device token) to <dir>/account.json, the
// file a build reads with CONVT_LICENSE_STORE=file and CONVT_CONFIG_DIR=<dir>.
// The token is never printed. Revoke it from the dashboard like any device.

import { chmodSync, existsSync, mkdirSync, readFileSync, writeFileSync } from "node:fs";
import { hostname } from "node:os";
import { join, resolve } from "node:path";
import { parseArgs } from "node:util";

import { schema as t, withDb, type Db } from "@convt/db";
import { devDir, repoRoot, requireSetting } from "@convt/db/env";
import { assertOwnedDatabase } from "@convt/db/guard";
import { fixtureId, fixtures } from "@convt/db/seed";
import { base64urlEncode } from "@convt/license";
import { and, eq, isNull } from "drizzle-orm";

import {
  approveDevice,
  exchangeCode,
  parseDeviceRequest,
  tokenHash,
} from "../src/server/device-auth";

/** The seeded fixture with this address, if any. Nothing else can sign in here. */
function fixtureKey(address: string): string | null {
  return Object.entries(fixtures).find(([, email]) => email === address)?.[0] ?? null;
}

export type DevSession = { email: string; token: string };

const random43 = () => base64urlEncode(crypto.getRandomValues(new Uint8Array(32)));

/**
 * What the app and the approval page do together, for a seeded fixture: a fresh
 * state and verifier, approval as the account's user, then the exchange.
 */
export async function devDeviceSignIn(db: Db, email: string, now: Date): Promise<DevSession> {
  const address = email.trim().toLowerCase();
  const key = fixtureKey(address);
  if (!key) throw new Error("dev:sign-in only signs in to the accounts `bun run db:seed` writes");
  const [user] = await db
    .select({ id: t.users.id, verified: t.users.emailVerified })
    .from(t.users)
    .where(eq(t.users.email, address));
  // The seed gives each fixture a fixed id, so an account someone signed up with
  // the same address isn't one.
  if (!user || user.id !== fixtureId("usr", key))
    throw new Error(`${address} isn't seeded in this database; run \`bun run db:seed\``);
  if (!user.verified) throw new Error(`${address} has no verified email, so it can't sign in`);

  const verifier = random43();
  const challenge = base64urlEncode(
    new Uint8Array(await crypto.subtle.digest("SHA-256", new TextEncoder().encode(verifier))),
  );
  const req = parseDeviceRequest({
    state: random43(),
    challenge,
    name: `${hostname()} (dev:sign-in)`,
    os: "Dev build",
  });
  if (!req) throw new Error("could not build the device request");
  const approved = await approveDevice(db, user.id, req, now);
  if (!approved.ok) throw new Error("approval refused: too many sign-ins this hour");
  const code = new URL(
    approved.link.replace("convt://", "https://convt.invalid/"),
  ).searchParams.get("code");
  const exchanged = await exchangeCode(db, { code, verifier }, "127.0.0.1", now);
  const { token, email: signedIn } = exchanged.body;
  if (exchanged.status !== 200 || typeof token !== "string" || typeof signedIn !== "string")
    throw new Error(
      `exchange refused: ${JSON.stringify(exchanged.body.error ?? exchanged.status)}`,
    );
  return { email: signedIn, token };
}

const accountFile = (configDir: string) => join(configDir, "account.json");

/** Refuses before anything signs in, so a refusal leaves no device behind. */
function refuseExisting(configDir: string, replace: boolean) {
  const file = accountFile(configDir);
  if (existsSync(file) && !replace)
    throw new Error(`${file} exists; pass --replace to sign in again`);
}

/** Writes the app's sign-in file, owner-only, refusing to replace one unless asked. */
export function writeAccountFile(configDir: string, session: DevSession, replace: boolean): string {
  refuseExisting(configDir, replace);
  const file = accountFile(configDir);
  mkdirSync(configDir, { recursive: true, mode: 0o700 });
  writeFileSync(file, JSON.stringify({ email: session.email, token: session.token }), {
    mode: 0o600,
    flag: replace ? "w" : "wx",
  });
  chmodSync(file, 0o600);
  return file;
}

/** Undoes a sign-in whose file couldn't be written, so no unused device stays active. */
export async function revokeDevSession(db: Db, token: string, now: Date): Promise<void> {
  await db
    .update(t.devices)
    .set({ revokedAt: now, updatedAt: now })
    .where(and(eq(t.devices.tokenHash, await tokenHash(token)), isNull(t.devices.revokedAt)));
}

/** The site origin dev:web wrote, so the app renews against the same server. */
function siteOrigin(): string | null {
  const vars = join(repoRoot, "apps", "web", ".dev.vars");
  if (!existsSync(vars)) return null;
  return (
    readFileSync(vars, "utf8")
      .match(/^BETTER_AUTH_URL=(.+)$/m)?.[1]
      ?.trim() ?? null
  );
}

async function main() {
  const { values, positionals } = parseArgs({
    allowPositionals: true,
    options: { config: { type: "string" }, replace: { type: "boolean", default: false } },
  });
  const [email] = positionals;
  if (!email || !values.config) {
    console.error("usage: bun run dev:sign-in <email> --config <dir> [--replace]");
    process.exit(2);
  }
  // The site's own role, like the /api/device routes.
  const url = requireSetting("DATABASE_URL");
  assertOwnedDatabase(url);
  const configDir = resolve(values.config);
  refuseExisting(configDir, values.replace);
  const { session, file } = await withDb(url, async (db) => {
    const signedIn = await devDeviceSignIn(db, email, new Date());
    try {
      return { session: signedIn, file: writeAccountFile(configDir, signedIn, values.replace) };
    } catch (error) {
      await revokeDevSession(db, signedIn.token, new Date());
      throw error;
    }
  });
  const pub = join(devDir, "license.pub");
  console.log(`dev:sign-in: ${session.email} signed in; wrote ${file}`);
  console.log("Run a build from source with:");
  console.log(`  CONVT_ACCOUNT_URL=${siteOrigin() ?? "<the dev:web origin>"}`);
  console.log("  CONVT_LICENSE_STORE=file");
  console.log(`  CONVT_CONFIG_DIR=${configDir}`);
  if (existsSync(pub)) console.log(`  CONVT_LICENSE_PUBKEY=${readFileSync(pub, "utf8").trim()}`);
}

if (import.meta.main) {
  await main().catch((error: unknown) => {
    console.error(`dev:sign-in: ${error instanceof Error ? error.message : String(error)}`);
    process.exit(1);
  });
}
