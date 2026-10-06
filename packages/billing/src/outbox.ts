// The email outbox. Rows are inserted in the ingest transaction; the drain claims,
// freezes, sends and completes them. A license key is emailed exactly once even if
// the Worker dies between any two steps: Resend deduplicates on the outbox id for
// 24 hours, and a row whose outcome may be unknown after 23 hours becomes
// `ambiguous` instead of risking a second copy. See docs/p7-billing-plan.md,
// section 5.

import { createHash } from "node:crypto";

import { sql } from "drizzle-orm";
import { newId } from "@convt/license";
import {
  alertDigest,
  licenseIssued,
  renewalFailed,
  templateVersion,
  trialEnding,
  type OutgoingEmail,
  type Rendered,
} from "@convt/mail";

import { type BillingContext, fault, one, type Q, rows } from "./context";

export type EmailKind = "license_issued" | "trial_ending" | "renewal_failed" | "alert_digest";

export async function enqueueEmail(
  q: Q,
  e: {
    kind: EmailKind;
    dedupeKey: string;
    to: string;
    userId: string | null;
    subjectId: string;
    now: Date;
  },
) {
  await q.execute(sql`
    insert into email_outbox (id, kind, dedupe_key, to_email, user_id, subject_id, status, next_attempt_at, created_at, updated_at)
    values (${newId("eml")}, ${e.kind}, ${e.dedupeKey}, ${e.to}, ${e.userId}, ${e.subjectId}, 'pending', ${e.now}, ${e.now}, ${e.now})
    on conflict (dedupe_key) do nothing`);
}

/** Minutes until the next attempt, by attempts already made. */
export const backoffMinutes = [1, 5, 30, 120, 360, 720];
export const leaseMs = 2 * 60_000;
const ambiguousAfterMs = 23 * 3600_000;
const maxAttempts = 10;

/** Status, provider code and a redacted, short message: no addresses, tokens or keys. */
export function safeError(status: number | null, code: string, message = ""): string {
  const redacted = message
    .replace(/[^\s@<>"']+@[^\s@<>"']+/g, "[email]")
    .replace(/[A-Za-z0-9_-]{20,}(\.[A-Za-z0-9_-]{20,})?/g, "[redacted]");
  return `${status ?? "-"} ${code}${redacted ? ` ${redacted}` : ""}`.slice(0, 200);
}

type Row = {
  id: string;
  kind: EmailKind;
  to_email: string;
  user_id: string | null;
  subject_id: string;
  claim_generation: number;
  attempts: number;
  payload: OutgoingEmail | null;
  first_attempt_at: Date | null;
  unknown_outcome_at: Date | null;
};

/** Renders a row's email, or null when it is no longer relevant (it becomes skipped). */
async function freeze(ctx: BillingContext, q: Q, row: Row): Promise<Rendered | null> {
  const site = ctx.config.siteUrl;
  if (row.kind === "license_issued") {
    const lic = await one<{
      token: string;
      plan: "desktop" | "pro";
      updates_until: string;
      revoked_at: Date | null;
    }>(
      q,
      sql`select token, plan, updates_until::text, revoked_at from licenses where id = ${row.subject_id}`,
    );
    if (!lic || lic.revoked_at) return null;
    return licenseIssued({
      product: lic.plan,
      token: lic.token,
      updatesUntil: lic.updates_until,
      siteUrl: site,
      downloadUrl: ctx.config.downloadUrl,
    });
  }
  if (row.kind === "trial_ending") {
    const s = await one<{
      status: string;
      cancel_at_period_end: boolean;
      trial_ends_at: Date | null;
      interval: string;
    }>(
      q,
      sql`select status, cancel_at_period_end, trial_ends_at, interval from subscriptions where id = ${row.subject_id}`,
    );
    if (!s || s.status !== "trialing" || s.cancel_at_period_end || !s.trial_ends_at) return null;
    const interval = s.interval === "year" ? "year" : "month";
    return trialEnding({
      trialEndsAt: s.trial_ends_at.toISOString(),
      amountCents:
        ctx.catalog.products[interval === "year" ? "pro_year" : "pro_month"].amountCents!,
      interval,
      siteUrl: site,
    });
  }
  if (row.kind === "renewal_failed") {
    const s = await one<{ status: string; kind: "pro" | "api"; current_period_start: Date | null }>(
      q,
      sql`select status, kind, current_period_start from subscriptions where id = ${row.subject_id}`,
    );
    if (!s || s.status !== "past_due") return null;
    return renewalFailed({
      kind: s.kind,
      periodStart: (s.current_period_start ?? new Date(0)).toISOString(),
      siteUrl: site,
    });
  }
  const digest = await digestInput(q, row.subject_id);
  return alertDigest(digest);
}

async function digestInput(q: Q, date: string) {
  const items = await rows<{ kind: string; subject: string; detail: string }>(
    q,
    sql`select kind, subject, detail from billing_alerts where digested_at::date = ${date}::date order by created_at`,
  );
  const counts: Record<string, number> = {};
  for (const i of items) counts[i.kind] = (counts[i.kind] ?? 0) + 1;
  return { date, counts, items };
}

export type DrainResult = {
  claimed: number;
  sent: number;
  retried: number;
  dead: number;
  skipped: number;
  ambiguous: number;
};

/** Claims up to `limit` due rows, then sends each. Safe to run in parallel. */
export async function drainOutbox(ctx: BillingContext, limit = 20): Promise<DrainResult> {
  const now = ctx.clock();
  const result: DrainResult = {
    claimed: 0,
    sent: 0,
    retried: 0,
    dead: 0,
    skipped: 0,
    ambiguous: 0,
  };
  const claimed = await ctx.db.transaction(async (tx) =>
    rows<Row>(
      tx,
      sql`
      update email_outbox set status = 'sending', locked_until = ${new Date(now.getTime() + leaseMs)},
        claim_generation = claim_generation + 1, updated_at = ${now}
      where id in (
        select id from email_outbox
        where (status = 'pending' and next_attempt_at <= ${now})
           or (status = 'sending' and locked_until < ${now})
        order by next_attempt_at
        limit ${limit}
        for update skip locked)
      returning id, kind, to_email, user_id, subject_id, claim_generation, attempts, payload, first_attempt_at, unknown_outcome_at`,
    ),
  );
  result.claimed = claimed.length;
  for (const row of claimed) {
    const outcome = await sendOne(ctx, row);
    result[outcome]++;
  }
  return result;
}

/** A fenced update: it applies only while this worker still holds the claim. */
async function fenced(q: Q, row: Row, set: ReturnType<typeof sql>) {
  const r = await q.execute(
    sql`update email_outbox set ${set} where id = ${row.id} and claim_generation = ${row.claim_generation} and status = 'sending'`,
  );
  return (r.rowCount ?? 0) > 0;
}

async function sendOne(ctx: BillingContext, row: Row): Promise<keyof Omit<DrainResult, "claimed">> {
  const now = ctx.clock();
  let payload = row.payload;
  if (!payload) {
    // Freeze on the first claim: render once and store the exact request.
    const rendered = await freeze(ctx, ctx.db, row);
    if (!rendered) {
      await fenced(
        ctx.db,
        row,
        sql`status = 'skipped', finished_at = ${now}, locked_until = null, updated_at = ${now}`,
      );
      return "skipped";
    }
    payload = {
      from: ctx.config.mailFrom,
      to: row.to_email,
      subject: rendered.subject,
      text: rendered.text,
      html: rendered.html,
      tags: [
        { name: "outbox_id", value: row.id },
        { name: "kind", value: row.kind },
      ],
    };
    const sha = createHash("sha256").update(JSON.stringify(payload)).digest("hex");
    const ok = await fenced(
      ctx.db,
      row,
      sql`payload = ${JSON.stringify(payload)}::jsonb, payload_sha256 = ${sha}, template_version = ${templateVersion}, updated_at = ${now}`,
    );
    if (!ok) return "skipped";
  }
  if (
    row.unknown_outcome_at &&
    row.first_attempt_at &&
    now.getTime() - row.first_attempt_at.getTime() > ambiguousAfterMs
  ) {
    await markAmbiguous(ctx, row, now);
    return "ambiguous";
  }
  const started = await fenced(
    ctx.db,
    row,
    // Until Resend answers, this attempt may have been accepted: if the Worker dies
    // here, the reclaimed row already counts as having an unknown outcome, so the
    // 23-hour rule applies to it. A definite answer restores the earlier value.
    sql`first_attempt_at = coalesce(first_attempt_at, ${now}), last_attempt_at = ${now}, attempts = attempts + 1,
      unknown_outcome_at = coalesce(unknown_outcome_at, ${now}), updated_at = ${now}`,
  );
  if (!started) return "skipped";
  const firstAttempt = row.first_attempt_at ?? now;
  const attempts = row.attempts + 1;

  const sent = await ctx.mail.send(payload, row.id);
  await fault(ctx, "after-send-before-mark");
  const done = ctx.clock();
  if (sent.ok) {
    await fenced(
      ctx.db,
      row,
      sql`status = 'sent', provider_message_id = ${sent.id}, sent_at = ${done}, finished_at = ${done}, locked_until = null, last_error = null, updated_at = ${done}`,
    );
    return "sent";
  }
  const error = safeError(sent.status, sent.code);
  if (sent.outcome === "dead") {
    await fenced(
      ctx.db,
      row,
      sql`status = 'dead', last_error = ${error}, finished_at = ${done}, locked_until = null, updated_at = ${done}`,
    );
    return "dead";
  }
  const unknownAt = sent.unknown ? done : row.unknown_outcome_at;
  const next = new Date(
    done.getTime() + backoffMinutes[Math.min(attempts - 1, backoffMinutes.length - 1)] * 60_000,
  );
  // Resend keeps a key 24 hours. Once a retry would land past 23 hours after the
  // first attempt and an earlier attempt might have been accepted, a retry could
  // send a second copy, so the row stops and waits for Leo.
  if (unknownAt && next.getTime() - firstAttempt.getTime() > ambiguousAfterMs) {
    await fenced(ctx.db, row, sql`unknown_outcome_at = ${unknownAt}, last_error = ${error}`);
    await markAmbiguous(ctx, { ...row, unknown_outcome_at: unknownAt }, done);
    return "ambiguous";
  }
  if (attempts >= maxAttempts) {
    await fenced(
      ctx.db,
      row,
      sql`status = 'dead', last_error = ${error}, finished_at = ${done}, locked_until = null, updated_at = ${done}`,
    );
    return "dead";
  }
  await fenced(
    ctx.db,
    row,
    sql`status = 'pending', next_attempt_at = ${next}, locked_until = null, last_error = ${error}, unknown_outcome_at = ${unknownAt}, updated_at = ${done}`,
  );
  return "retried";
}

async function markAmbiguous(ctx: BillingContext, row: Row, now: Date) {
  await fenced(
    ctx.db,
    row,
    sql`status = 'ambiguous', finished_at = ${now}, locked_until = null, updated_at = ${now}`,
  );
}

/**
 * Leo's decision on an ambiguous row: `sent` records that Resend delivered it;
 * `resend` sends it again under a new key (a new outbox row).
 */
export async function resolveOutbox(
  ctx: BillingContext,
  outboxId: string,
  decision: "sent" | "resend",
) {
  const now = ctx.clock();
  return ctx.db.transaction(async (tx) => {
    const row = await one<{
      status: string;
      kind: EmailKind;
      dedupe_key: string;
      to_email: string;
      user_id: string | null;
      subject_id: string;
    }>(
      tx,
      sql`select status, kind, dedupe_key, to_email, user_id, subject_id from email_outbox where id = ${outboxId} for update`,
    );
    if (!row) throw new Error(`no outbox row ${outboxId}`);
    if (row.status !== "ambiguous")
      throw new Error(`outbox row ${outboxId} is ${row.status}, not ambiguous`);
    if (decision === "sent") {
      await tx.execute(
        sql`update email_outbox set status = 'sent', last_error = 'resolved as sent', updated_at = ${now} where id = ${outboxId}`,
      );
      return { status: "sent" as const };
    }
    await tx.execute(
      sql`update email_outbox set status = 'dead', last_error = 'resolved: sent again under a new key', updated_at = ${now} where id = ${outboxId}`,
    );
    const newId_ = newId("eml");
    await tx.execute(sql`
      insert into email_outbox (id, kind, dedupe_key, to_email, user_id, subject_id, status, next_attempt_at, created_at, updated_at)
      values (${newId_}, ${row.kind}, ${`${row.dedupe_key}:resend:${outboxId}`}, ${row.to_email}, ${row.user_id}, ${row.subject_id}, 'pending', ${now}, ${now}, ${now})`);
    return { status: "resend" as const, newId: newId_ };
  });
}

/** Payloads are nulled 25 hours after a final status; metadata rows go after 400 days. */
export async function outboxRetention(ctx: BillingContext) {
  const now = ctx.clock();
  const nulled = await ctx.db.execute(sql`
    update email_outbox set payload = null, updated_at = ${now}
    where payload is not null and status in ('sent', 'skipped', 'dead') and finished_at < ${new Date(now.getTime() - 25 * 3600_000)}`);
  const deleted = await ctx.db.execute(sql`
    delete from email_outbox where status in ('sent', 'skipped', 'dead') and finished_at < ${new Date(now.getTime() - 400 * 86_400_000)}`);
  const bodies = await ctx.db.execute(sql`
    update webhook_events set body = null, updated_at = ${now}
    where body is not null and status <> 'failed' and received_at < ${new Date(now.getTime() - 30 * 86_400_000)}`);
  return {
    payloadsNulled: nulled.rowCount ?? 0,
    rowsDeleted: deleted.rowCount ?? 0,
    bodiesNulled: bodies.rowCount ?? 0,
  };
}

export { fault };
