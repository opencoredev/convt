// Which overview a signed-in account sees. Pure, so the precedence is unit-tested.

export type AccountState = "pro" | "trial" | "desktop" | "pro_lapsed" | "api_only" | "new";

export type StateSubscription = {
  kind: string;
  status: string;
  endedAt: Date | null;
};

export type StateLicense = { plan: string; revokedAt: Date | null };

/** `paused` is never offered, so a paused Pro subscription shows as lapsed (and is alerted). */
const ended = (s: StateSubscription, now: Date) =>
  s.status === "canceled" ||
  s.status === "unpaid" ||
  s.status === "incomplete_expired" ||
  s.status === "paused" ||
  (s.endedAt !== null && s.endedAt <= now);

/**
 * Trialing, active or past due, and not ended. `incomplete` (a checkout that never
 * finished) is neither live nor ended: it does not show anywhere.
 */
const live = (s: StateSubscription, now: Date) =>
  (s.status === "trialing" || s.status === "active" || s.status === "past_due") && !ended(s, now);

/**
 * Precedence: Pro (active or past due) over trial (Pro trialing) over Desktop (an
 * unrevoked Desktop license) over lapsed Pro (a Pro subscription that ended) over
 * API only (an API subscription and nothing else) over new. API enrollment is shown
 * separately whatever the state.
 */
export function deriveAccountState(
  subscriptions: StateSubscription[],
  licenses: StateLicense[],
  now: Date,
): AccountState {
  const pro = subscriptions.filter((s) => s.kind === "pro");
  if (pro.some((s) => (s.status === "active" || s.status === "past_due") && !ended(s, now)))
    return "pro";
  if (pro.some((s) => s.status === "trialing" && !ended(s, now))) return "trial";
  if (licenses.some((l) => l.plan === "desktop" && l.revokedAt === null)) return "desktop";
  if (pro.some((s) => ended(s, now))) return "pro_lapsed";
  if (subscriptions.some((s) => s.kind === "api" && live(s, now))) return "api_only";
  return "new";
}

/** The current API enrollment, if any. */
export function activeApiSubscription<T extends StateSubscription>(
  subscriptions: T[],
  now: Date,
): T | null {
  return subscriptions.find((s) => s.kind === "api" && live(s, now)) ?? null;
}

/** The Pro subscription the billing page shows: the live one, else the newest ended one. */
export function currentProSubscription<T extends StateSubscription & { createdAt: Date }>(
  subscriptions: T[],
  now: Date,
): T | null {
  const pro = subscriptions
    .filter((s) => s.kind === "pro")
    .sort((a, b) => b.createdAt.getTime() - a.createdAt.getTime());
  return pro.find((s) => live(s, now)) ?? pro.find((s) => ended(s, now)) ?? null;
}
