// bun run analytics:backfill-signups
//
// Sends a `user_signed_up` event for every existing account, using the original
// created_at timestamp and `$insert_id=user_signed_up:<user id>` so a live signup
// and this backfill are the same event. Does not run in CI. Does not send email
// addresses or other PII.
//
// Local (this checkout's database only):
//   bun run analytics:backfill-signups            # dry-run
//   bun run analytics:backfill-signups --send     # needs POSTHOG_KEY
//
// Production or another remote database (do this on purpose):
//   OWNER_DATABASE_URL=... POSTHOG_KEY=phc_... \
//     bun run analytics:backfill-signups --send --remote --confirm-production
//
// Staging should keep POSTHOG_KEY empty; this script refuses to send without a key.

import { sql } from "drizzle-orm";

import { connect } from "../client";
import { servicesEnv } from "../env";
import { assertOwnedDatabase } from "../guard";

const args = new Set(process.argv.slice(2));
const send = args.has("--send");
const remote = args.has("--remote");
const confirmProduction = args.has("--confirm-production");

const env = servicesEnv();
const url = env.OWNER_DATABASE_URL ?? env.DATABASE_URL;
if (!url) {
  console.error("analytics:backfill-signups: OWNER_DATABASE_URL is not set");
  process.exit(2);
}
if (!remote) assertOwnedDatabase(url);
else if (!confirmProduction) {
  console.error(
    "analytics:backfill-signups: --remote also needs --confirm-production. This is not a CI step.",
  );
  process.exit(2);
}

const key = env.POSTHOG_KEY;
const host = (env.POSTHOG_HOST ?? "https://us.i.posthog.com").replace(/\/$/, "");
if (send && !key) {
  console.error("analytics:backfill-signups: POSTHOG_KEY is not set");
  process.exit(2);
}

const { client, db } = await connect(url);
try {
  const rows = await db.execute<{
    id: string;
    created_at: Date | string;
    provider_id: string | null;
  }>(sql`
    select u.id, u.created_at,
      (select a.provider_id from accounts a
        where a.user_id = u.id order by a.created_at asc limit 1) as provider_id
    from users u
    order by u.created_at asc`);

  let sent = 0;
  for (const row of rows.rows) {
    const method =
      row.provider_id === "github" || row.provider_id === "google" ? row.provider_id : "email";
    const createdAt = new Date(row.created_at).toISOString();
    const event = {
      api_key: key,
      event: "user_signed_up",
      distinct_id: row.id,
      timestamp: createdAt,
      properties: {
        $lib: "convt-backfill",
        $insert_id: `user_signed_up:${row.id}`,
        signup_method: method,
        backfill: true,
      },
    };
    if (!send) {
      console.log(`dry-run ${row.id} ${createdAt} ${method}`);
      continue;
    }
    const res = await fetch(`${host}/capture/`, {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify(event),
    });
    if (!res.ok) {
      console.error(`failed ${row.id}: ${res.status} ${await res.text()}`);
      process.exitCode = 1;
      continue;
    }
    sent++;
    console.log(`sent ${row.id} ${createdAt} ${method}`);
  }
  console.log(
    send
      ? `analytics:backfill-signups: sent ${sent}/${rows.rows.length}`
      : `analytics:backfill-signups: ${rows.rows.length} users (dry-run; pass --send to capture)`,
  );
} finally {
  await client.end();
}
