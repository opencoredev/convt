// The desktop trial and the key the app keeps (CNV-56). The site authenticates the
// device token, peppers the app's device hash, and asks convt-billing through
// BillingRpc.startTrial and BillingRpc.currentKey.
//
// A trial is one `trials` row per account and per computer. Its token is signed on
// every request from the row and the account's current email, so the database
// holds no trial token; Ed25519 makes the same inputs give the same token. A trial
// token's `updates_until` is the last UTC day the trial works: started on day S, it
// works through S + 6.

import { and, desc, eq, inArray, isNull, ne } from "drizzle-orm";

import { schema as t, type Db } from "@convt/db";
import { newId, sign } from "@convt/license";

import type { BillingContext } from "./context";
import { isoDay } from "./context";

/** What signing a key needs: a connection as convt_billing and the signing key. */
export type KeyDeps = Pick<BillingContext, "db" | "signingKey">;

export const trialDays = 7;
/** The longest a Pro subscription's trial token may run past today. */
export const proTrialCapDays = 31;

const dayMs = 24 * 60 * 60 * 1000;
const storedHashPattern = /^[0-9a-f]{64}$/;

const addDays = (day: string, days: number) =>
  isoDay(new Date(Date.parse(`${day}T00:00:00Z`) + days * dayMs));

export type TrialResult =
  | { ok: true; key: string; endsAt: Date }
  | { ok: false; reason: "device_used" };

type TrialRow = { id: string; startedAt: Date; endsAt: Date };

async function signTrial(deps: KeyDeps, row: TrialRow, email: string): Promise<string> {
  return sign(
    {
      id: row.id,
      email,
      plan: "trial",
      issued: isoDay(row.startedAt),
      updates_until: isoDay(new Date(row.endsAt.getTime() - dayMs)),
    },
    await deps.signingKey(),
  );
}

async function trialOf(db: Db, userId: string): Promise<TrialRow | null> {
  const [row] = await db
    .select({ id: t.trials.id, startedAt: t.trials.startedAt, endsAt: t.trials.endsAt })
    .from(t.trials)
    .where(eq(t.trials.userId, userId));
  return row ?? null;
}

/**
 * The account's trial, started now if it has none. An account gets one trial ever,
 * on any computer, and an expired one still comes back (the app shows it ended). A
 * computer whose hash already started another account's trial gets `device_used`.
 * Concurrent calls for one account or one computer create one row: the unique
 * indexes decide, and the loser reads the winner's row.
 */
export async function startTrial(
  deps: KeyDeps,
  input: { userId: string; deviceHash: string; now: Date },
): Promise<TrialResult> {
  if (!storedHashPattern.test(input.deviceHash))
    throw new Error("startTrial: the device hash is not 64 lowercase hex digits");
  const [user] = await deps.db
    .select({ email: t.users.email })
    .from(t.users)
    .where(eq(t.users.id, input.userId));
  if (!user) throw new Error("startTrial: no such user");
  const reply = async (row: TrialRow): Promise<TrialResult> => ({
    ok: true,
    key: await signTrial(deps, row, user.email),
    endsAt: row.endsAt,
  });

  const existing = await trialOf(deps.db, input.userId);
  if (existing) return reply(existing);

  // A computer counts as used once any account's trial reached it, not only
  // the computer that started it: the site records the hash on every device
  // that asks, including one that got an account's existing trial back.
  const [usedHere] = await deps.db
    .select({ id: t.trials.id })
    .from(t.devices)
    .innerJoin(t.trials, eq(t.trials.userId, t.devices.userId))
    .where(and(eq(t.devices.deviceHash, input.deviceHash), ne(t.devices.userId, input.userId)))
    .limit(1);
  if (usedHere) return { ok: false, reason: "device_used" };

  const startDay = isoDay(input.now);
  const [inserted] = await deps.db
    .insert(t.trials)
    .values({
      id: newId("trl"),
      userId: input.userId,
      deviceHash: input.deviceHash,
      startedAt: input.now,
      endsAt: new Date(`${addDays(startDay, trialDays)}T00:00:00Z`),
      createdAt: input.now,
      updatedAt: input.now,
    })
    .onConflictDoNothing()
    .returning({ id: t.trials.id, startedAt: t.trials.startedAt, endsAt: t.trials.endsAt });
  if (inserted) return reply(inserted);

  // A conflict: either this account won a race on another computer, or the
  // computer belongs to another account's trial.
  const raced = await trialOf(deps.db, input.userId);
  if (raced) return reply(raced);
  return { ok: false, reason: "device_used" };
}

export type CurrentKey = { key: string; updatesUntil: string } | null;

/**
 * The key the desktop app should hold: the account's unrevoked paid key of either
 * plan that covers the most builds. Failing that, a trial token for a Pro
 * subscription in its trial, through the trial's end and at most 31 days ahead.
 * The desktop trial itself comes only from startTrial.
 */
export async function currentKey(
  deps: KeyDeps,
  input: { userId: string; now: Date },
): Promise<CurrentKey> {
  const [paid] = await deps.db
    .select({ key: t.licenses.token, updatesUntil: t.licenses.updatesUntil })
    .from(t.licenses)
    .where(
      and(
        eq(t.licenses.userId, input.userId),
        inArray(t.licenses.plan, ["desktop", "pro"]),
        eq(t.licenses.trial, false),
        isNull(t.licenses.revokedAt),
      ),
    )
    .orderBy(desc(t.licenses.updatesUntil), desc(t.licenses.createdAt))
    .limit(1);
  const today = isoDay(input.now);
  // A paid key covering every build up to today beats any trial. One whose
  // update window ended earlier still runs older builds, but a Pro trial
  // also runs the newer ones, so the trial goes first while it lasts.
  if (paid && paid.updatesUntil >= today) return paid;

  const [trialing] = await deps.db
    .select({
      id: t.subscriptions.id,
      email: t.users.email,
      trialEndsAt: t.subscriptions.trialEndsAt,
      periodEnd: t.subscriptions.currentPeriodEnd,
    })
    .from(t.subscriptions)
    .innerJoin(t.users, eq(t.users.id, t.subscriptions.userId))
    .where(
      and(
        eq(t.subscriptions.userId, input.userId),
        eq(t.subscriptions.kind, "pro"),
        eq(t.subscriptions.status, "trialing"),
      ),
    )
    .orderBy(desc(t.subscriptions.createdAt))
    .limit(1);
  const endsAt = trialing?.trialEndsAt ?? trialing?.periodEnd;
  if (!trialing || !endsAt) return paid ?? null;

  const cap = addDays(today, proTrialCapDays);
  // The UTC day the subscription's trial ends on, so the key works until the
  // trial does. An end at exactly midnight belongs to the day before.
  const end = isoDay(new Date(endsAt.getTime() - 1));
  if (endsAt <= input.now) return paid ?? null;
  const updatesUntil = end < cap ? end : cap;
  const key = await sign(
    {
      id: trialing.id,
      email: trialing.email,
      plan: "trial",
      issued: today,
      updates_until: updatesUntil,
    },
    await deps.signingKey(),
  );
  return { key, updatesUntil };
}
