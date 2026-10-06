# P7 plan: billing and license issuance

This plan adds payments to convt.app: checkout for Desktop, Pro and the API, a webhook handler that records purchases and issues signed license keys, the customer portal, transactional email, account deletion, and a reconciler. It builds on the P6 schema and pages. Nothing in this phase touches a real payment account, a real webhook, real email or the production signing key. Everything runs locally against a mock that speaks Polar's and Resend's APIs and signs webhooks with Polar's real scheme, plus Mailpit and the dev signing key. The desktop app and the CLI stay network-free and unchanged.

Status of the inputs, checked 2026-10-05: P6 is done locally (see [the P6 plan](p6-accounts-plan.md)). The TypeScript signer in `packages/license` passes the Rust vectors. The billing page reads Pro, API and invoices from Postgres, and every action on it is a placeholder. The pricing buttons link to `/sign-in?plan=...`. The Paper file could not be opened (HTTP 502 from the Paper connection), so no P7 artboard is known to exist; section 9 lists the screens built without a design.

## Decisions

Leo did not answer the open questions, so these defaults stand. Each is reversible.

1. Polar is the payment provider, as merchant of record.
2. No license key is issued during a Pro trial, and keys get no expiry field. During the trial the desktop app runs on its own 7-day local trial.
3. The API price stays a $0.01 placeholder. A price that is not a whole number of cents is refused until P9 defines precision and rounding.
4. A spend cap is required to enroll in the API: $20 a month suggested, minimum $1, editable.
5. A partial refund keeps a key. A full refund or a lost dispute revokes it on the dashboard.
6. Switching between monthly and yearly takes effect at once, using Polar's `invoice` proration.
7. Keys are emailed for a Desktop purchase and a subscription's first Pro key. Renewed Pro keys appear on the dashboard and P8 fetches them.
8. Desktop can be bought without an account. Pro and the API require one.

## 1. Provider

### Boundary

Everything above one TypeScript interface, `BillingProvider` in `packages/billing/src/provider.ts`, speaks our catalog names (`desktop`, `pro_month`, `pro_year`, `api`) and normalized facts. Only the Polar adapter knows Polar's ids, payloads and signature scheme. A Stripe adapter would produce the same facts (Checkout Sessions, `invoice.paid`, `customer.subscription.*`, `charge.refunded`, `charge.dispute.closed`, `Stripe-Signature`, the Billing Portal, Billing Meters).

```ts
type Proration = "invoice" | "next_period";

interface BillingProvider {
  name: "polar" | "stripe";
  verifyWebhook(raw: Uint8Array, headers: Headers, now: Date): Promise<Delivery | Rejected>;
  toFacts(delivery: Delivery): ProviderFacts; // snapshots, plus hints that need a fetch
  createCheckout(input: CheckoutInput): Promise<{ providerCheckoutId: string; url: string }>;
  getCheckout(id: string): Promise<CheckoutFact>;
  getOrder(id: string): Promise<OrderFact>;
  getSubscription(id: string): Promise<SubscriptionFact>;
  getDispute(id: string): Promise<DisputeFact>;
  scan(
    kind: "orders" | "subscriptions" | "refunds" | "disputes",
    from: ScanCursor,
  ): Promise<ScanPage>;
  paymentMethods(userId: string): Promise<CardFact[]>;
  changeProduct(
    subscriptionId: string,
    product: "pro_month" | "pro_year",
    proration: Proration,
  ): Promise<SubscriptionFact | PaymentFailed>;
  setCancelAtPeriodEnd(subscriptionId: string, value: boolean): Promise<SubscriptionFact>;
  revokeSubscription(subscriptionId: string): Promise<SubscriptionFact>;
  portalUrl(userId: string): Promise<string>;
  receiptUrl(providerOrderId: string): Promise<string>;
  settings(): Promise<OrganizationFacts>; // multiple subscriptions, trial abuse, emails
  // P9: reportUsage(records), each record's id as the idempotency key.
}
```

Facts:

- `OrderFact`: provider order, customer, checkout and subscription ids; our user id from the customer's `external_id`; email; catalog product; reason (`purchase`, `subscription_create`, `subscription_cycle`, `subscription_update`, `subscription_meter_cycle`); status; net, applied-balance, refunded amounts in cents; currency; `billed_at` (the provider's `created_at`); `version`; and its coverage items.
- `CoverageItem`, one per order line with a catalog Pro price: provider item id, catalog product, period start and end (the item's `start_timestamp` and `end_timestamp`), amount in cents (negative for a credit), and `proration`.
- `SubscriptionFact`: ids, user, email, catalog product, status, trial end, current period, `cancel_at_period_end`, canceled, ends and ended times, `pending_update`, `version`.
- `DisputeFact`: id, order id, status, amount, `closed`, `version`.
- `version` is the provider's `modified_at`, or `created_at` when `modified_at` is null (Polar leaves it null on objects never modified). Every fact also carries a SHA-256 of its normalized fields.

### What was checked against Polar and Resend

Read on 2026-10-05 in `@polar-sh/sdk` 1.0.2 (types for API version `2026-10`, the current version in its README) and Polar's and Resend's docs:

- **Signatures.** Polar's delivery docs: secrets created before 8 September 2026 use HMAC with the UTF-8 bytes of the full `whsec_...` string as the key; secrets created on or after that date follow Standard Webhooks (base64-decode of the part after `whsec_`). The SDK tries both. Polar sends one `webhook-signature`.
- **Delivery.** Up to 10 retries with exponential backoff, a 10-second timeout (2 seconds recommended), no redirects followed, and the endpoint is disabled after 10 consecutive failures. No ordering guarantee is documented.
- **Events** (`WebhookEventType`): `checkout.created|updated|expired`, `order.created|updated|paid|refunded`, `subscription.created|updated|active|canceled|uncanceled|cycled|revoked|past_due|paused|resumed|migrated`, `refund.created|updated`, `customer.*`, `organization.updated`, and others we ignore. There is no dispute event.
- **Order.** Statuses `draft`, `pending`, `paid`, `refunded`, `partially_refunded`, `void`. Fields include `paid`, `net_amount`, `applied_balance_amount`, `due_amount`, `refunded_amount`, `billing_reason`, `checkout_id`, `subscription_id`, `customer.external_id`, `created_at` and a nullable `modified_at`. There is no `paid_at`. Items carry `amount`, `proration`, `product_price_id`, `start_timestamp` and `end_timestamp`. Polar's docs say orders with a zero total are marked paid immediately.
- **Subscription.** Statuses `incomplete`, `incomplete_expired`, `trialing`, `active`, `past_due`, `canceled`, `unpaid`, `paused`.
- **Lists.** Page-number pagination (`page`, `limit`, `max_page`). Orders filter by `created_after` and sort by `created_at`; subscriptions sort by `started_at`, `current_period_end`, `ended_at` and similar, with no modified filter or sort; refunds and disputes sort by `created_at`. Nothing lists by modification time.
- **Disputes.** `GET /v1/disputes` with statuses `prevented`, `early_warning`, `needs_response`, `under_review`, `lost`, `won`. A refund can carry the dispute it prevented (`Refund.dispute`). The docs do not say what a lost chargeback does to the order.
- **Proration.** `SubscriptionUpdate` takes `product_id` and `proration_behavior` (`invoice`, `prorate`, `next_period`, and a preview `reset`). Polar's proration docs: an interval change promotes `prorate` to `invoice`; with `invoice`, a downgrade is credited on a new invoice; and "the subscription update is applied only if the immediate payment (if any) succeeds. If the payment fails, the API returns an error and the subscription stays unchanged."
- **Organization settings.** `allow_multiple_subscriptions` (Polar's docs: off by default, one active subscription per customer per organization), `prevent_trial_abuse`, and per-email toggles including `subscription_trial_conversion_reminder` and `subscription_past_due`.
- **Checkout.** `CheckoutCreate` has `allow_trial`, `external_customer_id`, `customer_email`, `currency`, `metadata` and `success_url`; checkout sessions report `is_payment_required` and `is_payment_setup_required`. Customer sessions take `external_customer_id`. `customers.listPaymentMethods` returns card brand, last four and expiry.
- **Resend.** Idempotency keys (header `Idempotency-Key`, up to 256 characters) are kept 24 hours. Reusing a key with a different payload returns 409 `invalid_idempotent_request`; a key whose first request is still running returns 409 `concurrent_idempotent_requests`, safe to retry later.

Still unconfirmed, and checked in a Polar sandbox before launch (step 10, which needs Leo's sandbox organization):

- **Blocking:** what a checkout for the metered-only `api` product produces: whether it collects and saves a card, the initial order (expected: `subscription_create` at $0, marked paid) and the subscription status sequence. Enrollment does not depend on the answer (section 4), but launch does.
- What a trial produces (a $0 `subscription_create` order is expected), what a lost chargeback does to the order, and whether the checkout lets the buyer change an email we pass.

### Catalog and organization settings

`packages/billing/src/catalog.ts` holds, per environment (`local`, `sandbox`, `production`), each product's provider product and price ids, amount, currency and interval, plus the switch policy. Only these products create entitlements.

| Catalog name | Polar product                          | Price                                                                          | Trial                 |
| ------------ | -------------------------------------- | ------------------------------------------------------------------------------ | --------------------- |
| `desktop`    | "convt Desktop", one-time              | $29.00 USD fixed                                                               | none                  |
| `pro_month`  | "convt Pro (monthly)", recurring month | $12.00 USD fixed                                                               | 7 days, card required |
| `pro_year`   | "convt Pro (yearly)", recurring year   | $96.00 USD fixed                                                               | 7 days, card required |
| `api`        | "convt API", recurring month           | metered unit price on meter `api_conversion` (placeholder 1 cent), no base fee | none                  |

The catalog loader, and the daily drift check against the provider, refuse a metered `unit_amount` that is not a whole number of cents of at least 1. Checkouts pass `currency: "usd"`. The daily check also reads the organization settings and alerts unless `allow_multiple_subscriptions` and `prevent_trial_abuse` are on and Polar's trial-conversion and past-due emails are off (ours replace them); with multiple subscriptions off, API enrollment is refused with a notice, because Polar would not let a Pro customer hold a second subscription.

### Checkout

Our server creates every checkout, so every order can be matched to a checkout we recorded:

- `GET /checkout/desktop`: allowed signed out. Creates a `checkouts` row with a nonce (section 3), then the Polar checkout with `metadata.convt_checkout = <checkouts.id>`, `success_url = https://convt.app/checkout/success?checkout_id={CHECKOUT_ID}`, and, with a session, the user's id as `external_customer_id` and their email. Answers 303 to Polar.
- `GET /checkout/pro?interval=month|year`: requires sign-in. Refuses when the account has a live Pro subscription. `allow_trial` is true only when the account never had a Pro subscription.
- `POST /dashboard/api/enroll` with the spend cap: requires sign-in, refuses when an API enrollment is live. Section 4 covers the cap.
- Rate limits: 10 checkouts per IP and 5 per user per hour, through the P6 bucket function with `checkout:` keys.

The desktop app's Buy button opens `https://convt.app/pricing` and needs no change.

### Webhook events

| Event                                                                                                          | Handling                                                                                                      |
| -------------------------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------- |
| `checkout.created`, `checkout.updated`, `checkout.expired`                                                     | Snapshot: `checkouts.status`. No entitlement.                                                                 |
| `order.created`, `order.paid`, `order.updated`, `order.refunded`                                               | Snapshot of the order and its coverage items; the embedded subscription is a second snapshot. Converge.       |
| `refund.created`, `refund.updated`                                                                             | Hint: fetch the order. A refund carrying a dispute also records the dispute. Converge.                        |
| `subscription.*` (all eleven above)                                                                            | Snapshot. Converge. `paused` and `migrated` also alert: we never offer pausing, and a migration needs a look. |
| `customer.created`, `customer.updated`, `customer.deleted`                                                     | Snapshot of `billing_customers` when the customer has our external id. Deletion alerts.                       |
| `organization.updated`                                                                                         | Hint: rerun the settings check.                                                                               |
| anything else (`benefit*`, `product.*`, `discount.*`, `member.*`, `customer_seat.*`, `customer.state_changed`) | Recorded as `ignored`. The drift check covers products.                                                       |

The webhook endpoint is created on API version `2026-10`, the version the adapter's schemas are generated from. Polar's own license-key benefit is not used.

### Portal and subscription changes

"Manage billing" (card, receipt email, invoices, payment retries) opens Polar's customer portal: a customer session by `external_customer_id`, accepted only if the returned URL's origin is Polar's (the mock's locally). The billing page's own buttons call the provider and ingest the returned subscription:

- Switch interval: `changeProduct` with the catalog's policy, `invoice` both ways. Polar applies the change only after the prorated charge succeeds; a failure leaves the subscription unchanged and the page says "Your card was declined. Nothing changed." A downgrade to monthly produces a credit, not a refund.
- Cancel and resume: `cancel_at_period_end` true or false. Cancelling during the trial ends it unpaid.
- Restart Pro after it ended: a new checkout, without a trial.
- Card: `paymentMethods(userId)` fills the existing card block; "Update card" opens the portal.
- Receipt PDF: the order's invoice URL, fetched on click.

## 2. Webhook handler

### Where it runs

A second Worker, `apps/billing` (`convt-billing`), owns every write to billing state and every billing secret: the webhook route, the crons, and an RPC entrypoint the site reaches through a service binding (checkout, checkout result sync, portal URL, subscription changes, spend cap, receipt URL, account deletion). It is routed at `convt.app/webhooks/*` with no other public route. The production signing key, the webhook secret and the Polar token never sit in the Worker that renders pages and runs Better Auth; each Worker has its own Sequenzy key for transactional mail. It connects as a new role, `convt_billing`, through its own Hyperdrive config. Shared logic lives in `packages/billing` so it runs in Bun tests as well as in the Worker.

### Verification, before parsing

1. Only `POST`. A body over 256 KiB gets 413.
2. Read the body once with `arrayBuffer()`.
3. Require `webhook-id`, `webhook-timestamp` and `webhook-signature`; the timestamp must be within 5 minutes of now.
4. HMAC-SHA256 with WebCrypto over `${id}.${timestamp}.${body}`, with both keys Polar documents for the configured secret (Standard Webhooks first, then the legacy UTF-8 key), as the SDK does. Accept if any `v1,<base64>` entry matches either, compared in constant time.
5. On failure answer 401 and write nothing to the database. Log one line with the reason, no headers or body.
6. Then decode UTF-8 strictly, parse JSON, and validate with zod schemas generated from the SDK's `2026-10` types.

The verifier is our own code, tested against signatures from the `standardwebhooks` library and the legacy scheme, and against the SDK's `validateEvent`, so the mock and the verifier cannot share a bug.

### Ingest

One verified delivery, one transaction as `convt_billing`, finished within a 5-second budget so Polar's 10-second timeout is never reached:

1. `insert into webhook_events ... on conflict (provider, provider_event_id) do nothing`, storing the verified body. If the row exists as `processed`, `ignored` or `rejected`, commit and answer 200. If it exists as `failed`, process it again.
2. Turn the payload into facts; fetch hints from the provider.
3. Take `pg_advisory_xact_lock(hashtextextended('polar:<kind>:<id>', 0))` for each subject in a fixed order. Transaction-scoped locks work through Hyperdrive's transaction pooling.
4. Check the facts against our records (below). A failure marks the event `rejected` with the reason, writes no entitlement, commits, answers 200 and alerts.
5. Apply each snapshot under the version rules below.
6. Converge the affected orders, subscriptions and disputes (section 3), inserting licenses, revocations and outbox rows with `on conflict do nothing` against their unique keys.
7. Call `claim_purchases(user_id)` when a fact named a user.
8. Mark the event `processed` or `ignored`, commit, answer 200, and start an outbox drain in `waitUntil`.

On an error after verification (database error, fetch failure, budget exceeded), the transaction rolls back and a second, separate transaction upserts the event row as `failed` with `attempts + 1`, the verified body and a sanitized reason, then the handler answers 500 so Polar retries. If that write fails too (the database is down), Polar's retries and the reconciler remain. A Polar redelivery of the same `webhook-id` is verified again from scratch. The reconciler replays stored `failed` rows from their stored body without re-verifying: only verified bodies are ever stored, and the row is only writable by `convt_billing`. Every processing attempt, from a delivery or a replay, increments `attempts`; at 10 the row becomes `dead`, is no longer replayed, and is alerted.

### Versions, conflicts and terminal facts

Each stored order, invoice, subscription and dispute keeps `provider_version` and `provider_hash`. A snapshot:

- newer than the stored version: applied;
- older: ignored;
- equal with the same hash: no change;
- equal with a different hash: a conflict. Under the subject lock the handler fetches the object from the provider and applies the fetched fact if its version is at least the stored one; if the fetch fails, the delivery fails and is retried.

A fetched fact can correct any mutable field: statuses, periods, cancel flags, amounts refunded, `pending_update`. These facts are terminal and are never undone by any snapshot or fetch: an order or invoice `refunded` or `void`; a subscription ended (`ended_at` set, or `canceled`, `incomplete_expired`); a dispute `lost` or `won`; a revoked license; an issued license, which is never deleted. `refunded_cents` only grows, `billed_at` is written once, and `paid_at` is set once. A newer fact that contradicts a terminal fact (a refunded order reported paid again) is not applied: the event is `processed` with the contradiction recorded and alerted.

### Checks against our records

- Product and price ids in the catalog for this environment; currency `usd`; no discount.
- Desktop and Pro `subscription_create` and `subscription_cycle` orders: each full-period coverage item equals the catalog price.
- Pro `subscription_update` orders: every item uses a catalog Pro price, and each item's absolute amount is at most the yearly price.
- A Desktop order or a new subscription names a `checkouts` row we created for that product; if the row names a user, the customer's `external_id` is that user.
- Once `billing_customers` maps a user to a provider customer, every fact for that user's orders and subscriptions carries the same customer id. A subscription's user never changes.
- A second live Pro subscription or API enrollment for one user is stored (the money is real) and alerted; Leo refunds it by hand.

### Failure modes

| Failure                                                                           | Result                                                                  |
| --------------------------------------------------------------------------------- | ----------------------------------------------------------------------- |
| Bad or missing signature, stale timestamp                                         | 401, nothing stored                                                     |
| Signed but malformed, unknown product, wrong amount, foreign checkout or customer | `rejected`, 200, no entitlement, alert                                  |
| Database error, deadlock, fetch failure, budget exceeded                          | Rollback, `failed` row in a second transaction, 500, Polar retries      |
| Ten failed attempts                                                               | `dead`, alert                                                           |
| Signing key missing or malformed                                                  | 500 before the transaction; the health RPC and the reconciler report it |
| Polar disables the endpoint                                                       | The reconciler keeps entitlements correct and alerts                    |

### Reconciler

Polar can list nothing by modification time, so the reconciler combines short scans of new objects, re-fetches of objects that can still change, and a slow full sweep. Every scan pages in ascending `created_at` (or `started_at`) order, so new objects append at the end and a stored page number stays valid. Progress is stored per scan in `reconcile_cursors` (`name`, `page`, `pass_started_at`, `updated_at`). Fetched objects go through the same ingest code.

Every 15 minutes:

- Orders with `created_after` the last run's start minus one hour.
- Refunds and disputes pages from the stored cursor to the end. Refunds are listed by refund time, so a late refund of an old order is found whatever the order's age.
- Re-fetch individually: every subscription our database has as `incomplete`, `trialing`, `active`, `past_due`, `paused` or `unpaid`, or ended in the last 35 days; every dispute not `lost` or `won`; every `checkouts` row still `open` after 10 minutes; every `webhook_events` row in `failed`.
- The trial-ending scan for email.

Continuously, 20 pages per run: a full sweep of all orders with status `paid`, `partially_refunded` or `refunded`, and of all subscriptions, from the stored cursor, wrapping to page 1 when a pass ends. At launch volumes a pass takes under a day; the daily check alerts if a pass is older than 7 days.

Daily at 03:17 UTC: catalog and organization settings drift; invariants (each paid, unrefunded, undisputed Desktop order has exactly one unrevoked original license, and refunded or lost ones have none unrevoked; each funded coverage period beyond a subscription's existing keys has a key; no `trialing` subscription has a key; no user has two live Pro or API subscriptions; every live API subscription has a cap); `email_outbox` rows `ambiguous` or `dead`; housekeeping (section 5 and `webhook_events` bodies nulled after 30 days). Each run writes `reconcile_runs`. Discrepancies, rejected and dead events, and dead or ambiguous emails go into one daily digest to `ALERT_EMAIL`.

## 3. Licenses

### What a key grants

A license key is an offline token for the desktop app and CLI and nothing else. Its `plan` field grants no cloud access: P9 authorizes every cloud job and API call against the account's current subscription state on the server, never against a key. A paid key, Desktop or Pro, keeps perpetual access to every build dated on or before its `updates_until`. Revocation after a refund or lost dispute changes the dashboard only; a key already activated keeps working offline in the builds it covers, which `docs/plan.md` accepts. No key is issued for a trial, and keys carry no expiry.

### Desktop

For an order that is `paid` or `partially_refunded`, with no lost dispute and no original license: issue one. `issued` is the UTC date of `billed_at` (Polar's order `created_at`, which never changes); `updates_until` is the same day twelve months later, with 29 February becoming 28 February. The signed email is the account's verified email when the order has a user, else the order's email. One `license_issued` email.

### Pro: keys only from paid coverage

Polar's subscription status says nothing about what was paid, so Pro keys come only from payment-coverage evidence. Ingest stores one `payment_coverage` row per Pro coverage item: invoice, subscription, catalog product, price id, period start and end, amount, and `kind` (`period` or `proration`). A coverage row is **funded** when its invoice:

- has status `paid` (Polar's `paid` true) and was never refunded in full;
- has no lost dispute;
- and has either `net_amount > 0`, or `net_amount <= 0` with `applied_balance_amount > 0` (paid from a credit that earlier payments created).

A $0 order with no applied balance (a trial start, a 100% discount) funds nothing.

Converge for a subscription: let `E` be the latest period end among its funded coverage rows. If the UTC date of `E` is later than the `updates_until` of every unrevoked original Pro key of the subscription, issue one key with `period_start` the date of that row's period start, `updates_until` the date of `E`, and `invoice_id` that row's invoice. The subscription's current status does not matter: past due means no new funded row, and a cancelled subscription's last paid period stays paid. The first Pro key of a subscription sends `license_issued`.

Switches under the `invoice` policy:

- Monthly to yearly, payment succeeds: the proration invoice's item covers the new yearly period, so a key with the later end is issued.
- Payment fails: Polar refuses the change, no invoice exists, nothing is issued.
- Zero charge with no balance applied: funds nothing.
- Yearly to monthly: Polar credits the customer. The new monthly coverage ends before the yearly key's date, so nothing is issued until a later monthly period (paid by card or from the credit balance) ends after it.

`next_period` is supported by the interface and tested, but the catalog does not use it.

### Uniqueness

`orders` is unique on (`provider`, `provider_order_id`), and `licenses_desktop_order_key` is unique on `order_id` where `plan = 'desktop' and reissue_of is null`. With inserts using `on conflict (order_id) where plan = 'desktop' and reissue_of is null do nothing` (Postgres accepts partial-index inference), one Desktop license per order holds under any concurrency. This index stays.

P6's `licenses_pro_period_key` on (`subscription_id`, `period_start`) refuses a paid key when a period's end moves without its start moving, and collides when two periods start on the same UTC date. Migration 0001 replaces it with two unique indexes, both where `plan = 'pro' and reissue_of is null`: (`subscription_id`, `period_start`, `updates_until`), and `invoice_id`. So one invoice funds at most one key, and one paid-through date at most one key. `reissue_of` stays reserved for support reissues, which P7 does not build.

### Refunds and disputes

- An order or invoice refunded in full revokes the licenses issued from it (`revoke_reason = 'refunded'`). Partial refunds revoke nothing, for Desktop and Pro alike.
- Disputes are stored in their own `disputes` table, not as refunds. Polar sends no dispute webhook, so they arrive through the reconciler's dispute scan and re-fetches, or attached to a refund. `lost` revokes the order's or invoice's licenses (`revoke_reason = 'dispute_lost'`), funds nothing afterwards, and alerts. `won` changes nothing. `prevented` is followed by Polar's refund, which the refund path handles. Because Polar's docs do not show what a lost chargeback does to the order, the sandbox check in step 10 confirms it; until then the dispute status alone drives revocation.
- Display: a revoked card keeps its masked key, shows a neutral `REFUNDED` or `DISPUTED` badge and "Refunded on Oct 5, 2026. This key is no longer active." in place of the update window. Copy and Activate are hidden. `getLicenseKey` refuses revoked keys. The overview skips revoked keys.

### Getting keys to the browser without HTML

Loader data is serialized into server-rendered HTML, so no loader returns a token. Tokens travel only in responses to client-side server function calls with `Cache-Control: private, no-store`:

- Dashboard: P6's `getLicenseKey(id)`, owner-checked, refusing revoked keys.
- Success page `/checkout/success?checkout_id=...`: rendered with no order data. On mount it calls `getCheckoutResult(checkoutId)`, which answers `pending`, `ready` (product, masked email, `updates_until`, token), `trial`, `api_enrolled` or `failed`. It polls every 2 seconds; on the first `pending` it asks `convt-billing` to sync that checkout from the provider through ingest, once per checkout. After 60 seconds it says the key is on its way by email. The ready state shows the key with Copy and **Open in convt**, which uses P6's `openActivationLink` (`convt://activate?key=<token>`); the app asks the user to confirm. `Referrer-Policy: no-referrer`.

The checkout result is released to the session user who owns the checkout, or to the browser holding the checkout's nonce:

- The nonce is 32 random bytes from `crypto.getRandomValues`, base64url; the row stores its SHA-256. The cookie is `__Host-convt_checkout` in production (`Secure`, `Path=/`, no `Domain`, `HttpOnly`, `SameSite=Lax`, so it rides the top-level return from Polar) and `convt_checkout` in local http development. Its value is `<checkouts.id>.<nonce>`.
- The server compares in constant time, requires the cookie's checkout id to equal the row found by `checkout_id`, and checks `checkouts.nonce_expires_at` (2 hours after creation) itself rather than trusting the cookie's lifetime.
- Disclosure rotates the nonce: the first `ready` response stores a new nonce hash with a 10-minute expiry, sets `key_disclosed_at`, and sets the new cookie in the same response. Reloads within 10 minutes show the key again; after that, or with the old nonce, the page says the key was shown and is in the email and on the dashboard after signing in with that address.
- If the first `ready` response is lost, the browser keeps the old, now invalid nonce and gets the same "already shown" page. The key is never lost: the email and the claim path still deliver it.

## 4. API enrollment and the spend cap

The enrollment form validates the cap (whole cents, $1 to $10,000). `convt-billing` stores it on the new `checkouts` row before calling Polar. When the API subscription is first ingested, the same transaction copies `checkouts.spend_cap_cents` to the subscription through the checkout id the subscription carries; an API subscription whose checkout is not ours is rejected, so every stored API subscription has a cap, which a check constraint enforces. A delayed or dropped webhook, or a worker restart after the checkout row was written, ends the same way when the success page's sync or the reconciler ingests the subscription. A checkout row that never reached Polar expires unused.

Enrollment is usable when the API subscription is `active`, has a cap, and `paymentMethods(userId)` returned at least one card when the subscription was ingested as active. So enrollment never depends on the metered checkout's initial order, which the step 10 sandbox check still confirms before launch.

| State          | Source                                                 | Keys (P9)                                      |
| -------------- | ------------------------------------------------------ | ---------------------------------------------- |
| Not enrolled   | no live API subscription                               | Create disabled; the enrollment form           |
| Pending        | open checkout, `incomplete`, or no card seen yet       | Create disabled                                |
| Enrolled       | `active`, cap, card                                    | Usable                                         |
| Payment failed | `past_due` or `unpaid`                                 | Exist; new jobs refused until payment succeeds |
| Ended          | `canceled`, `incomplete_expired`, or `ended_at` passed | New jobs refused                               |

Pro and the API are separate Polar subscriptions held at once, which needs `allow_multiple_subscriptions` on (section 1). Our own checks keep each to one per user.

The contract with P9's reservations:

- P9 creates a job in one transaction that locks the API subscription row `FOR UPDATE`, requires enrollment to be usable, and checks settled usage since `current_period_start` plus open reservations plus the new charge against `spend_cap_cents`.
- P7's "set spend cap" takes the same row lock, so cap changes and reservations serialize. Lowering the cap keeps every existing reservation and settles them normally; it blocks any new reservation that would bring spent plus reserved above the new cap. Spent plus reserved can therefore exceed a lowered cap only through commitments made before the change.
- Prices are whole cents until P9 defines sub-cent precision and rounding for `usage_events.amount_cents`, `cloud_jobs.reserved_cents` and the cap.
- Ending enrollment sets `cancel_at_period_end`; usage until then is still reported and billed.

## 5. Email

### Templates

Plain TypeScript functions in a new `packages/mail` (the site keeps its copy of `mail.ts` until the sign-in fix in `apps/web/src/server` has landed, then imports the package). Each takes frozen inputs and returns subject, text and minimal escaped HTML. Polar sends receipts as merchant of record.

| Kind             | Sent when                                                 | Dedupe key                                                | Content                                                                                |
| ---------------- | --------------------------------------------------------- | --------------------------------------------------------- | -------------------------------------------------------------------------------------- |
| `license_issued` | Desktop key, or a subscription's first Pro key            | `license_issued:<license id>`                             | The key, an "Open in convt" `convt://activate` link, download link, updates-until date |
| `trial_ending`   | A Pro trial ends within 48 hours and is not set to cancel | `trial_ending:<subscription id>:<trial_ends_at>`          | End date, amount and interval to be charged, link to cancel                            |
| `renewal_failed` | A Pro or API subscription becomes `past_due`              | `renewal_failed:<subscription id>:<current_period_start>` | What failed, what stops working, portal link                                           |
| `alert_digest`   | Daily, when anything needs Leo                            | `alert_digest:<date>`                                     | Counts and ids, no keys or bodies, to `ALERT_EMAIL`                                    |

### Outbox

Production and staging now use Sequenzy (`MAIL_TRANSPORT=sequenzy`, secret `SEQUENZY_API_KEY`, sender `convt <hello@convt.app>`). Resend remains an optional transport. Local sign-in mail goes to Mailpit, and local billing uses the Resend mock forwarding to Mailpit. Sequenzy sends HTML through `POST /api/v1/transactional/send` with click and open tracking disabled. Its keys replay for 14 days; billing retains the conservative 23-hour ambiguity cutoff shared with Resend. HTTP 429 honors `Retry-After`; network failures and 5xx remain unknown outcomes.

Rows are inserted in the ingest transaction with `on conflict (dedupe_key) do nothing`, holding the kind, recipient, user and subject id. The drain runs after each commit and every minute:

1. **Claim.** Up to 20 due rows with `FOR UPDATE SKIP LOCKED`: `status = 'sending'`, `locked_until = now() + 2 minutes`, `claim_generation + 1`. Commit.
2. **Freeze.** On a row's first claim, before any send, check relevance (a revoked license, a trial no longer trialing, a subscription active again make it `skipped`), then render once and store the exact request: `payload` (from, to, subject, text, html, tags with the outbox id), `template_version`, `payload_sha256`. Commit. Later attempts send these stored bytes unchanged, so a deploy that changes a template cannot make Resend refuse a retry.
3. **Send.** Set `first_attempt_at` (once) and `last_attempt_at`, commit, then call the selected provider with `Idempotency-Key: <outbox id>`.
4. **Complete,** fenced: `update ... where id = $1 and claim_generation = $2`, so a worker whose lease expired cannot mark a row another worker owns.
   - Success: `sent` with the message id.
   - 409 `concurrent_idempotent_requests`, 429, 5xx, a timeout or a network error: `pending` with backoff of 1, 5, 30 minutes, then 2, 6 and 12 hours. Retries reuse the frozen payload and key.
   - 409 `invalid_idempotent_request` (impossible with a frozen payload) or a 4xx address error: `dead`, alerted.
   - If the next attempt would fall later than 23 hours after `first_attempt_at` and an earlier attempt had an unknown outcome (timeout, network error, 5xx), Resend's key may have expired, so a retry could send a second copy. The row becomes `ambiguous` instead, is not retried, and goes into the digest with its outbox id. For Sequenzy, investigate the provider's records using the row's recipient and attempt timestamps; no Resend tags are sent. With the optional Resend transport, the outbox id is also an email tag. Leave the row ambiguous if acceptance cannot be confirmed. `bun run billing:outbox resolve <id> sent|resend` records Leo's decision; `resend` sends under a new key.
5. **Retention.** `last_error` keeps only the HTTP status, the provider's error code and at most 200 characters passed through the P6 redactor (no addresses, tokens or keys). `payload` is nulled 25 hours after a row reaches `sent`, `skipped` or `dead`; the metadata row is deleted after 400 days. `convt_billing` may delete only `email_outbox` and `webhook_events` rows, and only through these retention jobs.

The local mock implements Resend's `POST /emails` with the documented idempotency semantics, a 24-hour key store on the mock's clock, both 409 codes, and fault modes: accept but time out, delay acceptance, return 5xx. Accepted messages are forwarded to Mailpit. The production `resend` transport points at it locally, so the same code path is tested.

## 6. Account deletion

P6 left deletion as a placeholder because it must end subscriptions first.

1. The settings page's Delete button needs a fresh session (Better Auth's `freshAge`, 1 hour, checked by the server function, as P6's freshness hook does) and the user's email typed to confirm. It calls `convt-billing`'s `requestDeletion(userId)`, which inserts an `account_deletions` row (unique per user while not `done`) and revokes every other session and device. Repeating the request returns the same row.
2. A cron step drives each open row: for every live Pro and API subscription, `revokeSubscription` (an immediate end, no refund), then ingest the returned fact. "Already ended" from the provider counts as success after a fetch. API subscriptions wait until P9's sender has reported all of the user's usage (none exists in P7). Provider errors back off and retry; after 24 hours the row is alerted, and the user's page says deletion is still in progress.
3. When the stored facts show no live subscription, `convt-billing` calls `delete_user(user_id, deletion_id)`, a `SECURITY DEFINER` function owned by `convt_owner` and executable only by `convt_billing`. It checks that the deletion row is in `deleting` and that no live subscription remains, then deletes the user. A crash before this step leaves the row open, and the next cron run resumes it.
4. Foreign keys: P6's cascades and `SET NULL` stand. Among the new tables, `checkouts`, `billing_customers`, `email_outbox` and `account_deletions` set `user_id` to null; financial rows keep their email. Pending outbox rows for a deleted user other than `license_issued` become `skipped`.

## 7. Schema (migration 0001)

One migration with its down file, grants in `sql/privileges.sql`, `roles.sql` adding `convt_billing`, and id prefixes `chk`, `cus`, `whe`, `eml`, `rcn`, `cov`, `dsp`, `del` in `idPrefixes` (convt-server needs none).

### Status mapping

| Provider status (Polar) | `orders` (Desktop)   | `invoices` (subscription orders) |
| ----------------------- | -------------------- | -------------------------------- |
| `draft`                 | not stored           | `draft`                          |
| `pending`               | `pending`            | `open`                           |
| `paid`                  | `paid`               | `paid`                           |
| `partially_refunded`    | `partially_refunded` | `partially_refunded`             |
| `refunded`              | `refunded`           | `refunded`                       |
| `void`                  | `void`               | `void`                           |

Subscriptions store Polar's eight statuses as they are, which adds `incomplete_expired` and `paused` to P6's list. `deriveAccountState` treats `incomplete_expired` as ended and `paused` as not live (shown as lapsed, alerted). Times: `billed_at` is the provider's `created_at`, written once; `paid_at` is when we first stored the order as paid, set once with `coalesce` and never moved by reconciliation. Only `billed_at` feeds a license date.

### Table changes

| Table                                       | Change                                                                                                                                                                                                                                                                                                                                                       |
| ------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| `orders`                                    | statuses as mapped; `paid_at` nullable with a check that `paid`, `partially_refunded` and `refunded` rows have it; add `billed_at`, `refunded_cents` (default 0), `checkout_id`, `provider_version`, `provider_hash`                                                                                                                                         |
| `invoices`                                  | add `partially_refunded` status, `reason`, `billed_at`, `paid_at` (same check), `net_cents`, `applied_balance_cents`, `refunded_cents`, `provider_version`, `provider_hash`                                                                                                                                                                                  |
| `subscriptions`                             | the two statuses; `checkout_id`, `provider_version`, `provider_hash`, `pending_update` (jsonb), `card_seen_at`; `spend_cap_cents` positive, null for Pro, required for API                                                                                                                                                                                   |
| `licenses`                                  | add `invoice_id`; replace the Pro index (section 3); `revoke_reason` checked against `refunded`, `dispute_lost`                                                                                                                                                                                                                                              |
| `payment_coverage` (new)                    | `id`, `invoice_id`, `provider_item_id`, `subscription_id`, `product`, `price_id`, `period_start`, `period_end`, `amount_cents`, `kind`; unique `provider_item_id`                                                                                                                                                                                            |
| `disputes` (new)                            | `id`, `provider`, `provider_dispute_id` (unique with provider), `order_id` or `invoice_id`, `status`, `amount_cents`, `closed`, `provider_version`, `provider_hash`                                                                                                                                                                                          |
| `checkouts` (new)                           | `id`, `provider`, `provider_checkout_id` (unique with provider), `user_id`, `product`, `allow_trial`, `spend_cap_cents`, `nonce_hash`, `nonce_expires_at`, `key_disclosed_at`, `status`, `synced_at`, timestamps                                                                                                                                             |
| `billing_customers` (new)                   | `id`, `user_id`, `provider`, `provider_customer_id`, `email`, `deleted_at`; unique (`provider`, `provider_customer_id`) and (`provider`, `user_id`)                                                                                                                                                                                                          |
| `webhook_events` (new)                      | `id`, `provider`, `provider_event_id` (unique with provider), `type`, `received_at`, `body` (nullable), `status` (`processed`, `ignored`, `rejected`, `failed`, `dead`), `reason`, `attempts`, `processed_at`                                                                                                                                                |
| `email_outbox` (new)                        | `id`, `kind`, `dedupe_key` (unique), `to_email`, `user_id`, `subject_id`, `status` (`pending`, `sending`, `sent`, `skipped`, `dead`, `ambiguous`), `claim_generation`, `locked_until`, `next_attempt_at`, `attempts`, `first_attempt_at`, `last_attempt_at`, `template_version`, `payload`, `payload_sha256`, `provider_message_id`, `last_error`, `sent_at` |
| `account_deletions` (new)                   | `id`, `user_id`, `status` (`pending`, `canceling`, `deleting`, `done`, `failed`), `attempts`, `last_error`, timestamps; unique `user_id` where `status <> 'done'`                                                                                                                                                                                            |
| `reconcile_runs`, `reconcile_cursors` (new) | as in section 2                                                                                                                                                                                                                                                                                                                                              |

### Grants

- `convt_web` loses `UPDATE` on `orders`, `subscriptions`, `licenses` and `invoices`. Purchase claiming moves into `claim_purchases(user_id)`, a `SECURITY DEFINER` function owned by `convt_owner` with a fixed `search_path`, executable by `convt_web` and `convt_billing`. It does exactly what P6's `claimPurchases` does (lock the user, require a verified email, attach unclaimed rows with that email), and the TypeScript function becomes a call to it. Calling it for any user id can only attach that user's own email's purchases to that user. `convt_web` gets `SELECT` on `checkouts`, `billing_customers`, `disputes` and `account_deletions`, and no write on any billing table.
- `convt_billing` gets `SELECT, INSERT, UPDATE` on the billing tables and new tables, `DELETE` only on `email_outbox` and `webhook_events`, `SELECT` on `users`, `usage_events` and `cloud_jobs`, and `EXECUTE` on `claim_purchases` and `delete_user`.

Seed changes: `trial@` loses its trial key; new fixtures `refunded@convt.test` (refunded Desktop order), `pastdue@convt.test` (past-due Pro), `disputed@convt.test` (lost dispute) and a pending API enrollment; every subscription fixture gets coverage rows, a `billing_customers` row with mock ids, and API fixtures a cap.

## 8. Local development and tests

### Local stack

- `tools/billing-mock`, a Bun server like `tools/oauth-mock`, refusing to bind anything but loopback. It serves the subset of Polar's API the adapter calls (checkouts, orders with items, subscriptions with `changeProduct` and the proration rules above including payment failure, customers and payment methods, customer sessions, refunds, disputes, organization settings, page-number list pagination), a hosted checkout page (Pay, Decline, Abandon), a portal page, Resend's `/emails` as described in section 5, and admin endpoints that move the clock, end a trial, renew, fail a renewal, refund fully or partly, open and close disputes, and toggle `allow_multiple_subscriptions` (when off, a second subscription checkout for a customer fails, as Polar's default does). Webhooks are signed with Standard Webhooks or the legacy scheme and can be duplicated, delayed, reordered, dropped, or forged. Its payloads are validated against the SDK's `2026-10` schemas in its own tests.
- `convt-billing` runs beside the site as an auxiliary Worker of the Cloudflare Vite plugin with the service binding, or as a second `wrangler dev` with the dev registry if step 1 finds the plugin cannot. Crons run through `/__scheduled` locally and are called directly in tests.
- `scripts/dev-web.sh` also starts the mock, writes `POLAR_WEBHOOK_SECRET` (a generated `whsec_` value) to `.convt-dev/services.env`, points the adapter and the Resend transport at the mock, and uses `.convt-dev/license.key` as `LICENSE_SIGNING_KEY`. `HYPERDRIVE_BILLING` gets the local URL as `convt_billing`.
- The billing worker refuses to start in production with a loopback provider or mail URL, or a signing key whose public key is the dev key's.

### Tests

Integration tests in Bun against the P6 disposable Postgres, as the real roles, with the mock in process and an injected clock:

- **Forged signatures:** wrong secret; one body byte changed; a valid signature for another `webhook-id`; timestamps 6 minutes old and ahead; missing headers; garbage signature header; one bad and one good signature (accepted); both key schemes accepted; oversized body. Every forgery leaves all tables unchanged.
- **Business checks:** unknown product or price, wrong amount, a discount, EUR, a checkout we did not create, another user's checkout, a changed customer id. Each `rejected`, no license.
- **Trials:** a trialing subscription and its $0 paid order issue nothing; conversion with a paid order issues one key; a trial cancelled before its end issues nothing; a returning customer's checkout has `allow_trial` false.
- **Pro coverage:** one key per paid period; switch to yearly with payment success (key with the later end), payment failure (nothing changes, nothing issued), zero charge (nothing issued), credit-funded downgrade (nothing until monthly coverage passes the yearly date, then one key, including a period paid wholly from the credit); `next_period` through the interface.
- **Duplicates:** one delivery 5 times; one order through `order.created`, `order.paid` and `order.updated` with distinct event ids: one order, one license, one outbox row.
- **Ordering and versions:** every permutation of a Desktop purchase and refund and of a renewal's order, cycle and failure ends in the same state; a null `modified_at` falls back to `created_at`; equal versions with different content fetch and apply the provider's state; a fetched contradiction of a terminal fact (refunded reported paid) is not applied and is alerted.
- **Failed events:** a fault after verification leaves a `failed` row with the body and `attempts = 1`; a redelivery processes it; a reconciler replay processes it; ten failures make it `dead`.
- **Crash between issuing and emailing:** a fault after commit and before the drain leaves a pending row that the next drain sends once; a fault inside the transaction after the license insert rolls back and the redelivery issues once; a fault after Resend accepted and before marking leads to one message after the retry.
- **Refunds and disputes:** full Desktop refund revokes; partial Desktop and partial Pro refunds do not; a refund before the paid event never issues; a refunded Pro invoice revokes only its key; a dispute lost revokes, a dispute won does not, a prevented dispute's refund revokes; `getLicenseKey` refuses revoked keys.
- **Reconciler:** with webhooks dropped, one run issues the missing license and email; a run racing the webhook issues once; a refund of a 200-day-old order is found by the refund scan; a lost dispute is found by the dispute scan; the full sweep resumes from its stored page after a restart and wraps; catalog drift, a fractional metered price, and settings drift are reported.
- **Checkout cookie:** the key is released only with the matching cookie or the owning session; an expired nonce, a nonce for another checkout, an injected cookie with a forged checkout id, and the old nonce after rotation are all refused; a reload within 10 minutes shows the key; the lost-first-response path shows the "already shown" page.
- **Spend cap:** the cap set at enrollment reaches the subscription with the webhook delayed, dropped, or the worker restarted between the checkout row and the Polar call; an API subscription without our checkout is rejected; enrollment is pending until a card is seen.
- **Pro and API together:** with multiple subscriptions on, one user holds both and gets no duplicate of either; with it off, API enrollment is refused with the notice and the daily check alerts.
- **Concurrency:** 20 concurrent deliveries of one paid order; `order.paid` racing `order.refunded` 50 times with random delays, always ending refunded with no unrevoked key; concurrent subscription snapshots ending at the newest; two drains over 50 rows sending each once; a stale worker's completion refused by the claim generation; a cap lowered while a simulated P9 reservation runs: existing reservations kept, the next over-cap reservation refused.
- **Email:** template snapshots and escaping; `trial_ending` once per trial; `renewal_failed` once per period; the frozen payload survives a template version change; backoff, `dead`, `skipped`; the 23-hour rule makes an ambiguous row `ambiguous` (mock clock past 24 hours shows why); `last_error` contains no address or token; payload redaction after 25 hours.
- **Account deletion:** with live Pro and API subscriptions both are revoked, then the user is deleted and financial rows keep the email; a provider failure retries and alerts; a partial cancellation (one of two) resumes; a crash before `delete_user` resumes; a stale session is refused; `delete_user` refuses while a subscription is live and is not executable by `convt_web`.
- **Grants:** as `convt_web`, changing an order's status, a subscription's period, an invoice's amounts, a license's token or revocation, or any coverage row fails, and `claim_purchases` still claims; as `convt_billing`, deleting a financial row fails.
- **Migration and fixtures:** every mapped status inserts; a pending order without `paid_at` inserts and a paid one without it fails; `billed_at` and `paid_at` are not moved by a later snapshot; the old Pro index's failing cases now pass.
- **Keys:** issued tokens verify with the Rust verifier (`cargo run -p convt-cli -- license activate`, `CONVT_LICENSE_STORE=file`, `HOME` and XDG directories in `mktemp` directories), with `updates_until` per the rules, including 29 February.

End to end with `agent-browser` against `dev-web.sh` (`apps/web/e2e/billing.sh`), at 1280 and 390 px in light and dark: guest Desktop purchase to the success page, key shown, Open in convt checked with the P6 anchor stub, no token in the server HTML, the success URL in a second session showing nothing; the purchase email; sign-up with that email claims it; Pro trial, trial end, key on the dashboard and by email; switch interval including a declined card; cancel and resume; portal redirect; a failed renewal; a refunded and a disputed card; API enrollment through pending to enrolled, a failed payment, and the cap edit; account deletion with live subscriptions. The `test-convt-web` skill gains the mock, the billing worker, the fixtures and `billing.sh`. Opening a real `convt://` link in the app stays a GUI check that needs Leo's permission.

## Step 1 results (2026-10-05)

- **SDK and schemas.** `@polar-sh/sdk` 1.0.2 ships TypeScript types for `2026-10` but no runtime schemas. The zod schemas in `packages/billing/src/polar.ts` are written by hand for the fields we read, and `tsc` checks each against the SDK's model type (`models.Order`, `models.Subscription`, `models.Checkout`, `models.Refund`, `models.Dispute`, `models.CustomerIndividual`, `models.Organization`): a field the SDK lacks, or a narrower type, fails the build. The mock builds every payload as the SDK's model types, so its shapes are checked the same way. The adapter calls Polar through the SDK client with `baseUrl` pointed at the mock locally.
- **Verifier.** `verify.ts` accepts and refuses exactly what `standardwebhooks` 1.1.1, Polar's legacy UTF-8 key and the SDK's `validateEvent` do, for both key schemes and eight forgeries (`test/unit/verify.test.ts`). The mock signs with wall time, as Polar does; its business clock moves separately.
- **Auxiliary Worker.** With `auxiliaryWorkers` in the Cloudflare Vite plugin, `convt-billing` runs beside the site, reads `apps/billing/.dev.vars` and `CLOUDFLARE_HYPERDRIVE_LOCAL_CONNECTION_STRING_HYPERDRIVE_BILLING`, and the site reaches its `BillingRpc` entrypoint through the `BILLING` service binding. An auxiliary Worker has no port, so outside production the site forwards `/webhooks/*` and `/__billing/*` to the binding (in production the zone route sends `/webhooks/*` to `convt-billing` directly). Crons run through `/__billing/scheduled?cron=...` or the plugin's local explorer API. The entrypoint needs its own `fetch` for that forwarding.
- **Second mode.** `convt-billing` under its own `wrangler dev --test-scheduled` verifies (a forgery gets 401), processes the mock's deliveries through Hyperdrive as `convt_billing`, and runs `/__scheduled`. The site under `wrangler dev` reported `env.BILLING (convt-billing#BillingRpc) [connected]` through the dev registry but never finished starting on this machine: the registry's file watcher failed with EMFILE (the shared per-user watch limit), so the cross-process RPC call in this mode is unverified here.

## 9. Work breakdown

The checkout is shared and not under git. P7 keeps P6's discipline: a manifest before step 2, a write log per step, and an allowlist (`apps/billing`, `apps/web`, `packages/billing`, `packages/mail`, `packages/db`, `packages/license/src/ids.ts`, `tools/billing-mock`, `scripts/dev-web.sh`, `scripts/db.sh`, `.agents/skills/test-convt-web`, `docs/p7-billing-plan.md`, `docs/plan.md`, `CLAUDE.md`, and the root `package.json` and `bun.lock` edited once each). Steps 7 and 8 start after the sign-in fix in `apps/web/src/server/auth.ts` lands, re-read each file before editing, and leave `auth.ts` alone.

| Step | Work                                                                                                                                                                                                                                                                 | Checks                                                                                           | Depends on  |
| ---- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------ | ----------- |
| 1    | Spike: schemas generated from the SDK's `2026-10` types; the verifier against both key schemes and `validateEvent`; the auxiliary Worker with a service binding, a second Hyperdrive binding and cron under the Vite plugin and `wrangler dev`. Record results here. | SDK-signed payloads verify; the site calls the billing Worker locally in both modes.             | none        |
| 2    | Migration 0001, down file, grants, `claim_purchases` and `delete_user`, `convt_billing`, id prefixes, seed changes, migration and grant tests.                                                                                                                       | `bun run db:ci`; the migration, fixture and grant tests in section 8.                            | 1           |
| 3    | `tools/billing-mock`.                                                                                                                                                                                                                                                | Its own tests: SDK schema conformance, signatures, Resend idempotency semantics.                 | 1           |
| 4    | `packages/mail`: transports and templates.                                                                                                                                                                                                                           | Template snapshots.                                                                              | 1           |
| 5    | `packages/billing`: catalog, adapter, verifier, ingest, coverage and converge, issuance, outbox, reconciler, deletion workflow; the integration tests in section 8.                                                                                                  | `bun test` against the disposable Postgres.                                                      | 2, 3, 4     |
| 6    | `apps/billing` Worker: webhook route, RPC, crons, production guards; `dev-web.sh` changes.                                                                                                                                                                           | Mock-driven round trip through the running Worker in both dev modes; a forged delivery gets 401. | 5           |
| 7    | `apps/web`: checkout routes, pricing links, success page, billing actions and portal, API enrollment and cap, revoked and disputed cards, account deletion, `claimPurchases` through the function.                                                                   | `bun run check && bun run check-types && bun run build`; view unit tests.                        | 6, auth fix |
| 8    | `e2e/billing.sh`, skill, CLAUDE.md, `docs/plan.md` P7 status.                                                                                                                                                                                                        | A fresh run of the skill passes `billing.sh` and the P6 scripts.                                 | 7           |
| 9    | Independent GPT-6.1 Sol review of the diff, fixes, full verification including the write-log check.                                                                                                                                                                  | All of the above.                                                                                | 8           |
| 10   | Sandbox check, only with Leo's go-ahead and his sandbox organization: metered-only checkout (blocking for launch), the trial order sequence, a lost test dispute, email editing at checkout. Record results here and adjust the mock.                                | Each unconfirmed fact in section 1 confirmed or the plan changed.                                | 6, Leo      |

Parallel lanes: after step 1, steps 2, 3 and 4 run together. Step 5 can start its verifier, catalog and coverage logic before 2 and 3 finish. Steps 6 and 7 run in sequence; step 8's skill text can start during 7. Step 10 can run any time after 6.

### Screens with no design

No Paper artboard is known for these (the file was unreachable). They are built in the existing card, badge, button and table styles, light and dark, at both widths:

1. Checkout success: pending, ready with key, trial started, API enrolled, failed, "on its way by email", and "already shown".
2. Billing page: plan actions, declined switch, cancel confirmation, "cancels on <date>" with Resume, past due.
3. API enrollment: not enrolled with the cap form, pending, enrolled, payment failed, ended, "needs multiple subscriptions" notice; cap edit.
4. Refunded and disputed license cards.
5. Account deletion: confirmation and "in progress".
6. The four email templates.

## Accounts and secrets Leo creates later

None are needed to build and verify P7 through step 9.

Done on 2026-10-06: both "convt" Polar organizations (production `6098e410-4ea7-48fd-b677-7b261b8e0f7c`, sandbox `c9b2ccbd-28a8-4f07-984b-66b59c06a410`) have the four products and the `api_conversion` meter, and their ids are in `catalog.ts`. The products carry no license-key benefit. Production is still in Polar's account review.

| What                                                                                                                                                      | Where it goes                                                                                                                                                     |
| --------------------------------------------------------------------------------------------------------------------------------------------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Polar organizations for production and sandbox                                                                                                            | Payout and tax details in Polar.                                                                                                                                  |
| Organization settings: **Allow multiple subscriptions** on, prevent trial abuse on, the trial-conversion and past-due emails off                          | Polar dashboard (the daily check verifies them).                                                                                                                  |
| Products and the `api_conversion` meter as in the catalog table, in both                                                                                  | Product and price ids into `catalog.ts` per environment.                                                                                                          |
| An organization access token with only the scopes the adapter uses                                                                                        | `POLAR_ACCESS_TOKEN` secret on `convt-billing` only.                                                                                                              |
| A webhook endpoint `https://convt.app/webhooks/polar`, API version `2026-10`, subscribed to the events in section 1                                       | Its secret as `POLAR_WEBHOOK_SECRET` on `convt-billing`.                                                                                                          |
| The production license signing key, generated offline with a new `bun run license:keygen` that writes the seed outside the repo and prints the public key | `LICENSE_SIGNING_KEY` secret on `convt-billing` only; the public key becomes `CONVT_LICENSE_PUBKEY` for release builds (P11); the seed in Leo's password manager. |
| The `convt_billing` role password and a Hyperdrive config as `convt_billing`, caching disabled                                                            | Its id as `HYPERDRIVE_BILLING` in `apps/billing/wrangler.jsonc`.                                                                                                  |
| `SEQUENZY_API_KEY` (from P6) also on `convt-billing`; `ALERT_EMAIL`                                                                                       | Worker secret and var.                                                                                                                                            |
| A real API price, in whole cents until P9 defines more                                                                                                    | `catalog.ts` and the Polar meter price.                                                                                                                           |
