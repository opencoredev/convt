// The webhook route's logic, shared by the Worker and the tests. Verification comes
// first and writes nothing on failure. A verified delivery is processed in one
// transaction; any error after verification rolls it back and a second
// transaction records the event as `failed` with its verified body, so Polar's
// retry or the reconciler's replay can finish it. Ten attempts make it `dead`.

import { sql } from "drizzle-orm";
import { newId } from "@convt/license";

import type { AnalyticsEvent } from "./analytics";
import { alert, type BillingContext, fault, one, type Q } from "./context";
import { applyFacts, BudgetExceeded, checkDeadline, hydrate } from "./ingest";
import { safeError } from "./outbox";
import { isRejected } from "./verify";

export const maxAttempts = 10;

export type WebhookResult = {
  status: number;
  body: string;
  eventStatus?: string;
  drain: boolean;
  /** Captured after the response so PostHog cannot delay Polar. */
  analytics?: AnalyticsEvent[];
};

const text = (status: number, body: string, extra: Partial<WebhookResult> = {}): WebhookResult => ({
  status,
  body,
  drain: false,
  ...extra,
});

/** POST /webhooks/polar. `raw` is the body exactly as received. */
export async function handleWebhook(
  ctx: BillingContext,
  method: string,
  raw: Uint8Array,
  headers: Headers,
): Promise<WebhookResult> {
  if (method !== "POST") return text(405, "method not allowed");
  const verified = await ctx.provider.verifyWebhook(raw, headers, new Date());
  if (isRejected(verified)) {
    // One line with the reason; never headers or the body.
    ctx.log(`[billing] webhook refused: ${verified.reason}`);
    return text(verified.status, "invalid signature");
  }
  await fault(ctx, "after-verify");
  return processEvent(ctx, { providerEventId: verified.id, body: verified.body }, "delivery");
}

/**
 * Processes one verified event: from a delivery, or replayed from the stored body
 * of a `failed` row (only verified bodies are ever stored).
 */
export async function processEvent(
  ctx: BillingContext,
  event: { providerEventId: string; body: string },
  mode: "delivery" | "replay",
): Promise<WebhookResult> {
  const deadline = Date.now() + ctx.config.budgetMs;
  const parsed = ctx.provider.parseEvent({
    id: event.providerEventId,
    timestamp: 0,
    body: event.body,
  });
  const type = parsed.type.slice(0, 100);
  try {
    // Fetch hints before the transaction holds any lock.
    const hydrated =
      parsed.ok && !parsed.ignored
        ? await hydrate(ctx, parsed.facts, deadline)
        : { cards: new Map<string, boolean>() };
    const result = await ctx.db.transaction(async (tx) => {
      const now = ctx.clock();
      const inserted = await one<{ id: string }>(
        tx,
        sql`
        insert into webhook_events (id, provider, provider_event_id, type, received_at, body, status, attempts, updated_at)
        values (${newId("whe")}, 'polar', ${event.providerEventId}, ${type}, ${now}, ${event.body}, 'failed', 1, ${now})
        on conflict (provider, provider_event_id) do nothing
        returning id`,
      );
      let attempts = 1;
      if (!inserted) {
        const row = (await one<{ status: string; attempts: number }>(
          tx,
          sql`select status, attempts from webhook_events where provider = 'polar' and provider_event_id = ${event.providerEventId} for update`,
        ))!;
        if (row.status !== "failed") return { done: row.status };
        attempts = row.attempts + 1;
        await tx.execute(sql`
          update webhook_events set attempts = ${attempts}, body = ${event.body}, updated_at = ${now}
          where provider = 'polar' and provider_event_id = ${event.providerEventId}`);
      }
      const finish = async (status: string, reason: string | null) => {
        await tx.execute(sql`
          update webhook_events set status = ${status}, reason = ${reason}, processed_at = ${now}, updated_at = ${now}
          where provider = 'polar' and provider_event_id = ${event.providerEventId}`);
        return { done: status, reason };
      };
      if (!parsed.ok) {
        await alert(tx, now, "rejected_event", event.providerEventId, parsed.reason);
        return finish("rejected", parsed.reason);
      }
      if (parsed.ignored) return finish("ignored", null);
      const outcome = await applyFacts(ctx, tx, parsed.facts, hydrated.cards);
      if (outcome.rejected !== null) {
        await alert(tx, now, "rejected_event", event.providerEventId, outcome.rejected);
        return finish("rejected", outcome.rejected);
      }
      if (parsed.facts.hints.some((h) => h.kind === "settings"))
        await alert(
          tx,
          now,
          "settings_changed",
          `organization:${now.toISOString().slice(0, 10)}`,
          "organization.updated: the daily check rereads the settings",
        );
      checkDeadline(deadline);
      await fault(ctx, "before-commit");
      return {
        ...(await finish(
          "processed",
          outcome.notes.length ? outcome.notes.join("; ").slice(0, 500) : null,
        )),
        events: outcome.events,
      };
    });
    if (mode === "delivery") await fault(ctx, "after-commit");
    const analytics = result.done === "processed" && "events" in result ? result.events : undefined;
    return text(200, "ok", {
      eventStatus: result.done,
      drain: result.done === "processed",
      analytics,
    });
  } catch (e) {
    const reason =
      e instanceof BudgetExceeded
        ? "budget exceeded"
        : safeError(null, (e as Error)?.name ?? "Error", (e as Error)?.message ?? "");
    ctx.log(`[billing] event ${event.providerEventId} failed: ${reason}`);
    try {
      await recordFailure(ctx.db, ctx.clock(), event, type, reason);
    } catch (e2) {
      // The database is down too: Polar's retries and the reconciler remain.
      ctx.log(`[billing] could not record the failure: ${(e2 as Error)?.name}`);
    }
    return text(500, "retry later", { eventStatus: "failed" });
  }
}

async function recordFailure(
  q: Q,
  now: Date,
  event: { providerEventId: string; body: string },
  type: string,
  reason: string,
) {
  const row = await one<{ status: string; attempts: number }>(
    q,
    sql`
    insert into webhook_events (id, provider, provider_event_id, type, received_at, body, status, reason, attempts, updated_at)
    values (${newId("whe")}, 'polar', ${event.providerEventId}, ${type}, ${now}, ${event.body}, 'failed', ${reason}, 1, ${now})
    on conflict (provider, provider_event_id) do update set
      attempts = webhook_events.attempts + 1,
      status = case when webhook_events.status in ('processed', 'ignored', 'rejected') then webhook_events.status
                    when webhook_events.attempts + 1 >= ${maxAttempts} then 'dead' else 'failed' end,
      body = excluded.body, reason = excluded.reason, updated_at = excluded.updated_at
    returning status, attempts`,
  );
  if (row?.status === "dead")
    await alert(
      q,
      now,
      "dead_event",
      event.providerEventId,
      `failed ${row.attempts} times: ${reason}`,
    );
}

/** The reconciler replays stored `failed` events without re-verifying. */
export async function replayFailedEvents(ctx: BillingContext, limit = 50) {
  const failed = await ctx.db.execute(sql`
    select provider_event_id, body from webhook_events
    where provider = 'polar' and status = 'failed' and body is not null
    order by received_at limit ${limit}`);
  let processed = 0;
  for (const r of failed.rows as Array<{ provider_event_id: string; body: string }>) {
    const res = await processEvent(
      ctx,
      { providerEventId: r.provider_event_id, body: r.body },
      "replay",
    );
    if (res.status === 200) processed++;
  }
  return { replayed: failed.rows.length, processed };
}
