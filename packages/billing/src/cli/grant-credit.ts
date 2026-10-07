// bun run billing:grant-credit <email> <dollars>
//
// Grants prepaid API credit to an account, with no card and no provider. The grant
// is an API subscription with provider `grant`: active, no period end, and a spend
// cap equal to the credit, so usage counts against it once and never resets. A
// second grant to the same account raises the existing cap. The worker never
// meters grant usage to Polar. Accounts with API card billing are refused. Connects as convt_billing with BILLING_DATABASE_URL
// (this checkout's own database by default).

import { connect } from "@convt/db";
import { servicesEnv } from "@convt/db/env";
import { newId } from "@convt/license";
import { sql } from "drizzle-orm";

const url = servicesEnv().BILLING_DATABASE_URL;
if (!url) {
  console.error("billing:grant-credit: BILLING_DATABASE_URL is not set");
  process.exit(2);
}
const [rawEmail, dollars] = process.argv.slice(2);
const cents = Math.round(Number(dollars) * 100);
if (!rawEmail || !Number.isInteger(cents) || cents <= 0 || cents > 1_000_000) {
  console.error("usage: bun run billing:grant-credit <email> <dollars, at most 10000>");
  process.exit(2);
}
const email = rawEmail.trim().toLowerCase();
const { client, db } = await connect(url);
try {
  const result = await db.transaction(async (tx) => {
    const user = (
      await tx.execute<{ id: string }>(sql`select id from users where email = ${email}`)
    ).rows[0];
    if (!user) throw new Error(`no account with email ${email}`);
    // Jobs reserve against the newest API subscription; a grant beside a Polar one
    // would take its usage off the bill and trip the duplicate checks.
    const paid = (
      await tx.execute(sql`
        select 1 as x from subscriptions
        where user_id = ${user.id} and kind = 'api' and provider = 'polar'
          and status not in ('canceled', 'incomplete_expired') and (ended_at is null or ended_at > now())
        limit 1`)
    ).rows[0];
    if (paid)
      throw new Error(`${email} already has API card billing; credit is for accounts without it`);
    const existing = (
      await tx.execute<{ id: string; spend_cap_cents: number }>(sql`
        select id, spend_cap_cents from subscriptions
        where user_id = ${user.id} and kind = 'api' and provider = 'grant' and status = 'active'
        order by created_at desc limit 1 for update`)
    ).rows[0];
    const now = new Date();
    if (existing) {
      await tx.execute(
        sql`update subscriptions set spend_cap_cents = spend_cap_cents + ${cents}, updated_at = ${now} where id = ${existing.id}`,
      );
      return { id: existing.id, creditCents: existing.spend_cap_cents + cents };
    }
    const id = newId("sub");
    await tx.execute(sql`
      insert into subscriptions (id, provider, provider_subscription_id, user_id, email, kind, status,
        current_period_start, spend_cap_cents, card_seen_at, created_at, updated_at)
      values (${id}, 'grant', ${`grant_${id}`}, ${user.id}, ${email}, 'api', 'active',
        ${now}, ${cents}, ${now}, ${now}, ${now})`);
    return { id, creditCents: cents };
  });
  console.log(
    `billing:grant-credit: ${email} has $${(result.creditCents / 100).toFixed(2)} of API credit (${result.id})`,
  );
} finally {
  await client.end();
}
