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

import {
  chmodSync,
  existsSync,
  linkSync,
  mkdirSync,
  readFileSync,
  renameSync,
  rmSync,
  writeFileSync,
} from "node:fs";
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

/**
 * Writes the app's sign-in file, owner-only, refusing to replace one unless asked.
 * The token goes into a new owner-only file that then takes the name, like the
 * app's own `write_private`, so a failed write leaves the old sign-in whole and
 * the token is never in a file anyone else could read.
 */
export function writeAccountFile(configDir: string, session: DevSession, replace: boolean): string {
  refuseExisting(configDir, replace);
  const file = accountFile(configDir);
  mkdirSync(configDir, { recursive: true, mode: 0o700 });
  const temp = join(configDir, `.account.json.${process.pid}.tmp`);
  try {
    writeFileSync(temp, JSON.stringify({ email: session.email, token: session.token }), {
      mode: 0o600,
      flag: "wx",
    });
    chmodSync(temp, 0o600);
    // A link fails if the name appeared meanwhile; a rename replaces it.
    if (replace) renameSync(temp, file);
    else linkSync(temp, file);
  } finally {
    rmSync(temp, { force: true });
  }
  return file;
}

/** The token in an existing sign-in file, if it holds one. */
export function storedToken(configDir: string): string | null {
  try {
    const parsed: unknown = JSON.parse(readFileSync(accountFile(configDir), "utf8"));
    if (typeof parsed !== "object" || parsed === null || !("token" in parsed)) return null;
    return typeof parsed.token === "string" ? parsed.token : null;
  } catch {
    return null;
  }
}

/** Turns a dev sign-in's device off, so no device without a file stays active. */
export async function revokeDevSession(db: Db, token: string, now: Date): Promise<void> {
  await db
    .update(t.devices)
    .set({ revokedAt: now, updatedAt: now })
    .where(and(eq(t.devices.tokenHash, await tokenHash(token)), isNull(t.devices.revokedAt)));
}

/**
 * Signs in and writes the file as one step. A failed write revokes the new
 * device; a replaced sign-in's device is revoked once the new file is in place.
 */
export async function signInToConfig(
  db: Db,
  email: string,
  configDir: string,
  replace: boolean,
  now: Date,
): Promise<{ session: DevSession; file: string }> {
  refuseExisting(configDir, replace);
  const previous = replace ? storedToken(configDir) : null;
  const session = await devDeviceSignIn(db, email, now);
  let file: string;
  try {
    file = writeAccountFile(configDir, session, replace);
  } catch (error) {
    await revokeDevSession(db, session.token, now);
    throw error;
  }
  if (previous && previous !== session.token) await revokeDevSession(db, previous, now);
  return { session, file };
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
  const { session, file } = await withDb(url, (db) =>
    signInToConfig(db, email, configDir, values.replace, new Date()),
  );
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
