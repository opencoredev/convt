// The reconciler. Polar lists nothing by modification time, so it combines short
// scans of new objects, re-fetches of objects that can still change, and a slow
// full sweep, all through the same ingest code. Every scan pages in ascending
// creation order, so new objects append and a stored page number stays valid.
// See docs/p7-billing-plan.md, section 2.

import { sql } from "drizzle-orm";
import { newId } from "@convt/license";

import { meteredPriceProblem, validateCatalog } from "./catalog";
import { alert, type BillingContext, isoDay, one, rows } from "./context";
import { ingestFacts } from "./ingest";
import { drainOutbox, enqueueEmail, outboxRetention } from "./outbox";
import { emptyFacts, type ProviderFacts, type ScanKind } from "./provider";
import { replayFailedEvents } from "./webhook";

const sweepPagesPerRun = 20;
let pageLimit = 100;

/** Tests use small pages to exercise paging and wrapping with few objects. */
export function setPageLimitForTests(n: number) {
  pageLimit = n;
}

export type FrequentStep =
  | "orders"
  | "refunds"
  | "disputes"
  | "refetch"
  | "replay"
  | "trials"
  | "sweep"
  | "outbox";

async function cursor(ctx: BillingContext, name: string) {
  const now = ctx.clock();
  await ctx.db.execute(
    sql`insert into reconcile_cursors (name, page, pass_started_at, updated_at) values (${name}, 1, ${now}, ${now}) on conflict (name) do nothing`,
  );
  return (await one<{ page: number; pass_started_at: Date }>(
    ctx.db,
    sql`select page, pass_started_at from reconcile_cursors where name = ${name}`,
  ))!;
}

async function saveCursor(ctx: BillingContext, name: string, page: number, wrapped: boolean) {
  const now = ctx.clock();
  await ctx.db.execute(sql`
    update reconcile_cursors set page = ${page}, updated_at = ${now},
      pass_started_at = case when ${wrapped} then ${now}::timestamptz else pass_started_at end
    where name = ${name}`);
}

/** Ingests one fact at a time, so one bad object cannot block the rest. */
async function ingestEach(ctx: BillingContext, facts: ProviderFacts, source: string) {
  let rejected = 0;
  let errors = 0;
  const each: ProviderFacts[] = [
    ...facts.orders.map((o) => ({ ...emptyFacts(), orders: [o] })),
    ...facts.subscriptions.map((s) => ({ ...emptyFacts(), subscriptions: [s] })),
    ...facts.disputes.map((d) => ({ ...emptyFacts(), disputes: [d] })),
    ...facts.hints
      .filter((h) => h.kind !== "settings")
      .map((h) => ({ ...emptyFacts(), hints: [h] })),
  ];
  for (const f of each) {
    try {
      const r = await ingestFacts(ctx, f, source);
      if (r.rejected) rejected++;
    } catch (e) {
      ctx.log(`[billing] ${source}: ${(e as Error).message}`);
      errors++;
    }
  }
  return { ingested: each.length, rejected, errors };
}

/**
 * Pages `kind` from the stored cursor to the end, then keeps the last page for next
 * time. A page with an object that failed to ingest (not one our checks rejected)
 * stops the scan there, so the next run retries it instead of skipping past.
 */
async function scanToEnd(ctx: BillingContext, kind: ScanKind, name: string) {
  const c = await cursor(ctx, name);
  let page = Math.max(1, c.page);
  let total = 0;
  for (let i = 0; i < 50; i++) {
    const r = await ctx.provider.scan(kind, { page, limit: pageLimit });
    const done = await ingestEach(ctx, r.facts, name);
    total += done.ingested;
    if (done.errors > 0 || page >= r.maxPage) break;
    page++;
  }
  await saveCursor(ctx, name, page, false);
  return total;
}

/** Every 15 minutes. `only` limits the run to some steps (tests). */
export async function reconcileFrequent(ctx: BillingContext, only?: FrequentStep[]) {
  const run = (step: FrequentStep) => !only || only.includes(step);
  const started = ctx.clock();
  const runId = newId("rcn");
  await ctx.db.execute(
    sql`insert into reconcile_runs (id, kind, started_at, status) values (${runId}, 'frequent', ${started}, 'running')`,
  );
  const summary: Record<string, unknown> = {};
  try {
    // New orders since the last run's start, minus an hour of overlap.
    const last = await one<{ started_at: Date }>(
      ctx.db,
      sql`
      select started_at from reconcile_runs where kind = 'frequent' and status = 'ok' order by started_at desc limit 1`,
    );
    const after = new Date(
      (last?.started_at ?? new Date(started.getTime() - 86_400_000)).getTime() - 3600_000,
    );
    let orders = 0;
    for (let page = 1; page <= 50 && run("orders"); page++) {
      const r = await ctx.provider.scan("orders", {
        page,
        limit: pageLimit,
        createdAfter: after.toISOString(),
      });
      orders += (await ingestEach(ctx, r.facts, "scan:orders")).ingested;
      if (page >= r.maxPage) break;
    }
    summary.orders = orders;
    // Refunds are listed by refund time, so a late refund of an old order is found.
    if (run("refunds")) summary.refunds = await scanToEnd(ctx, "refunds", "refunds");
    if (run("disputes")) summary.disputes = await scanToEnd(ctx, "disputes", "disputes");

    // Objects that can still change, one by one.
    const subs = !run("refetch")
      ? []
      : await rows<{ provider_subscription_id: string }>(
          ctx.db,
          sql`
      select provider_subscription_id from subscriptions
      where provider_subscription_id not like 'seed_%'
        and (status in ('incomplete', 'trialing', 'active', 'past_due', 'paused', 'unpaid')
          or ended_at > ${new Date(started.getTime() - 35 * 86_400_000)})`,
        );
    for (const s of subs) {
      try {
        await ingestFacts(
          ctx,
          {
            ...emptyFacts(),
            subscriptions: [await ctx.provider.getSubscription(s.provider_subscription_id)],
          },
          "refetch",
        );
      } catch (e) {
        ctx.log(`[billing] refetch ${s.provider_subscription_id}: ${(e as Error).message}`);
      }
    }
    summary.subscriptions = subs.length;
    const openDisputes = !run("refetch")
      ? []
      : await rows<{ provider_dispute_id: string }>(
          ctx.db,
          sql`
      select provider_dispute_id from disputes where status not in ('lost', 'won') and provider_dispute_id not like 'seed_%'`,
        );
    for (const d of openDisputes) {
      try {
        await ingestFacts(
          ctx,
          { ...emptyFacts(), disputes: [await ctx.provider.getDispute(d.provider_dispute_id)] },
          "refetch",
        );
      } catch (e) {
        ctx.log(`[billing] refetch ${d.provider_dispute_id}: ${(e as Error).message}`);
      }
    }
    const stale = !run("refetch")
      ? []
      : await rows<{ provider_checkout_id: string }>(
          ctx.db,
          sql`
      select provider_checkout_id from checkouts
      where status in ('created', 'open') and provider_checkout_id is not null and created_at < ${new Date(started.getTime() - 10 * 60_000)}
        and created_at > ${new Date(started.getTime() - 3 * 86_400_000)}`,
        );
    for (const c of stale) {
      try {
        const co = await ctx.provider.getCheckout(c.provider_checkout_id);
        const facts = { ...emptyFacts(), checkouts: [co] };
        if (co.status === "succeeded")
          facts.orders.push(...(await ctx.provider.checkoutOrders(c.provider_checkout_id)));
        if (co.providerSubscriptionId)
          facts.subscriptions.push(await ctx.provider.getSubscription(co.providerSubscriptionId));
        await ingestFacts(ctx, facts, "refetch");
      } catch (e) {
        ctx.log(`[billing] refetch checkout: ${(e as Error).message}`);
      }
    }
    // A checkout row that never reached the provider expires unused.
    await ctx.db.execute(sql`
      update checkouts set status = 'expired', updated_at = ${started}
      where status = 'created' and provider_checkout_id is null and nonce_expires_at < ${started}`);
    if (run("replay")) summary.replayed = await replayFailedEvents(ctx);
    if (run("trials")) summary.trialEnding = await trialEndingScan(ctx);
    if (run("sweep")) summary.sweep = await sweep(ctx);
    if (run("outbox")) summary.outbox = await drainOutbox(ctx);
    await finishRun(ctx, runId, "ok", summary);
  } catch (e) {
    summary.error = (e as Error).message;
    await finishRun(ctx, runId, "failed", summary);
    throw e;
  }
  return summary;
}

async function finishRun(
  ctx: BillingContext,
  id: string,
  status: string,
  summary: Record<string, unknown>,
) {
  await ctx.db.execute(sql`
    update reconcile_runs set status = ${status}, finished_at = ${ctx.clock()}, summary = ${JSON.stringify(summary)}::jsonb where id = ${id}`);
}

/** Pro trials ending within 48 hours and not set to cancel get one email each. */
export async function trialEndingScan(ctx: BillingContext) {
  const now = ctx.clock();
  const due = await rows<{
    id: string;
    user_id: string | null;
    email: string;
    trial_ends_at: Date;
    verified: string | null;
  }>(
    ctx.db,
    sql`
    select s.id, s.user_id, s.email, s.trial_ends_at, (select u.email from users u where u.id = s.user_id and u.email_verified) as verified
    from subscriptions s
    where s.kind = 'pro' and s.status = 'trialing' and not s.cancel_at_period_end
      and s.trial_ends_at > ${now} and s.trial_ends_at <= ${new Date(now.getTime() + 48 * 3600_000)}`,
  );
  for (const s of due) {
    await enqueueEmail(ctx.db, {
      kind: "trial_ending",
      dedupeKey: `trial_ending:${s.id}:${s.trial_ends_at.toISOString()}`,
      to: s.verified ?? s.email,
      userId: s.user_id,
      subjectId: s.id,
      now,
    });
  }
  return due.length;
}

/** The slow full sweep: 20 pages per run across orders and subscriptions, wrapping. */
export async function sweep(ctx: BillingContext, pages = sweepPagesPerRun) {
  const out: Record<string, number> = {};
  for (const [kind, name] of [
    ["orders", "sweep:orders"],
    ["subscriptions", "sweep:subscriptions"],
  ] as const) {
    const c = await cursor(ctx, name);
    let page = Math.max(1, c.page);
    let done = 0;
    for (let i = 0; i < pages / 2; i++) {
      const r = await ctx.provider.scan(kind, { page, limit: pageLimit });
      const facts = r.facts;
      if (kind === "orders")
        facts.orders = facts.orders.filter((o) =>
          ["paid", "partially_refunded", "refunded"].includes(o.status),
        );
      const result = await ingestEach(ctx, facts, name);
      done += result.ingested;
      // A failed object keeps the sweep on this page for the next run.
      if (result.errors > 0) break;
      if (page >= r.maxPage) {
        page = 1;
        await saveCursor(ctx, name, page, true);
        break;
      }
      page++;
      await saveCursor(ctx, name, page, false);
    }
    out[kind] = done;
  }
  return out;
}

/** Daily at 03:17 UTC. Returns the discrepancies it found. */
export async function reconcileDaily(ctx: BillingContext) {
  const now = ctx.clock();
  const runId = newId("rcn");
  await ctx.db.execute(
    sql`insert into reconcile_runs (id, kind, started_at, status) values (${runId}, 'daily', ${now}, 'running')`,
  );
  const found: Array<{ kind: string; subject: string; detail: string }> = [];
  const note = (kind: string, subject: string, detail: string) =>
    found.push({ kind, subject, detail });

  // Catalog and settings drift.
  for (const p of validateCatalog(ctx.catalog)) note("catalog", ctx.catalog.env, p);
  try {
    const remote = await ctx.provider.products();
    for (const [name, e] of Object.entries(ctx.catalog.products)) {
      const price = remote.find((r) => r.priceId === e.priceId && r.productId === e.productId);
      if (!price) {
        note(
          "catalog_drift",
          name,
          `price ${e.priceId} of product ${e.productId} is missing at the provider`,
        );
        continue;
      }
      if (price.archived) note("catalog_drift", name, "the price is archived");
      if (price.currency !== "usd") note("catalog_drift", name, `currency ${price.currency}`);
      if (e.amountCents !== null && price.amountCents !== e.amountCents)
        note(
          "catalog_drift",
          name,
          `the provider charges ${price.amountCents}, the catalog says ${e.amountCents}`,
        );
      if (name === "api") {
        const problem = meteredPriceProblem(price.unitAmount);
        if (problem) note("catalog_drift", name, problem);
        else if (
          price.unitAmount !== e.unitAmount &&
          Number(price.unitAmount) !== Number(e.unitAmount)
        )
          note(
            "catalog_drift",
            name,
            `unit amount ${price.unitAmount}, the catalog says ${e.unitAmount}`,
          );
      }
    }
    const s = await ctx.provider.settings();
    if (!s.allowMultipleSubscriptions)
      note(
        "settings",
        "allow_multiple_subscriptions",
        "off: API enrollment is refused until it is on",
      );
    if (!s.preventTrialAbuse) note("settings", "prevent_trial_abuse", "off");
    if (s.trialConversionEmail)
      note("settings", "subscription_trial_conversion_reminder", "on: ours replaces it");
    if (s.pastDueEmail) note("settings", "subscription_past_due", "on: ours replaces it");
  } catch (e) {
    note("provider", "daily", `could not read products or settings: ${(e as Error).message}`);
  }

  // Invariants.
  for (const r of await rows<{ id: string; n: number }>(
    ctx.db,
    sql`
    select o.id, (select count(*)::int from licenses l where l.order_id = o.id and l.plan = 'desktop' and l.reissue_of is null and l.revoked_at is null) as n
    from orders o
    where o.status in ('paid', 'partially_refunded')
      and not exists (select 1 from disputes d where d.order_id = o.id and d.status = 'lost')`,
  )) {
    if (r.n !== 1) note("invariant", r.id, `a paid Desktop order has ${r.n} unrevoked keys`);
  }
  for (const r of await rows<{ id: string }>(
    ctx.db,
    sql`
    select o.id from orders o
    where (o.status = 'refunded' or exists (select 1 from disputes d where d.order_id = o.id and d.status = 'lost'))
      and exists (select 1 from licenses l where l.order_id = o.id and l.revoked_at is null)`,
  )) {
    note("invariant", r.id, "a refunded or lost Desktop order has an unrevoked key");
  }
  for (const r of await rows<{ id: string }>(
    ctx.db,
    sql`
    select s.id from subscriptions s
    where s.kind = 'pro' and exists (
      select 1 from payment_coverage c join invoices i on i.id = c.invoice_id
      where c.subscription_id = s.id and c.amount_cents > 0 and i.status in ('paid', 'partially_refunded')
        and (i.net_cents > 0 or i.applied_balance_cents > 0)
        and not exists (select 1 from disputes d where d.invoice_id = i.id and d.status = 'lost')
        and c.period_end::date > coalesce((select max(l.updates_until) from licenses l
          where l.subscription_id = s.id and l.plan = 'pro' and l.revoked_at is null and l.reissue_of is null), '1970-01-01'))`,
  )) {
    note("invariant", r.id, "a funded period has no key");
  }
  for (const r of await rows<{ id: string }>(
    ctx.db,
    sql`
    select s.id from subscriptions s where s.status = 'trialing'
      and exists (select 1 from licenses l where l.subscription_id = s.id and l.revoked_at is null and not l.trial)`,
  )) {
    note("invariant", r.id, "a trialing subscription has a key");
  }
  for (const r of await rows<{ user_id: string; kind: string; n: number }>(
    ctx.db,
    sql`
    select user_id, kind, count(*)::int as n from subscriptions
    where user_id is not null and status in ('trialing', 'active', 'past_due') and (ended_at is null or ended_at > ${now})
    group by user_id, kind having count(*) > 1`,
  )) {
    note("invariant", `${r.kind}:${r.user_id}`, `${r.n} live ${r.kind} subscriptions`);
  }
  for (const r of await rows<{ id: string }>(
    ctx.db,
    sql`
    select id from subscriptions where kind = 'api' and status in ('active', 'past_due') and spend_cap_cents is null`,
  )) {
    note("invariant", r.id, "a live API subscription has no cap");
  }
  for (const r of await rows<{ id: string; status: string }>(
    ctx.db,
    sql`
    select id, status from email_outbox where status in ('ambiguous', 'dead') and finished_at > ${new Date(now.getTime() - 86_400_000)}`,
  )) {
    note(
      `email_${r.status}`,
      r.id,
      `outbox row ${r.status}; look it up in Resend by the outbox_id tag`,
    );
  }
  const passes = await rows<{ name: string; pass_started_at: Date }>(
    ctx.db,
    sql`select name, pass_started_at from reconcile_cursors where name like 'sweep:%'`,
  );
  for (const p of passes)
    if (now.getTime() - p.pass_started_at.getTime() > 7 * 86_400_000)
      note("sweep", p.name, "the current pass is older than 7 days");

  for (const f of found) await alert(ctx.db, now, f.kind, f.subject, f.detail);
  const housekeeping = await outboxRetention(ctx);

  // One digest for everything undigested: today's findings, rejected and dead events, emails.
  const date = isoDay(now);
  const undigested = await ctx.db.execute(
    sql`update billing_alerts set digested_at = ${now} where digested_at is null`,
  );
  if ((undigested.rowCount ?? 0) > 0 && ctx.config.alertEmail) {
    await enqueueEmail(ctx.db, {
      kind: "alert_digest",
      dedupeKey: `alert_digest:${date}`,
      to: ctx.config.alertEmail,
      userId: null,
      subjectId: date,
      now,
    });
  }
  await drainOutbox(ctx);
  await finishRun(ctx, runId, "ok", {
    found: found.length,
    housekeeping,
    digested: undigested.rowCount ?? 0,
  });
  return { found, housekeeping };
}
