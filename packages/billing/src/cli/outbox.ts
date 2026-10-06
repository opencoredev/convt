// bun run billing:outbox list | resolve <outbox id> sent|resend
//
// Leo's tool for email outbox rows that need a person: `ambiguous` rows (Resend may
// or may not have sent them; look the outbox id up in Resend by its tag) and `dead`
// ones. `resolve <id> sent` records that it was delivered; `resend` queues it again
// under a new idempotency key. Connects as convt_billing with BILLING_DATABASE_URL
// (this checkout's own database by default).

import { connect } from "@convt/db";
import { servicesEnv } from "@convt/db/env";
import { logTransport } from "@convt/mail";
import { sql } from "drizzle-orm";

import { loadCatalog } from "../catalog";
import type { BillingContext } from "../context";
import { resolveOutbox } from "../outbox";
import type { BillingProvider } from "../provider";

const url = servicesEnv().BILLING_DATABASE_URL;
if (!url) {
  console.error("billing:outbox: BILLING_DATABASE_URL is not set");
  process.exit(2);
}
const [cmd, id, decision] = process.argv.slice(2);
const { client, db } = await connect(url);
try {
  if (cmd === "list" || !cmd) {
    const rows = await db.execute(sql`
      select id, kind, status, attempts, last_error, first_attempt_at from email_outbox
      where status in ('ambiguous', 'dead') order by updated_at desc limit 50`);
    for (const r of rows.rows) console.log(JSON.stringify(r));
    if (rows.rows.length === 0) console.log("billing:outbox: no ambiguous or dead rows");
  } else if (cmd === "resolve" && id && (decision === "sent" || decision === "resend")) {
    const ctx = {
      db,
      clock: () => new Date(),
      provider: {} as BillingProvider,
      catalog: loadCatalog("local"),
      signingKey: async () => {
        throw new Error("not needed");
      },
      mail: logTransport(),
      config: {
        siteUrl: "",
        mailFrom: "",
        alertEmail: null,
        downloadUrl: "",
        budgetMs: 5000,
        checkoutCookie: "",
      },
      log: console.log,
    } satisfies BillingContext;
    console.log(JSON.stringify(await resolveOutbox(ctx, id, decision)));
  } else {
    console.error("usage: bun run billing:outbox list | resolve <outbox id> sent|resend");
    process.exit(2);
  }
} finally {
  await client.end();
}
