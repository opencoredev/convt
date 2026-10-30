# CNV-101: one-time pricing (Desktop $39, Desktop + Cloud $69)

Status: revision 2, implemented on this branch. Revision 2 followed Leo's decisions and a GPT-6.1 Sol review of revision 1. The sandbox and production Polar products were created on 2026-10-30 and their ids are in the catalog. A sandbox bundle checkout passed the same day (see "Polar sandbox" below). Branch `leoplayz317/cnv-101-sell-one-time-products-only-desktop-at-39-and-a-desktop`.

Goal: the pricing page sells two one-time products, Desktop at $39 and Desktop + Cloud at $69 (a Desktop license plus 365 days of the cloud features Pro has). Pro monthly and yearly stop being sold, but keep working for current subscribers: renewals, the monthly/yearly switch, cancel, the dashboard and their keys. Old $29 orders keep granting what they did. Products go into the Polar sandbox now. Production Polar products and the deploy come after Leo merges.

## Decisions

1. Making the cloud work in production is CNV-103, a separate worktree that owns `apps/web/wrangler.jsonc` vars and `cloud-credential.ts`. This PR touches neither. It ships Desktop $39 and Desktop + Cloud $69 together. The deploy is the gate: the bundle goes on sale only when this PR is deployed, and that happens only after CNV-103 is verified. There is no feature flag.
2. The in-app trial (the packaged app's "Start your 7-day free trial", which opens `/checkout/pro?from=app`) keeps working as it does today. Public pages drop the trial copy. The campaign emails keep theirs.
3. Bundle buyers see the Desktop key. The desktop app learns to offer Cloud when the account says it has Pro access, so a Desktop key plus a live cloud year gets Cloud in the next release.
4. Buying Desktop + Cloud again adds a year (rule in section 3).
5. FAMILY stays on Pro monthly. There is no upgrade price for Desktop owners. PRODUCTHUNT is not applied to the new prices.
6. The cloud allowance stays one 50 GB monthly allowance per account, summed across every Pro entitlement the account has, including open reservations.

## 1. What Pro gives today, and what the bundle reuses

A Pro subscription grants a `plan: "pro"` license key and cloud conversions. Cloud is gated by a live `subscriptions` row (`kind = 'pro'`, `status = 'active'`, `current_period_start <= now() < current_period_end`, `ended_at` unset or later): in `cloudAllowance` (`packages/db/src/queries/cloud.ts`, used by the browser converter and `/api/device/cloud`) and again in convt-server when a job is created (`crates/convt-server/src/jobs.rs`). The 50 GB counts `usage_events` and open `cloud_jobs` reservations. Both reference `subscriptions`. In production the convt-server API is live, but convt-web has no `CONVT_API_URL`, so nobody can use the cloud yet. That is CNV-103.

## 2. Model

A Desktop + Cloud order is a one-time Polar order for the catalog product `bundle`. It grants:

- **A Desktop key.** The order goes into `orders` (`product = 'bundle'`), and `convergeDesktop` issues the lifetime Desktop key exactly as for a Desktop order. It sends one `license_issued` email and one `license_purchased` event.
- **A prepaid cloud year.** A `subscriptions` row with `order_id` set, `kind = 'pro'`, `interval = 'year'`, `cancel_at_period_end = true`, no Polar subscription id, and dates from the schedule in section 3. Every cloud reader already accepts this row, in TypeScript and in Rust, so nothing new is needed to open the cloud for it.

**The bundle issues no Pro key.** Revision 1 issued one through `payment_coverage` and `convergePro`. The review showed what that costs:

- A lost chargeback on a one-time order records the dispute against `disputes.order_id`, while Pro revocation only looks at `invoice_id`.
- The funded rule skips $0 items.
- Pro key dates are immutable (`licenses_guard_update`), so a refund of an earlier stacked year can't move a later key's `updates_until`.

The key would buy nothing. The Desktop key already covers every build, the server checks cloud access, and app builds up to 0.5.0 keep a stored Desktop key over any Pro key (`offer_key`), so they would never use it. Without a key, refunds, chargebacks, complimentary orders and stacking are all plain updates to a mutable coverage row. Bundle buyers on 0.5.0 or older get Cloud only after updating to the release that carries the app fix (section 6).

### Schema: migration 0008

- `subscriptions.order_id text references orders(id)`, unique where not null.
- `subscriptions.provider_subscription_id` becomes nullable. A new check `subscriptions_prepaid_check` requires `(order_id is null) = (provider_subscription_id is not null)`, and a prepaid row must have `kind = 'pro'`, `interval = 'year'`, `cancel_at_period_end`, and both period dates and `ended_at` set.
- `orders_product_check`: `('desktop', 'bundle')`.
- `checkouts_product_check` (`catalogProducts`): add `bundle`.
- Steps: `drizzle-kit generate`, a hand-written `migrations/down/0008_*.sql`, and a check of `sql/privileges.sql`. Grants are table-level (`convt_billing` select/insert/update on `subscriptions`), so none are added. `cargo sqlx prepare` runs because the convt-server quota queries change (section 5). The migration is forward-only, and code that is already deployed works on it.

## 3. The cloud schedule, refunds and chargebacks

`scheduleCloud(user)` in `packages/billing/src/converge.ts` runs under the user lock (`applyFacts` takes `user:<id>` first) every time a bundle order of that user converges. It walks the user's bundle orders in `(billed_at, id)` order and keeps a running `end`, which starts at the earliest billed time:

- **An order in good standing** (`paid` or `partially_refunded`, and no `lost` dispute) gets `start = max(billed_at, end)` and `end = start + 365 days`. Its row is upserted with `status = 'active'`, those period dates, and `ended_at = end`.
- **An order that is refunded in full or lost a dispute** keeps its row with `status = 'canceled'`. `canceled_at` and `ended_at` are set to `least(its scheduled end, the time it was canceled)`. Those dates are frozen once set. The canceled order contributes `max(end, its ended_at)` to the running end, which is the time it actually covered and nothing more.
- An order that never paid (`pending`, `void`) has no row.

The result:

- **A repeat purchase stacks.** Bought while a year is live, it starts when that year ends.
- **A refund of an earlier year moves later years forward.** They start when the refund happened, or at their own purchase time if that is later, so no paid time is lost and none is granted twice. A row whose period already started never moves backward, because everything before it ended before it started.
- Past rows don't change. Only rows that are canceled now, or that start after the change, get new dates.

A bundle order with no account (a Polar-hosted checkout, which the Desktop path already tolerates) gets a row scheduled on its own, by `billed_at`. `claim_purchases` attaches it later by email. Bundle checkouts on convt.app require sign-in, so a buyer only ends up in this case by going around the site.

A regular Pro subscription and a bundle can coexist: a monthly subscriber who buys Desktop + Cloud keeps their subscription and its controls. The bundle year does not stack on a recurring period, and the allowance stays one per account (section 5).

Refund and dispute paths already reach the order: a refund webhook, the reconciler's dispute fetch (`applyDispute` targets `orders` first) and the backfill all end in `convergeDesktop` for the order. `convergeBundle` wraps it and then runs `scheduleCloud`. The Desktop key is revoked by the existing code (`refunded` or `dispute_lost`), and cloud ends in the same transaction.

## 4. Desktop to $39 and the catalog

The catalog is edited directly and the old price is recorded as legacy. The `POLAR_DESKTOP_LIFETIME_*` env override goes away: it swapped ids but kept `amountCents`, and the list-price check only compared against the current price id. So after a price change, any refetch of a $29 order (a refund, a dispute, a backfill, a version conflict) would have been rejected, and that key would never have been revoked.

- `Catalog.legacyPrices: Array<{ product: "desktop"; productId; priceId; amountCents }>` replaces `legacyDesktop`. A legacy price is accepted only at its own recorded amount.
- `listPriceOf(catalog, product, priceId)` returns the current or legacy amount for that (product, price) pair. The one-time order check in `checkFacts` and `complimentaryOrder` (renamed from `complimentaryDesktop`, now for any one-time product) use it.
- Local: Desktop moves to `price_local_desktop_39` at 3900, `price_local_desktop` stays as legacy at 2900, and the bundle is `prod_local_bundle` / `price_local_bundle` at 6900.
- Sandbox: real ids from the sandbox org.
- Production: real ids from the production org; the $29 Desktop price is recorded as legacy at 2900.
- `pro_month` and `pro_year` stay in the catalog and stay unarchived in Polar. The reconciler's drift check needs them, and switching intervals updates a subscription to the other product's price. New sales stop in our checkout code, not in Polar.
- `scripts/deploy-worker.sh` refuses to deploy web or billing while the target catalog has a `PLACEHOLDER` id. Both catalogs are now filled, so this guard only catches a future product added without ids.

## 5. Readers of `subscriptions`

Every reader either calls Polar or means "a recurring subscription", and the ones that do must skip prepaid rows (`order_id is null`):

- `actions.ts` `liveSub` (cancel, resume, switch)
- `deletion.ts` (revokes at Polar; prepaid rows are ended locally first, see below)
- `reconcile.ts` (provider fetch, trial sweep, duplicate-subscription alert)
- `checkout.ts` `liveSql`, `canStartProTrial` (the trial check) and the account-wide fallback in `stateOf`
- `ingest.ts` `duplicateCheck`
- `outbox.ts` subscription emails
- `marketing.ts` paid status: a prepaid year counts as paid
- `packages/db/src/queries/state.ts` and `apps/web/src/server/views.ts`: the recurring plan and its controls render from recurring rows only. The cloud year is a separate `cloud` field on the overview and billing views.

Changes in the readers that should include prepaid rows:

- **Renewal** (`renewal.ts` `proSubscription`): a prepaid row counts only while `current_period_start <= now < current_period_end`, the same test convt-server applies. The app and the API then agree on a stacked year that hasn't started.
- **Quota.** One allowance per account. `cloudAllowance` and convt-server's job creation pick the newest usable Pro row for the new job's `subscription_id`. Then they sum `pro_bytes` usage (with its corrections) and open reservations over **every** Pro subscription of the user since the start of the UTC month. Append-only usage stays as it is. Job creation already holds a per-user advisory lock, so the sum stays serialized. `cargo sqlx prepare` regenerates `.sqlx`.
- **Account deletion.** `delete_user` refuses while any subscription is live. `advanceDeletion` ends the user's prepaid rows locally first (`status = 'canceled'`, `ended_at = least(ended_at, now)`), keeps them and their usage, then revokes the Polar subscriptions as before.
- **Checkout result.** `stateOf` sends `bundle` through the same order-linked path as `desktop`, so the success page shows that order's Desktop key and never an older Pro key found account-wide.

## 6. Desktop app (`crates/convt-app`)

- `cloud::access` takes the account's last answer as well. It returns `Ready` when signed in and either the stored key is a current Pro key or convt.app said `access: pro`. convt.app re-checks before it issues a credential, so this loosens nothing on the server.
- `renew_if_due` also asks once per run when a signed-in computer has a Desktop key and no answer yet, so Cloud shows after a restart instead of the next day.
- `LICENSE_PRICE` goes from `$29` to `$39`. The Lapsed stage's "Get convt Pro" becomes "See pricing".
- The trial flow is untouched (decision 2).

## 7. Surfaces

Public web (drop trial copy, show new prices, link `/checkout/desktop` and `/checkout/bundle` only):

- `components/landing/pricing.tsx` (two cards, no toggle; the API card stays), `closing.tsx`
- `components/site/guide.tsx`, both HEIC guides and their `lastmod` dates
- `routes/_site/terms.tsx` (adds Desktop + Cloud; keeps the Pro terms for current subscribers), `contact.tsx`
- `lib/llms.ts`, `landing-static/llms.txt`, `landing-static/index.md`, `landing-static/about.md`
- `lib/site.ts`, `lib/config.ts`
- `apps/extension/src/welcome/onboarding.ts`

Account: dashboard home, licenses (bundle keys read "Desktop + Cloud, bought …"), billing (a cloud card with its end date and "Buy another year"; recurring controls only for recurring subscriptions; no "Start free trial" or Pro checkout links), checkout success, the browser converter's "needs Pro" card.

`/checkout/bundle` is new and requires sign-in. `/checkout/pro` without `from=app` answers 303 to `/#pricing`; with `from=app` it works as today.

Email: the `license_issued` template gains a bundle variant naming the cloud end date (bump `templateVersion` and its snapshots). Campaign emails are unchanged.

Unchanged: Raycast, the CLI's local trial, README and the docs site (no prices), the changelog (history). Add a changeset.

## 8. Discounts

- **PRODUCTHUNT:** both production ids stay, so refetches of discounted launch orders validate. Products stay `desktop` and `pro_month`. Polar restricts a code by product, not by price, so the catalog also lists the prices it applies to (`priceIds`): the $29 Desktop price and Pro monthly's price. An order at $39 with the code is rejected. It ends 31 Oct 2026 in Polar.
- **FAMILY:** Pro monthly only, unchanged.
- **ZORTOS_DISCORD:** 100% off Desktop; it works at $39 through `complimentaryOrder`.
- A 100% giveaway code on a bundle is accepted the same way a complimentary Desktop order is. Both representations (a 100% discount, or an item Polar zeroed) grant the key and the cloud year, because neither depends on money funding a key.

## 9. Analytics and reconciliation

- `license_purchased` comes from the Desktop key only, so a bundle counts once. A new `product` property (`desktop`, `bundle`, `pro`) is set in both copies of the event builder (convt-billing and the web claim path).
- The reconciler drift check covers `bundle` and the current Desktop price. Legacy prices are archived on purpose and not checked.
- The backfill goes through ingest, so bundle orders follow automatically.

## 10. Implementation order

1. Catalog: legacy prices, `bundle`, removal of the env override; unit tests.
2. Schema and migration 0008, the down file; `db:ci`.
3. Billing: ingest, `convergeBundle` with `scheduleCloud`, the prepaid guards, deletion, checkout result, renewal, analytics; integration tests.
4. Quota aggregation in TypeScript and Rust, `cargo sqlx prepare`.
5. Billing mock, seed fixture `bundle@convt.test`.
6. Web: checkout routes, views, dashboard, billing page, pricing and the copy surfaces, mail template.
7. Desktop app.
8. Polar sandbox products and sandbox catalog ids.
9. Verification (section 11).

## 11. Verification

Static and suites: `bun run check && bun run check-types && bun run build`; `bun run rs:check`; `rust-test-limited` for `convt-license`, `convt-app` and `convt-server`; `packages/billing` unit and integration; `tools/billing-mock`; `packages/mail`; `apps/web` unit; `bun run db:ci`.

Locally with `bun run dev:web` and the billing mock:

- Buy Desktop at $39.
- Stage a $29 legacy order: set the mock's Desktop price back to the legacy id and amount, buy, restore. Refund it and confirm the key revokes with no `rejected_fact` alert.
- Buy the bundle: Desktop key, cloud until a year out, browser converter allowed.
- Buy it again: the new year stacks.
- Refund the first bundle: the second year moves to the refund time.
- Lose a chargeback on a bundle: Desktop key DISPUTED, cloud ended.
- The existing Pro fixture renews, switches and cancels.
- Delete an account that holds a bundle.

Polar sandbox: one bundle checkout with Stripe's test card, ingested through the success page's sync.

Screenshots with `agent-browser`: the pricing page and the billing page at 1280 and 390 px.

## Polar sandbox

The sandbox token on ms-a2 (`~/.config/convt/secrets/polar-sandbox-billing-oat`) can't create products (`403 insufficient_scope`), so both orgs' products were made in the Polar dashboard on 2026-10-30: "convt Desktop" moved to a $39 one-time price and "convt Desktop + Cloud" was created at $69.

The sandbox check ran `dev:web` with billing pointed at the sandbox (`BILLING_CATALOG=sandbox`, the sandbox API URL and token) and bought the bundle with Stripe's test card. The success page showed the Desktop key, the order row was `bundle` at 6900 (Polar adds sales tax on top; the list-price check uses the subtotal), the prepaid row lasted 365 days with no Polar subscription, and the billing page showed the cloud year. Polar refuses `.test` addresses, so sign in with a real domain for this check.

## 12. Production rollout (after merge, by Leo or with his go-ahead)

Steps 1 to 3 were done on 2026-10-30:

1. CNV-103 is deployed and a real cloud job has run. The sandbox products exist and a sandbox bundle checkout has passed (above).
2. In the production Polar org "convt", "convt Desktop" (`9d3028c1-…`) moved to a $39 one-time price, replacing `a68e7412-…`, and "convt Desktop + Cloud" was created with one $69 one-time USD price. Pro monthly, Pro yearly and the API product are unchanged. PRODUCTHUNT ends 31 Oct 2026 and the catalog refuses it on $39 either way.
3. The production catalog has the new price and product ids.

Polar starts charging $39 as soon as step 2 is done, but production billing only knows the $29 price until step 4 is deployed. Do steps 2 to 4 together.

4. Run migration 0008 on production (forward-only), then deploy convt-billing, then convt-web. The order matters because web's `/checkout/bundle` calls billing's new product.
5. Release the desktop app with the Cloud fix.

## Notes from implementation

- The terms keep a short clause on the Pro trial, because the app still starts it (decision 2). Marketing pages, guides, llms.txt and the dashboard dropped the trial copy.
- `apps/web/e2e/sales.sh` already expected "Pro is coming soon", which no code renders anymore. It was stale before this branch and is left as it was.

## Risks

- A future reader of `subscriptions` that calls Polar must skip prepaid rows. The nullable `provider_subscription_id` makes Drizzle-typed readers handle the null, and the reconcile and deletion tests cover the raw SQL ones.
- Bundle buyers on app 0.5.0 or older have no in-app Cloud until they update. The browser converter works for them.
