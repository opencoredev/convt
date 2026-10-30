# CNV-101: one-time pricing (Desktop $39, Desktop + Cloud $69)

Status: plan, not implemented. Branch `leoplayz317/cnv-101-sell-one-time-products-only-desktop-at-39-and-a-desktop`.

Goal: the pricing page sells two one-time products, Desktop at $39 and Desktop + Cloud at $69 (a Desktop license plus 365 days of what Pro gives today). Pro monthly and yearly stop being sold but keep working for current subscribers. Old $29 orders keep granting what they did. Polar sandbox products are created during implementation; production Polar products and the deploy happen only after Leo merges.

## 1. What Pro delivers today

Pro is two separate grants, and they are checked in different places.

**A Pro license key.** `convergePro` (`packages/billing/src/converge.ts`) signs a `plan: "pro"` key whose `updates_until` is the end of the latest funded `payment_coverage` row (a paid, unrefunded, undisputed invoice that charged money or credit). `licenses_source_check` requires a Pro key to have a `subscription_id`. In `convt-license`, a Pro key covers builds dated up to `updates_until` and those builds keep working for good. A Desktop key is lifetime (`LIFETIME_UPDATES_UNTIL`). For local conversions, a Pro key gives nothing a Desktop key doesn't.

**Cloud conversions.** These are not gated by the key. They are gated by a live `subscriptions` row with `kind = 'pro'`, `status = 'active'` and `current_period_end > now()`:

- `cloudAllowance` (`packages/db/src/queries/cloud.ts`) gates the browser converter (`/dashboard/api/convert`, through `fetchCloudCredential`) and the desktop app's `/api/device/cloud` (`apps/web/src/server/device-auth.ts`). A trial is refused.
- convt-server re-checks the same row when it creates a job (`crates/convt-server/src/jobs.rs:122`) and counts the 50 GB monthly quota per `subscription_id` (`cloud_jobs.subscription_id` and `usage_events.subscription_id` are FKs to `subscriptions`).
- The desktop app (`crates/convt-app/src/cloud.rs`, `access`) shows the Cloud choice only when the stored key is a Pro key with `updates_until >= today`. `Licensing::offer_key` (`crates/convt-license/src/client.rs:491`) never replaces a stored Desktop key with a Pro key.

**Are the two Pro card features live?** The code has shipped. The cloud is only partly live:

- convt-server's jobs API is up in production: `POST https://convt-api-production.up.railway.app/v1/jobs` answers 415, not 404, so its routes are mounted, which means `DATABASE_URL`, the web token secret and `CONVT_SANDBOX_VERIFIED=1` are all set.
- The production `convt-web` Worker has no `CONVT_API_URL`. It is not in `apps/web/wrangler.jsonc` vars, and `wrangler secret list` shows only `BETTER_AUTH_SECRET`, `CONVT_WEB_TOKEN_SECRET` and `SEQUENZY_API_KEY`. Without it, `cloudConfig` returns `missing`. A paying Pro customer on convt.app then sees "Cloud conversion is being connected" in the browser converter, and the app's `/api/device/cloud` answers 503 `not_configured`. **So today a Pro customer can't use either cloud feature, from the browser, from a phone, or from the app.** (Check in the Cloudflare dashboard; a dashboard-set var would be wiped by the next `wrangler deploy` anyway.)
- "Convert from phone or browser" is the `/dashboard/api/convert` page. It works on a phone's browser, but you only reach it through a link on the API page (`dashboard/api.tsx:106`), shown when `SALES=all`.
- Railway Buckets CORS and the real-bucket upload/download were still NOT CHECKED according to `docs/p9-cloud-plan.md`. `api.convt.app` doesn't resolve yet (CNV-36).

What's missing before the $69 product is honest: set `CONVT_API_URL` (the Railway host until CNV-36) on the production web Worker, add it to `wrangler.jsonc` so deploys keep it, run the bucket acceptance test and one real browser and app cloud job, and give the browser converter its own link from the dashboard for Pro and Desktop + Cloud owners. This is Leo's call (question 1). The plan includes the link and the config line, but setting production config is a deploy step.

## 2. Modeling Desktop + Cloud

One one-time Polar order, `product = 'bundle'`, grants:

1. A Desktop key, exactly as a Desktop order does today: an `orders` row, `convergeDesktop`, lifetime key.
2. A **prepaid Pro year**: a `subscriptions` row with `kind = 'pro'`, `interval = 'year'`, `status = 'active'`, `current_period_start = billed_at` (or the end of an earlier live prepaid year, see section 4), `current_period_end = start + 365 days`, `ended_at = current_period_end`, `cancel_at_period_end = true`, and a new `order_id` pointing at the order. It has no Polar subscription id.
3. A `payment_coverage` row for that year (invoice = the order's mirrored invoice, `product = 'bundle'`), so the unchanged `convergePro` issues one Pro key with `updates_until` = the year's end.

Why put it in `subscriptions` instead of a new table: every cloud reader, in TypeScript and in Rust, and the quota and usage accounting already key on a `kind = 'pro'` subscription row. A separate `cloud_passes` table would mean changing `jobs.rs` and its sqlx queries, adding a pass FK to `cloud_jobs` and the append-only `usage_events`, and reworking the quota on the one path that meters paid usage. The prepaid row needs **no Rust change and no `cargo sqlx prepare`**. The cost is that every reader that talks to Polar must skip prepaid rows. They are listed below, and making `provider_subscription_id` nullable lets the compiler flag the Drizzle-typed ones.

### Schema (migration 0008)

In `packages/db/src/schema/billing.ts` and `billing-ops.ts`:

- `subscriptions.order_id text references orders(id)`, unique where not null.
- `subscriptions.provider_subscription_id` becomes nullable. New check `subscriptions_source_check`: `(order_id is null) = (provider_subscription_id is not null)`, and a prepaid row must have `kind = 'pro' and interval = 'year' and cancel_at_period_end and ended_at is not null`.
- `orders_product_check`: `('desktop', 'bundle')`.
- `payment_coverage_product_check`: add `'bundle'`.
- `catalogProducts` (checkouts check): add `'bundle'`.

Then run `drizzle-kit generate`, hand-write `migrations/down/0008_*.sql`, and check `sql/privileges.sql`. Grants are table-level (`convt_billing` select/insert/update, `convt_web`/`convt_server` select), so no new grant is expected; confirm it in `db:ci`. No sqlx prepare, because convt-server's queries don't change, but `db:ci` still runs its sqlx check. The migration is forward-only, and code that is already deployed keeps working on it.

### Billing code (`packages/billing`)

- `catalog.ts`: rework the types so illegal entries can't be written. `CatalogEntry` becomes a union on `kind`: `one_time` (`amountCents`), `recurring` (`amountCents`, `interval`, `trialDays`), `metered` (`unitAmount`). Add `sale: "open" | "subscribers_only"`, with `pro_month`/`pro_year` set to `subscribers_only`. Add `bundle` (`one_time`, 6900, `coverageDays: 365`) to local, sandbox and production. Replace `legacyDesktop` with `legacyPrices: Array<{ product: "desktop"; productId; priceId; amountCents }>` (section 3).
- `checkout.ts` `createCheckout`: refuse `subscribers_only` products with a new refusal, `not_on_sale`. `bundle` requires a signed-in, verified user, as Pro does today, because the cloud year belongs to an account.
- `ingest.ts` `checkFacts`: one-time orders (`desktop`, `bundle`) share one list-price-or-complimentary check against current and legacy prices of that product (section 3). `bundle` takes the Desktop path (`orders` table, invoice mirror, guest/foreign checkouts tolerated and claimed by email). `describeOrder`: "Desktop + Cloud, one year of cloud".
- `converge.ts`: a new `convergeBundle(orderId)`, called from `applyFacts` for `bundle` orders, does three things:
  - Runs `convergeDesktop` (plan `desktop`; its email is the only email).
  - Upserts the prepaid row (`on conflict (order_id)`) plus its coverage row. On full refund or a lost dispute, it sets the row to `status = 'canceled'` and `ended_at = least(ended_at, now)`.
  - Runs `convergePro` on it.

  `convergePro` changes in three ways. Its funded rule also accepts `i.order_id is not null`, because a one-time order already passed the price check, so a 100% code (section 6) still funds the key. It sends no `license_issued` email for prepaid rows. It sends no second `license_purchased` event for them.

- Readers that must skip prepaid rows (`order_id is null`), because they call Polar or mean "a real subscription":
  - `actions.ts` `liveSub` (cancel, resume, switch)
  - `deletion.ts:47` (cancels at Polar)
  - `reconcile.ts:138` (fetches from Polar), `:239` (trial sweep), `:396` (duplicate-subscription alert)
  - `checkout.ts` `liveSql` and `canStartProTrial` (a bundle owner is not "already Pro" for a Pro checkout that no longer exists, but `already_pro` must not block a bundle purchase)
  - `ingest.ts` `duplicateCheck`
  - `outbox.ts` renewal and trial emails (a prepaid row never goes `past_due`, but add the guard)

  Readers that should **include** them: `cloudAllowance`, `jobs.rs` (unchanged), `renewal.ts` `proSubscription` and `currentProAccess` (a live prepaid year is `pro`; after `ended_at` it's `lapsed`), `currentProKey`, `reconcile.ts:375` (funded coverage without a key), marketing `pro_status`, and `claim_purchases` (already attaches subscriptions by email).

- `packages/db/src/queries/cloud.ts` `cloudAllowance`: pick the row the same way `jobs.rs` does, newest usable first (`order by usable desc, created_at desc`). Otherwise a stacked future-start year, or a Pro subscription next to a bundle, shows "not allowed" in the browser while the API accepts the job.

### Web (`apps/web`)

- New `routes/checkout/bundle.ts` (`/checkout/bundle`, sign-in first, keeps `from=app`). `checkout-start.ts` takes `kind: "desktop" | "bundle"`. `/checkout/pro` answers 303 to `/#pricing` (see question 2 for `?from=app`).
- `server/views.ts`: a prepaid row shows as plan `kind: "bundle"`, "Desktop + Cloud", with the summary "Paid once. Cloud until <date>; it doesn't renew. Your Desktop license is lifetime." It has no cancel or switch. After it ends, the plan falls back to "Desktop (lifetime)" with "Cloud ended <date>" and a "Buy another year" link. `checkoutResultView` (line 469) maps `bundle` to its own product, not to `pro`. The key the app should use (line 177) is the Desktop key for bundle owners.
- `dashboard/billing.tsx`: remove "Start free trial" / "Start Pro" and the lapsed `/checkout/pro?interval=` link. Keep everything for live subscribers (switch, cancel, resume, portal, invoices).

### Desktop app (`crates/convt-app`)

`cloud::access` returns `Ready` when signed in and `account.access == Some(Access::Pro)`, as well as for a current stored Pro key. convt.app re-checks before it issues a credential, so this loosens nothing on the server. Without this change, a bundle buyer who activates the Desktop key (the one the dashboard shows) has no Cloud choice in the app. Only a new app release carries it. Builds up to 0.5.0 show Cloud only while the Pro key is the stored key (question 3). Also: `LICENSE_PRICE` `$29` to `$39` (`ui/mod.rs:199`). The Lapsed stage's "Get convt Pro" (`first_run.rs:523`) becomes "See pricing".

## 3. Desktop to $39

Edit the catalog directly and record the old price as legacy. Remove the `POLAR_DESKTOP_LIFETIME_*` env override.

The override is broken for a price change. `loadCatalog` swaps only the ids and keeps the catalog's `amountCents`. The `listPrice` check in `checkFacts` compares an order's item only against the _current_ price id. So after a switch, any refetch of an old $29 order fails: a refund webhook, a dispute through the reconciler, a backfill, or a version conflict. The failure reads `amount: Desktop is 3900, the order is 2900`, and the order is rejected and alerted. **A refund or lost chargeback on an old order would then never revoke its key.** `complimentaryDesktop` has the same problem, since it compares against `price.amountCents`. The override is a no-op today (its values equal the catalog's), so removing it changes nothing in production.

Steps:

- In Polar (sandbox now, production after merge), add a $39 price to the existing Desktop product and archive the $29 price. The product id stays and the price id is new. Record the old price in `legacyPrices` with `amountCents: 2900`. If Polar forces a new product instead, `legacyPrices` carries the old product id too.
- The list-price check, `complimentaryDesktop` (renamed `complimentaryOrder(product)`) and `productByPriceId`/`productByProviderId` match on the (price id, amount) pair from current or legacy. A legacy price is accepted only at its own recorded amount.
- Remove `POLAR_DESKTOP_LIFETIME_*` from `env.ts`, `apps/billing/wrangler.jsonc` (production and staging), `index.ts`, `cli/backfill.ts` and `test/unit/env.test.ts`.
- `scripts/deploy-worker.sh billing` refuses to deploy when the target catalog (production or sandbox) has a `PLACEHOLDER` id, so main can carry production placeholders until Leo creates the products.
- Old orders, claims, `convergeDesktop` and the dashboard read stored rows, not the catalog, so they keep showing what was paid ($29 or a discounted amount).

## 4. After the cloud year

Recommendation (question 4): buying Desktop + Cloud again adds 365 days of cloud, starting at the later of the purchase time and the end of the current live prepaid year. Each order also issues its own Desktop key. That's simple, every order is complete on its own, and the extra key could go to another person. The dashboard shows "Cloud until <date>" all year. From 30 days before the end, and after it, it shows "Buy another year" (`/checkout/bundle`). In v1 the year ending sends no email.

Alternatives for Leo: a cheaper "Cloud year" add-on for owners of Desktop or a past bundle (a third catalog product, same prepaid machinery, no second key). Or a reminder email 14 days before the end (a new outbox kind and template, plus a daily cron query).

## 5. Every price, trial and Pro-checkout surface

Web, public:

- `src/components/landing/pricing.tsx`: two cards, Desktop $39 and Desktop + Cloud $69. Drop the monthly/yearly toggle and the trial line. The API card is unchanged.
- `src/lib/site.ts:67-70` (`BUY_DESKTOP_URL` comment, `buyProUrl` becomes `BUY_BUNDLE_URL`), `src/lib/config.ts:14-15` (`buyPro` becomes `buyBundle`).
- `src/components/landing/closing.tsx:25` ("Try convt free for 7 days").
- `src/components/site/guide.tsx:193-205` (guide CTA "Start a 7-day trial").
- `routes/_site/guides/heic-to-jpg-mac.tsx:65,364` and `batch-heic-to-jpg-windows.tsx:48,303` (`cost`, trial and $29 copy, plus their `lastmod` in `site.ts`).
- `routes/_site/terms.tsx:67-80` (Trial, Desktop $29, Pro $12/$96). Keep the Pro terms for existing subscribers, and add Desktop + Cloud: one payment, 365 days of cloud, no renewal, refund rules. Bump the terms date.
- `routes/_site/contact.tsx:59-60` (FAQ "How does the free trial work?").
- `src/lib/llms.ts:22`, `landing-static/llms.txt:24-25`, `landing-static/index.md:32-35`, `landing-static/about.md:19`.
- JSON-LD and the sitemap: no `offers` in JSON-LD today, so nothing to change. Pages whose text changes get a new `lastmod`.
- `content/changelog.md`: history, leave it. Add a changeset (`bun run changeset`) for the new prices.

Web, account and checkout:

- `routes/_app/_shell/dashboard/index.tsx:38,68-69` ("Every format works free for 7 days", "Desktop is $29 once. Pro is $12 a month.", "7-day free trial").
- `routes/_app/_shell/dashboard/billing.tsx:113-114,151,222,266-271,296`.
- `server/views.ts:144,170-171,217` (`proPrice` stays for subscribers; `"$29, paid once"` uses the order's amount, not a constant).
- `routes/_app/checkout/success.tsx:116-130,209-212` (Desktop + Cloud heading and copy), `lib/checkout-copy.ts`.
- `routes/_app/_shell/dashboard/api_.convert.tsx:131,149-157` ("needs paid Pro" becomes "needs Pro or Desktop + Cloud", and the link goes to pricing, not "View Pro billing").

Email (`packages/mail`):

- `templates.ts` `license_issued` (line 109): the bundle variant names the cloud year and its end date. The trial-ending email (140-143) stays for open trials. Bump `templateVersion` and the snapshots.
- `marketing.ts:227,397` (campaign copy "7 days free"): a product call, question 5.

Elsewhere:

- `apps/extension/src/welcome/onboarding.ts:258` ("Free for 7 days, then $29 once") and its store listing in `apps/extension/README.md` if it says the same.
- `integrations/raycast` (`src/convt.tsx:63`, `README.md:24`): "requires a trial or license" stays true for the CLI's local trial. No change, and it ships through raycast/extensions anyway.
- Desktop app: `ui/mod.rs:199` (`$29`), `ui/first_run.rs` (trial and Lapsed stages, question 2), `ui/settings_window.rs:1007`, `ui/account.rs:75`, `ui/main_window.rs:603` (trial copy inside the app; leave it unless question 2 says otherwise).
- `crates/convt-license/src/client.rs:20-22` (`TRIAL_DAYS`, `BUY_URL` stay; the CLI's local trial is not a public page).
- `README.md`: no prices. `apps/docs`: no prices or trial (API docs only).
- `apps/billing/wrangler.jsonc:18` comment; `.agents/skills/test-convt-web/SKILL.md` and `test-convt-desktop/SKILL.md` (trial flow, fixtures, checkout routes); `CLAUDE.md` if the product list changes.
- `tools/billing-mock/src/mock.ts:40-70` (add `bundle`, the $39 Desktop price and the legacy $29 price), `pages.ts`, `preload.ts`.
- `packages/db/src/seed.ts`: a new fixture `bundle@convt.test` (live prepaid year, Desktop and Pro keys); `desktop@convt.test` keeps its $29 order.

## 6. Discounts

- **PRODUCTHUNT** (30%, Desktop and first 3 months of Pro monthly, ends 31 Oct 2026 PT): keep both production ids in the catalog so refetches of discounted launch orders still validate. Don't extend it to the bundle. If the change ships before it expires, Polar's product restriction makes it apply to the $39 price, and `discountProblem` already checks 30% of the order's own subtotal.
- **FAMILY** (100%, Pro monthly, production only): with Pro off sale it can't be used for new family members. Recommendation: add `bundle` to its catalog products and to the Polar discount after merge, and keep `pro_month` for subscriptions that already use it (question 6). The prepaid funded rule (section 2) makes a $0 bundle issue both keys. `complimentaryOrder` covers it.
- **ZORTOS_DISCORD** (100%, Desktop): stays Desktop-only and works at $39, because the complimentary check uses the matched price's amount.
- Sandbox: create one 100% test discount restricted to Desktop and Desktop + Cloud to exercise the complimentary path. Record it in the sandbox catalog.

## 7. Analytics and reconciliation

- `license_purchased` (`packages/billing/src/analytics.ts`, mirrored in `apps/web/src/server/analytics.ts` for claims) is sent once per license with `{ plan }`. A bundle would send two events (desktop and pro) and be counted twice. Change it to one event per purchase, `{ plan, product }` with `product` in `desktop | bundle | pro`: the bundle's event comes from its Desktop key (`insertId` unchanged, `license_purchased:desktop:<orderId>`), and both copies of `purchaseEventsFromLicenses` skip Pro keys whose subscription is prepaid. Check Leo's PostHog purchase insights (read-only) and note any that should split on `product`.
- `desktop_trial_started` is unchanged (only Polar trials send it).
- Reconciler `catalog_drift` (`reconcile.ts:301-334`) checks every catalog product at Polar: `bundle` is added. **Pro monthly and yearly must stay unarchived in Polar**, both for the drift check and because switching between them is a subscription update to the other product's price. Taking them off sale is done in our checkout code, not in Polar. Legacy prices are archived, which is intended, and they are not drift-checked.
- Backfill (`backfill.ts`, `cli/backfill.ts`) goes through ingest, so bundle orders follow automatically. Add a test with a legacy $29 order and a bundle order.

## 8. Implementation steps

1. Catalog types and legacy prices (`catalog.ts`), env override removal, `complimentaryOrder`, the list-price check. Unit tests (`test/unit/catalog.test.ts`, `parse.test.ts`, `env.test.ts`): a $29 legacy order with its refund and dispute; $39; complimentary at both prices; an unknown price; a legacy id at the wrong amount.
2. Schema and migration 0008, the down file and the privileges check; `bun run db:ci`.
3. Ingest, `convergeBundle`, `convergePro` funded/email/event changes, the prepaid guards in the listed readers, `cloudAllowance` ordering.
4. Billing mock products and preload, the seed fixture `bundle@convt.test`.
5. Checkout: `not_on_sale`, `/checkout/bundle`, `/checkout/pro` redirect.
6. Web views, dashboard, billing page, success page, converter page, pricing page and every copy surface in section 5; mail templates and snapshots.
7. Desktop app cloud gate and copy, with its tests in `crates/convt-app/src/cloud/tests.rs` and `ui/tests.rs`.
8. Polar sandbox: with the sandbox token from `~/.config/convt/secrets/polar-sandbox-billing-oat` (never printed), add the $39 price to the sandbox Desktop product, archive $29, create "convt Desktop + Cloud" at $69 one-time, and create the 100% test code. Write the ids into the `sandbox` catalog. Production entries stay `PLACEHOLDER` with a comment.
9. Changeset, docs (`docs/p7-billing-plan.md` note, test skills).
10. A Sol review of the diff, fixes, the checks below, then the PR.

After merge (Leo): create the production price and product (and FAMILY if approved). A follow-up commit fills the production ids. Then run the production migration, deploy billing, deploy web (billing first, because web's `/checkout/bundle` calls billing's new RPC product), set `CONVT_API_URL` on the web Worker, and release the app.

## 9. Test plan

Static: `bun run check && bun run check-types && bun run build`, `bun run rs:check`, and `cargo test` through `rust-test-limited` for `convt-license`, `convt-app` (headless tests, see `test-convt-desktop`) and `convt-server` (unchanged, but it runs against the new schema).

Unit and integration:

- `packages/billing`: `bun test test/unit && bun run test:integration`. Changed or new cases in `desktop.test.ts` (legacy $29 order: grant, refund revokes, lost dispute revokes, refetch accepted; $39 order; ZORTOS at $39), a new `bundle.test.ts` (both keys; prepaid row dates; refund revokes both and ends cloud; lost dispute; complimentary FAMILY bundle; second bundle stacks; guest bundle claimed by email; one `license_purchased`), `pro.test.ts` (existing subscriber renew, switch and cancel still work; `currentProAccess` for a bundle owner during and after the year; `can_start_trial` according to question 2), `checkout.test.ts` (`not_on_sale` for Pro, bundle needs sign-in), `reconcile.test.ts` (prepaid rows are never fetched from Polar; drift on `bundle`), `deletion.test.ts` (prepaid row isn't cancelled at Polar), `backfill.test.ts`, `analytics.test.ts`.
- `packages/db`: `bun run db:ci` (drift, down file, `cloud.test.ts` with prepaid and Pro rows side by side, convt-server sqlx check).
- `apps/web`: `bun test test/unit` (`views.test.ts`, `launch.test.tsx`, `dashboard-links.test.ts` updated to the bundle links and to no `/checkout/pro`) and `test:integration` (`device.test.ts`: `/api/device/cloud` allowed for a bundle owner).
- `tools/billing-mock` and `packages/mail` test suites.
- A repo-wide grep check (in `launch.test.tsx` or a new unit test) that no public route renders `/checkout/pro` or "7-day".

End to end with the billing mock (`bun run dev:web`, `db:seed`, `bun tools/billing-mock/src/preload.ts`, the `e2e/billing.sh` flow extended):

- **Desktop, old price:** the seeded `desktop@convt.test` $29 order; refund it through `/admin/refund` and run the reconciler cron; the key shows REFUNDED and no `rejected_fact` alert appears.
- **Desktop, new price:** `new@convt.test` buys `/checkout/desktop` at $39; one lifetime key, invoice $39.00, one email.
- **Bundle:** `new@convt.test` buys `/checkout/bundle`; the dashboard shows Desktop + Cloud with an end date a year out, a Desktop key and a Pro key; the browser converter is allowed (with the local convt-server from `test-convt-server`, run one real job); `/api/device/license` returns `access: pro`; `/admin/clock` +366 days shows the year ended and "Buy another year"; a second purchase stacks.
- **Existing Pro subscriber:** `pro@convt.test` renews (`/admin/renew`), switches yearly to monthly and back, cancels and resumes; keys keep extending; no Pro checkout link anywhere.
- Mailpit: the bundle license email and the unchanged Pro emails, in both themes.

Polar sandbox: run `dev:web` with `BILLING_CATALOG=sandbox`, the sandbox API URL and the sandbox token in `apps/billing/.dev.vars` (0600, gitignored, not committed). Buy Desktop $39 and Desktop + Cloud $69 with Stripe's test card in Polar's sandbox checkout through `agent-browser`. The success page's sync ingests without a webhook. Confirm both keys and the prepaid row. Refund the bundle from the sandbox dashboard or API, then run the reconciler.

Screenshots: with `agent-browser` (named session and profile), capture the pricing page at 1280 and 390 px in light and dark, and the dashboard home and billing for `bundle@`, `desktop@` and `pro@` at both widths, then look at each one. Load `frontend-design`/`interface-quality` before changing the cards. Desktop app screenshots under Xvfb per `test-convt-desktop`, only with Leo's permission to launch it.

## Risks

- The cloud isn't usable in production today (section 1). Selling $69 for it before `CONVT_API_URL` is set, and a real job is run, sells a feature that doesn't work.
- Prepaid rows in `subscriptions`: a reader added later that calls Polar without the `order_id is null` guard would act on a row Polar doesn't know. Mitigations: the nullable `provider_subscription_id` type, a reconcile test, and a comment on the table.
- App builds up to 0.5.0 show Cloud only with the Pro key stored. A bundle buyer who activates the Desktop key loses in-app cloud until they update (question 3).
- Removing the Pro checkout removes the only trial path in packaged apps (question 2).
- The production catalog carries placeholders between merge and the id fill. The deploy guard is what keeps them out of production.

## Open questions for Leo

1. **Cloud readiness.** convt.app can't issue cloud credentials in production (no `CONVT_API_URL` on `convt-web`), so Pro customers can't use the browser converter or app cloud today. Should we make the cloud work (set the var, run the bucket and browser checks) before Desktop + Cloud goes on sale, or launch Desktop $39 first and hold the bundle?
2. **The in-app trial.** Since 0.5.0 the packaged app has no local trial. A new account's only path is "Start your 7-day free trial", which opens `/checkout/pro?from=app`, a Polar Pro trial that becomes $12/month. If that goes, shipped apps show new users "Your Pro plan has ended. Get convt Pro". Options:
   - (a) Keep that app-only path for now. It's on no public page, but it still sells a subscription.
   - (b) Turn it off and accept the wrong "ended" copy in shipped apps.
   - (c) A no-card 7-day trial recorded by convt.app, returned as `access: trial`, which shipped apps already understand, with no Polar involved.

   Recommendation: (c) as its own issue, and (a) until it lands. Either way this is a licensing change only you can approve.

3. **Older apps and the bundle.** Should the dashboard show bundle owners the Desktop key (lifetime, but no in-app cloud on 0.5.0 or older) or the Pro key (cloud works on 0.5.0, but needs the Desktop key after the year)? Recommendation: the Desktop key, and ship the app's account-based cloud gate in the next release before or with the pricing switch.
4. **After the year.** Is "buy Desktop + Cloud again to add a year (another Desktop key comes with it)" right, or do you want a cheaper Cloud-year renewal for owners, or a reminder email before the year ends?
5. **Campaign email copy** in `packages/mail/src/marketing.ts` ("you've got 7 days free"): rewrite it now, or leave it for the marketing pass?
6. **FAMILY code.** Move it to Desktop + Cloud (100% off) so new family members still get cloud?
7. **Upgrade price for Desktop owners.** Should existing Desktop owners ($29 or $39) get the cloud for less than $69, for example a $30 code or the Cloud-year product from question 4?
