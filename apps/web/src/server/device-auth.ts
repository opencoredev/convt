// Desktop sign-in and Pro renewal (P8), the site's side. Server-only.
//
// 1. The app opens /device?state=&challenge=&name=&os=&version=. `challenge` is the
//    base64url SHA-256 of a verifier that never leaves the app.
// 2. The signed-in user approves. `approveDevice` stores a one-time code with the
//    user id and the challenge in `verifications` (five minutes), and the page opens
//    convt://auth?state=...&code=...
// 3. The app posts the code and the verifier to /api/device/token. `exchangeCode`
//    deletes the code (one use, whatever happens next), checks the verifier against
//    the challenge, and creates a `devices` row holding only the SHA-256 (hex) of a
//    new device token, which it returns once.
// 4. The app posts the token to /api/device/license. `renewDevice` finds the
//    unrevoked device, notes when it was seen, and asks convt-billing for the
//    account's current Pro key.
//
// The /api/device routes never read cookies: they authenticate by code and verifier
// or by bearer token, which is why request.ts lets them skip the Origin check.
// Revoking a device on the dashboard (revokeDevice) makes its token answer 401.

import { schema as t, consumeSendBucket, type Db } from "@convt/db";
import { base64urlEncode, newId, randomBytes } from "@convt/license";
import { and, eq, isNull, sql } from "drizzle-orm";

const codeTtlMs = 5 * 60 * 1000;
const hourMs = 60 * 60 * 1000;
const codePrefix = "device-code:";
export const deviceTokenScheme = "cvd_";

/** Per-hour limits. Every local request comes from one IP, so tests pass their own. */
export const deviceLimits = {
  approvePerUser: 10,
  tokenPerIp: 30,
  renewPerIp: 120,
  renewPerDevice: 30,
};

/** The largest JSON body the /api/device routes read. */
export const maxDeviceBody = 4096;

/**
 * Reads a JSON body of at most `max` bytes, stopping as soon as it is larger, so an
 * oversized upload is never buffered. "too_large" for that, null for bad JSON.
 */
export async function readSmallJson(
  request: Request,
  max = maxDeviceBody,
): Promise<{ value: unknown } | "too_large" | null> {
  const declared = Number(request.headers.get("content-length") ?? 0);
  if (declared > max) return "too_large";
  if (!request.body) return null;
  const reader = request.body.getReader();
  const chunks: Uint8Array[] = [];
  let size = 0;
  for (;;) {
    const { done, value } = await reader.read();
    if (done) break;
    size += value.byteLength;
    if (size > max) {
      await reader.cancel().catch(() => {});
      return "too_large";
    }
    chunks.push(value);
  }
  const bytes = new Uint8Array(size);
  let at = 0;
  for (const c of chunks) {
    bytes.set(c, at);
    at += c.byteLength;
  }
  try {
    return { value: JSON.parse(new TextDecoder().decode(bytes)) };
  } catch {
    return null;
  }
}

export type DeviceRequest = {
  state: string;
  challenge: string;
  name: string;
  os: string;
  version: string;
};

const b64url43 = /^[A-Za-z0-9_-]{43}$/;

/** Printable text without control characters, trimmed and cut to `max`. */
function cleanText(value: unknown, max: number): string {
  if (typeof value !== "string") return "";
  let out = "";
  for (const ch of value.normalize("NFC")) {
    const code = ch.codePointAt(0)!;
    if (code < 0x20 || (code >= 0x7f && code < 0xa0) || (code >= 0x2028 && code <= 0x202e))
      continue;
    out += ch;
  }
  return [...out.trim()].slice(0, max).join("");
}

/** The app's request from the /device URL, or null if it is malformed. */
export function parseDeviceRequest(search: Record<string, unknown>): DeviceRequest | null {
  const { state, challenge } = search;
  if (typeof state !== "string" || !b64url43.test(state)) return null;
  if (typeof challenge !== "string" || !b64url43.test(challenge)) return null;
  const version = cleanText(search.version, 32);
  return {
    state,
    challenge,
    name: cleanText(search.name, 64) || "This computer",
    os: cleanText(search.os, 32) || "Unknown",
    version: /^[0-9A-Za-z.+-]{1,32}$/.test(version) ? version : "",
  };
}

async function sha256(text: string): Promise<Uint8Array> {
  return new Uint8Array(await crypto.subtle.digest("SHA-256", new TextEncoder().encode(text)));
}

export async function tokenHash(token: string): Promise<string> {
  return [...(await sha256(token))].map((b) => b.toString(16).padStart(2, "0")).join("");
}

function sameText(a: string, b: string): boolean {
  if (a.length !== b.length) return false;
  let diff = 0;
  for (let i = 0; i < a.length; i++) diff |= a.charCodeAt(i) ^ b.charCodeAt(i);
  return diff === 0;
}

/** The link the approval page opens. */
export function authLink(state: string, result: { code: string } | { error: string }): string {
  const tail = "code" in result ? `code=${result.code}` : `error=${result.error}`;
  return `convt://auth?state=${state}&${tail}`;
}

export type ApproveResult = { ok: true; link: string } | { ok: false; reason: "rate_limited" };

/** Step 2: a one-time code for this user and this app's challenge. */
export async function approveDevice(
  db: Db,
  userId: string,
  req: DeviceRequest,
  now: Date,
): Promise<ApproveResult> {
  const count = await consumeSendBucket(db, `device-approve:user:${userId}`, hourMs, now);
  if (count > deviceLimits.approvePerUser) return { ok: false, reason: "rate_limited" };
  await purgeExpiredCodes(db, now);
  const code = base64urlEncode(randomBytes(32));
  await db.insert(t.verifications).values({
    id: newId("ver"),
    identifier: codePrefix + (await tokenHash(code)),
    value: JSON.stringify({
      userId,
      challenge: req.challenge,
      name: req.name,
      os: req.os,
      version: req.version,
    }),
    expiresAt: new Date(now.getTime() + codeTtlMs),
    createdAt: now,
    updatedAt: now,
  });
  return { ok: true, link: authLink(req.state, { code }) };
}

export type DeviceResponse = { status: number; body: Record<string, unknown> };

const json = (status: number, body: Record<string, unknown>): DeviceResponse => ({ status, body });

/** Step 3: the code and verifier for a device token. */
export async function exchangeCode(
  db: Db,
  input: unknown,
  ip: string,
  now: Date,
): Promise<DeviceResponse> {
  const byIp = await consumeSendBucket(db, `device-token:ip:${ip}`, hourMs, now);
  if (byIp > deviceLimits.tokenPerIp) return json(429, { error: "rate_limited" });
  const { code, verifier } = (input ?? {}) as { code?: unknown; verifier?: unknown };
  if (
    typeof code !== "string" ||
    !b64url43.test(code) ||
    typeof verifier !== "string" ||
    !b64url43.test(verifier)
  )
    return json(400, { error: "invalid_request" });
  // Deleted before anything else is checked: a code works once, even a wrong try.
  const [row] = await db
    .delete(t.verifications)
    .where(eq(t.verifications.identifier, codePrefix + (await tokenHash(code))))
    .returning({ value: t.verifications.value, expiresAt: t.verifications.expiresAt });
  if (!row || row.expiresAt <= now) return json(400, { error: "invalid_grant" });
  const grant = JSON.parse(row.value) as {
    userId: string;
    challenge: string;
    name: string;
    os: string;
    version: string;
  };
  if (!sameText(base64urlEncode(await sha256(verifier)), grant.challenge))
    return json(400, { error: "invalid_grant" });
  const [user] = await db
    .select({ email: t.users.email, verified: t.users.emailVerified })
    .from(t.users)
    .where(eq(t.users.id, grant.userId));
  if (!user?.verified) return json(400, { error: "invalid_grant" });
  const token = deviceTokenScheme + base64urlEncode(randomBytes(32));
  await db.insert(t.devices).values({
    id: newId("dev"),
    userId: grant.userId,
    name: grant.name,
    os: grant.os,
    appVersion: grant.version || null,
    tokenHash: await tokenHash(token),
    lastSeenAt: now,
    createdAt: now,
    updatedAt: now,
  });
  return json(200, { token, email: user.email });
}

/** The bearer token of a request, if it looks like a device token. */
export function bearer(header: string | null): string | null {
  const m = /^Bearer (cvd_[A-Za-z0-9_-]{43})$/.exec(header ?? "");
  return m ? m[1] : null;
}

async function activeDevice(db: Db, token: string) {
  const [device] = await db
    .select({ id: t.devices.id, userId: t.devices.userId })
    .from(t.devices)
    .where(and(eq(t.devices.tokenHash, await tokenHash(token)), isNull(t.devices.revokedAt)));
  return device ?? null;
}

export type ProKeySource = (
  userId: string,
) => Promise<{ key: string; updatesUntil: string } | null>;

/** Step 4: the current Pro key for the device's account. */
export async function renewDevice(
  db: Db,
  proKey: ProKeySource,
  token: string | null,
  input: unknown,
  ip: string,
  now: Date,
): Promise<DeviceResponse> {
  const byIp = await consumeSendBucket(db, `device-renew:ip:${ip}`, hourMs, now);
  if (byIp > deviceLimits.renewPerIp) return json(429, { error: "rate_limited" });
  if (!token) return json(401, { error: "signed_out" });
  const device = await activeDevice(db, token);
  if (!device) return json(401, { error: "signed_out" });
  const byDevice = await consumeSendBucket(db, `device-renew:dev:${device.id}`, hourMs, now);
  if (byDevice > deviceLimits.renewPerDevice) return json(429, { error: "rate_limited" });
  const version = cleanText((input as { version?: unknown } | null)?.version, 32);
  await db
    .update(t.devices)
    .set({
      lastSeenAt: now,
      updatedAt: now,
      ...(/^[0-9A-Za-z.+-]{1,32}$/.test(version) ? { appVersion: version } : {}),
    })
    .where(eq(t.devices.id, device.id));
  const current = await proKey(device.userId);
  return json(200, { key: current?.key ?? null, updates_until: current?.updatesUntil ?? null });
}

/** The app's Sign out: revokes the device the token belongs to. */
export async function signOutDevice(
  db: Db,
  token: string | null,
  now: Date,
): Promise<DeviceResponse> {
  if (!token) return json(401, { error: "signed_out" });
  const rows = await db
    .update(t.devices)
    .set({ revokedAt: now, updatedAt: now })
    .where(and(eq(t.devices.tokenHash, await tokenHash(token)), isNull(t.devices.revokedAt)))
    .returning({ id: t.devices.id });
  return rows.length ? json(200, { ok: true }) : json(401, { error: "signed_out" });
}

/** Removes expired codes, so abandoned approvals don't pile up. */
export async function purgeExpiredCodes(db: Db, now: Date): Promise<void> {
  await db.execute(
    sql`delete from verifications where identifier like ${codePrefix + "%"} and expires_at <= ${now}`,
  );
}
