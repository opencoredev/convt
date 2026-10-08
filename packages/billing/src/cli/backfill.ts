// bun run billing:backfill
// bun run billing:backfill --apply
// bun run billing:backfill --apply --confirm-production
//
// Pulls every Polar order and creates any missing orders and Desktop licenses
// through the same ingest as a webhook, then attaches them to verified accounts
// with that email (claim_purchases). Safe to run twice: already-stored orders
// are skipped or re-ingested as no-ops.
//
// Default is a dry run: it prints the Polar orders we do not have and writes
// nothing. Pass --apply to write. Connects as convt_billing with
// BILLING_DATABASE_URL and the Polar / license env convt-billing uses.
//
// Do not run --apply against production from this checkout unless Leo is
// backfilling a missed purchase by hand. The 15-minute reconciler also picks
// up new Polar orders once ingest accepts them.

import { connect } from "@convt/db";
import { servicesEnv } from "@convt/db/env";
import { logTransport, resendTransport, sequenzyTransport } from "@convt/mail";

import { backfillPolarOrders } from "../backfill";
import type { BillingContext } from "../context";
import { loadCatalog } from "../catalog";
import { loadSigningKey, readBillingEnv } from "../env";
import { drainOutbox } from "../outbox";
import { createPolarProvider } from "../polar";

const apply = process.argv.includes("--apply");
const raw = servicesEnv();
let env;
try {
  env = readBillingEnv({
    ...raw,
    ENV: raw.ENV ?? "development",
    SITE_URL: raw.SITE_URL ?? "http://localhost:3000",
  });
} catch (e) {
  console.error(`billing:backfill: ${(e as Error).message}`);
  process.exit(2);
}

const url = raw.BILLING_DATABASE_URL;
if (!url) {
  console.error("billing:backfill: BILLING_DATABASE_URL is not set");
  process.exit(2);
}

if (
  apply &&
  (env.env === "production" || env.catalogEnv === "production") &&
  !process.argv.includes("--confirm-production")
) {
  console.error(
    "billing:backfill: --apply against production also needs --confirm-production. Dry-run first.",
  );
  process.exit(2);
}

const { client, db } = await connect(url);
try {
  const catalog = loadCatalog(env.catalogEnv);
  const mail =
    env.mail.transport === "resend"
      ? resendTransport({ apiKey: env.mail.apiKey, baseUrl: env.mail.apiUrl })
      : env.mail.transport === "sequenzy"
        ? sequenzyTransport({ apiKey: env.mail.apiKey })
        : logTransport();
  const ctx = {
    db,
    clock: () => new Date(),
    provider: createPolarProvider({
      accessToken: env.polar.accessToken,
      baseUrl: env.polar.apiUrl,
      webhookSecret: env.polar.webhookSecret,
      catalog,
      portalOrigin: env.polar.portalOrigin,
    }),
    catalog,
    signingKey: () => loadSigningKey(env),
    mail,
    config: {
      siteUrl: env.siteUrl,
      mailFrom: env.mail.from,
      alertEmail: env.alertEmail,
      downloadUrl: env.downloadUrl,
      budgetMs: 20_000,
      checkoutCookie: "convt_checkout",
    },
    log: console.log,
  } satisfies BillingContext;
  const result = await backfillPolarOrders(ctx, { dryRun: !apply });
  console.log(JSON.stringify(result, null, 2));
  if (result.dryRun) {
    console.log(
      result.missing.length
        ? `billing:backfill: ${result.missing.length} Polar order(s) are not in the database. Re-run with --apply to ingest them.`
        : "billing:backfill: every Polar order is already stored.",
    );
  } else {
    const drained = await drainOutbox(ctx);
    console.log(`billing:backfill: outbox drain ${JSON.stringify(drained)}`);
  }
} finally {
  await client.end();
}
