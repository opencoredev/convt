// bun tools/billing-mock/src/preload.ts. Copies the seeded fixtures' provider
// customers and subscriptions from this checkout's database into the running mock,
// so "Manage billing", the card block and plan changes work for fixture accounts.
// Reads as convt_billing; writes nothing to the database. Safe to repeat.

import pg from "pg";

const env: Record<string, string> = {};
const file = Bun.file(new URL("../../../.convt-dev/services.env", import.meta.url));
if (await file.exists()) {
  for (const line of (await file.text()).split("\n")) {
    const m = line.match(/^([A-Z_]+)=(.*)$/);
    if (m) env[m[1]] = m[2];
  }
}
const dbUrl = process.env.BILLING_DATABASE_URL ?? env.BILLING_DATABASE_URL;
const mockUrl = process.env.BILLING_MOCK_URL ?? env.BILLING_MOCK_URL;
if (!dbUrl || !mockUrl) {
  console.error(
    "preload: BILLING_DATABASE_URL and BILLING_MOCK_URL are needed (run bun run db:up)",
  );
  process.exit(2);
}
const client = new pg.Client({ connectionString: dbUrl });
await client.connect();
try {
  const customers = await client.query(
    `select c.provider_customer_id as id, c.user_id as external_id, c.email, c.created_at,
       -- A fixture whose API enrollment waits for a card has none at the provider.
       exists (select 1 from subscriptions s where s.provider_customer_id = c.provider_customer_id
               and s.kind = 'api' and s.status = 'active' and s.card_seen_at is null) as no_card
     from billing_customers c where c.provider_customer_id like 'seed_%'`,
  );
  const subs = await client.query(
    `select s.provider_subscription_id as id, s.provider_customer_id as customer_id, s.kind, s.interval,
            s.status, s.current_period_start, s.current_period_end, s.trial_ends_at, s.cancel_at_period_end,
            s.ended_at, s.created_at
     from subscriptions s where s.provider_subscription_id like 'seed_%' and s.provider_customer_id is not null`,
  );
  const iso = (d: Date | null) => (d ? d.toISOString() : null);
  const body = {
    customers: customers.rows.map((r) => ({ ...r, created_at: iso(r.created_at) })),
    subscriptions: subs.rows.map((r) => {
      const start = r.current_period_start ?? r.created_at;
      const end =
        r.current_period_end ??
        new Date(Date.UTC(start.getUTCFullYear(), start.getUTCMonth() + 1, start.getUTCDate()));
      return {
        id: r.id,
        customer_id: r.customer_id,
        product: r.kind === "api" ? "api" : r.interval === "year" ? "pro_year" : "pro_month",
        status: r.status,
        current_period_start: iso(start),
        current_period_end: iso(end),
        trial_end: iso(r.trial_ends_at),
        cancel_at_period_end: r.cancel_at_period_end,
        ended_at: iso(r.ended_at),
        created_at: iso(r.created_at),
      };
    }),
  };
  const res = await fetch(`${mockUrl}/admin/preload`, {
    method: "POST",
    headers: { "content-type": "application/json" },
    body: JSON.stringify(body),
    signal: AbortSignal.timeout(5000),
  });
  console.log(`preload: ${res.status} ${await res.text()}`);
} finally {
  await client.end();
}
