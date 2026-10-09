// bun run marketing:backfill
//
// Subscribes every existing account that has never had a marketing row to
// campaign email (Leo's decision, 2026-10-09). Accounts that unsubscribed keep
// their choice. Writes database rows only; convt-billing's per-minute cron pushes
// them to Sequenzy once SEQUENZY_MARKETING_API_KEY is set, and holds accounts
// whose email is unverified. Prints user ids, never email addresses.
//
// Local (this checkout's database only):
//   bun run marketing:backfill                  # dry run: what would change
//   bun run marketing:backfill --apply          # enroll
//   bun run marketing:backfill --apply --resync # also push every subscriber again
//
// Production (do this on purpose, after the privacy policy change is live):
//   OWNER_DATABASE_URL=... bun run marketing:backfill --remote --confirm-production
//   OWNER_DATABASE_URL=... bun run marketing:backfill --apply --remote --confirm-production

import { connect } from "../client";
import { servicesEnv } from "../env";
import { assertOwnedDatabase } from "../guard";
import { applyMarketingBackfill, planMarketingBackfill } from "../queries/marketing";

const args = new Set(process.argv.slice(2));
const known = new Set(["--apply", "--resync", "--remote", "--confirm-production"]);
for (const a of args) {
  if (!known.has(a)) {
    console.error(`marketing:backfill: unknown option ${a}`);
    process.exit(2);
  }
}
const apply = args.has("--apply");
const resync = args.has("--resync");
const remote = args.has("--remote");
if (resync && !apply) {
  console.error("marketing:backfill: --resync needs --apply");
  process.exit(2);
}

const env = servicesEnv();
const url = env.OWNER_DATABASE_URL;
if (!url) {
  console.error("marketing:backfill: OWNER_DATABASE_URL is not set");
  process.exit(2);
}
if (!remote) assertOwnedDatabase(url);
else if (!args.has("--confirm-production")) {
  console.error("marketing:backfill: --remote also needs --confirm-production");
  process.exit(2);
}

const { client, db } = await connect(url);
try {
  const plan = await planMarketingBackfill(db);
  const unverified = plan.missing.filter((m) => !m.verified).length;
  for (const m of plan.missing)
    console.log(
      `${apply ? "enroll" : "dry-run"} ${m.userId} ${m.createdAt.toISOString()}${m.verified ? "" : " (held until verified)"}`,
    );
  console.log(`existing rows: ${JSON.stringify(plan.existing)}`);
  if (!apply) {
    console.log(
      `marketing:backfill: ${plan.missing.length} account(s) to enroll, ${unverified} of them unverified (dry run; pass --apply to write)`,
    );
  } else {
    const result = await applyMarketingBackfill(db, { resync });
    console.log(
      `marketing:backfill: enrolled ${result.enrolled}${resync ? `, queued ${result.resynced} for a fresh push` : ""}`,
    );
  }
} finally {
  await client.end();
}
