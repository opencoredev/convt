# P6 plan: accounts and dashboard shell

This plan turns the dashboard and account pages in `apps/web` from placeholder data into real data from Postgres, adds sign-in, and gives convt-server typed access to the same tables. P7 to P9 build on the schema without rewriting it. Nothing in this phase uses a real external service: Postgres, email and OAuth all run locally. The desktop app, the CLI and every crate except convt-server are untouched.

Status of the inputs: the dashboard (`/dashboard`, `/dashboard/licenses`, `/dashboard/billing`, `/dashboard/api`, `/account`) and sign-in (`/sign-in`, `/sign-in/check-email`) pages exist and read from `src/lib/account.ts`, which returns `src/lib/placeholder.ts`. convt-server has `/health` and `/v1/formats` and no database. The Paper file could not be opened while this plan was written (connection error), so whether the overview states have artboards is unchecked.

## Decisions

The defaults from `docs/plan.md` stand: Better Auth inside the TanStack Start app, email plus GitHub and Google, sessions in Postgres, Postgres on Railway reached through Hyperdrive, Drizzle owning the schema in `packages/db`, sqlx in convt-server, and a Postgres container per checkout. Leo accepted these further defaults on 2026-10-04:

1. Apple sign-in stays a placeholder button with the "not available yet" notice until the Apple Developer account exists (P4).
2. Email sign-in uses Better Auth's email OTP plugin, not the magic-link plugin, so the link and the code on the built check-email page are one single-use secret.
3. A plain "Sign out" link goes in the header after Help, styled like Docs and Help, until the designer places it.
4. Overview states follow Paper artboards where they exist; otherwise they reuse the existing cards with short empty lines and links. A `new` state (signed in, bought nothing) is added.
5. Down migrations exist for local development only. Production is forward-only and recovers from backups.
6. Account deletion stays a placeholder until P7, because it must cancel a subscription first.

Versions are pinned exactly: `better-auth` 1.7.7, `pg` 8.23.1, `drizzle-orm` 0.45.3, `drizzle-kit` 0.31.11 (current on npm on 2026-10-04). Statements below marked "confirmed" were read in those packages' published source; the step 1 spike confirms the rest in a running Worker.

## 1. Schema

### Ownership

`packages/db` owns the schema: Drizzle table definitions in `src/schema/*.ts`, generated SQL in `migrations/`, hand-written reverse SQL in `migrations/down/` with one file per migration, role grants and triggers in the migrations themselves, and the query functions the web app calls in `src/queries/`. Migrations run from Bun (`bun run db:migrate`), never from the Worker or convt-server. Better Auth's tables are generated once with `@better-auth/cli generate` and then owned here. A unit test compares them with the fields Better Auth's adapter expects, so an upgrade that adds a column fails CI instead of failing at sign-in.

Drizzle's migrator records each applied migration in `drizzle.__drizzle_migrations` as the SHA-256 of the SQL file and the journal's `when` timestamp, and applies any migration newer than the last recorded timestamp. It never compares hashes (confirmed in `drizzle-orm/pg-core/dialect.js` and `migrator.js`). So:

- `db:rollback` runs the down file and deletes that migration's row from `drizzle.__drizzle_migrations` in the same transaction. The journal file in the repo changes only when a migration is removed from the repo, which is allowed only for migrations never applied outside local databases.
- convt-server embeds the list of (`when`, SHA-256) pairs from `migrations/meta/_journal.json` and the SQL files at build time. At startup it reads the table and refuses to serve unless every embedded migration is present with the same hash, in order. Rows newer than the embedded list are allowed with a warning, because migrations are deployed before the code that needs them and must be additive.

convt-server uses sqlx with compile-time checked queries (`query!`, `query_as!`) and offline data committed in `crates/convt-server/.sqlx`. With no `DATABASE_URL`, sqlx macros read that data, so `cargo build` and `cargo test` keep working with no database. sqlx never runs migrations.

### Database roles

The migration creates no roles; `packages/db/roles.sql` creates them once per cluster (run by `db.sh up`, and by Leo on Railway):

- `convt_owner` owns every table and runs migrations.
- `convt_web` (the Worker, through Hyperdrive) and `convt_server` (convt-server and the worker) get `SELECT, INSERT, UPDATE` on what they use, no `DELETE` on financial tables, and only the column grants described under usage below.

Local development and every test connect as `convt_web` or `convt_server`, never as the owner, so a missing grant fails locally.

### Conventions

- Table and column names are snake_case and plural. Drizzle uses `casing: "snake_case"`; Better Auth uses `usePlural: true`.
- Primary keys are text with a type prefix and 128 random bits in Crockford base32: `usr_`, `ses_`, `acc_`, `dev_`, `ord_`, `sub_`, `inv_`, `lic_`, `key_`, `job_`, `use_`. Better Auth produces them through `advanced.database.generateId`. TypeScript and Rust share one generator spec, checked by a test vector.
- Timestamps are `timestamptz`. License dates are `date`, matching the `YYYY-MM-DD` strings in the signed payload. Money is integer cents plus `currency`.
- Status fields are `text` with a `CHECK` constraint, not Postgres enums, which cannot be reversed cleanly in a down migration.
- Emails are stored lowercased and trimmed; uniqueness is on that value.
- `provider` columns hold the payment provider name (`polar` by default), so a switch to Stripe in P7 is a data change.

### Tables

Auth (Better Auth shapes, renamed):

| Table             | Columns beyond `id`, `created_at`, `updated_at`                                                                       | Constraints and indexes                                                     |
| ----------------- | --------------------------------------------------------------------------------------------------------------------- | --------------------------------------------------------------------------- |
| `users`           | `email`, `email_verified`, `name`, `image`                                                                            | unique `email`                                                              |
| `sessions`        | `user_id`, `token`, `expires_at`, `ip_address`, `user_agent`                                                          | unique `token`; index `user_id`; FK cascade                                 |
| `accounts`        | `user_id`, `provider_id` (`github`, `google`), `account_id` (the provider's user id), encrypted token fields, `scope` | unique (`provider_id`, `account_id`); index `user_id`; FK cascade           |
| `verifications`   | `identifier`, `value` (hashed OTP and attempt count), `expires_at`                                                    | unique `identifier`; index `expires_at`                                     |
| `rate_limits`     | `key`, `count`, `last_request`                                                                                        | unique `key`                                                                |
| `otp_send_limits` | `key` (`email:<sha256>` or `ip:<ip>`), `window_start`, `count`, `expires_at`                                          | primary key `key`; index `expires_at`                                       |
| `devices`         | `user_id`, `name`, `os`, `app_version`, `token_hash` (null until P8), `last_seen_at`, `revoked_at`                    | unique `token_hash`; index `user_id` where `revoked_at is null`; FK cascade |

`verifications.identifier` is unique on purpose. Better Auth's OTP resend inserts a new row with the same identifier and deletes the old row only when that insert fails (confirmed in `resolveOTP`, `plugins/email-otp/routes.mjs`). Without the unique index, a resend would leave the old code valid. `otp_send_limits` is explained under rate limits.

The email method has no `accounts` row: the user's verified email is always a sign-in method, which is why the settings page marks it not removable. `devices` backs the "Macs" list and the app sessions on the settings page; P8 fills `token_hash`, and in P6 the rows come only from seeds.

Purchases and entitlements:

| Table           | Columns                                                                                                                                                                                                                                                                                                                                                                                                             | Constraints and indexes                                                                                                                                                                                                                                                           |
| --------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `orders`        | `provider`, `provider_order_id`, `provider_customer_id`, `user_id` (nullable), `email`, `product` (`desktop`), `amount_cents`, `currency`, `status` (`paid`, `refunded`, `partially_refunded`), `paid_at`, `refunded_at`                                                                                                                                                                                            | unique (`provider`, `provider_order_id`); index `user_id`, index `email` where `user_id is null`                                                                                                                                                                                  |
| `subscriptions` | `provider`, `provider_subscription_id`, `provider_customer_id`, `user_id` (nullable), `email`, `kind` (`pro`, `api`), `interval` (`month`, `year`, null for API), `status` (`trialing`, `active`, `past_due`, `canceled`, `unpaid`, `incomplete`), `trial_ends_at`, `current_period_start`, `current_period_end`, `cancel_at_period_end`, `canceled_at`, `ended_at`, `spend_cap_cents` (API), `provider_updated_at` | unique (`provider`, `provider_subscription_id`); index `user_id`, index `email` where `user_id is null`                                                                                                                                                                           |
| `licenses`      | `user_id` (nullable), `email` (as signed), `plan` (`desktop`, `pro`), `trial` (bool), `order_id`, `subscription_id`, `period_start` (date), `issued_on`, `updates_until`, `token`, `reissue_of`, `revoked_at`, `revoke_reason`                                                                                                                                                                                      | check: Desktop has `order_id` and no `subscription_id`; Pro has `subscription_id` and `period_start`. Unique `order_id` where `plan = 'desktop' and reissue_of is null`. Unique (`subscription_id`, `period_start`) where `plan = 'pro' and reissue_of is null`. Index `user_id`. |
| `invoices`      | `provider`, `provider_invoice_id`, `user_id` (nullable), `subscription_id`, `order_id`, `description`, `amount_cents`, `currency`, `status`, `issued_at`, `receipt_url`                                                                                                                                                                                                                                             | unique (`provider`, `provider_invoice_id`); index (`user_id`, `issued_at desc`)                                                                                                                                                                                                   |

The two partial unique indexes on `licenses` are the business facts P7 relies on: a duplicate or reordered webhook for the same order or the same Pro period fails the insert, whatever its event id. A support reissue sets `reissue_of` and is outside the index on purpose. `provider_updated_at` lets P7 ignore a subscription event older than the stored state. `licenses.id` is the `id` inside the signed token.

### Claiming purchases

Purchases can exist before an account does ("use the email from your receipt and it will be waiting"), so `user_id` is nullable on orders, subscriptions, licenses and invoices. `claimPurchases(db, userId)` takes only the user id. In one transaction it locks the user row, reads the current `email` and `email_verified`, does nothing unless the email is verified by convt (section 2), and then attaches unclaimed rows with that email. It runs from `databaseHooks.user.create.after` and from `databaseHooks.user.update.after` whenever `email` or `email_verified` changed, which covers sign-up, the later email verification of an OAuth sign-up, sign-in by code to an unverified account, and an email change, whichever endpoint caused it. P7's webhook handler calls the same function and never matches purchases to users itself.

API and cloud:

| Table          | Columns                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                  | Constraints and indexes                                                                                                                                                                               |
| -------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `api_keys`     | `user_id`, `name`, `prefix` (display, such as `cvt_live_8f3a`), `secret_hash` (SHA-256 of the whole key, bytea), `last_used_at`, `revoked_at`                                                                                                                                                                                                                                                                                                                                                            | unique `secret_hash`; unique `prefix`; index `user_id` where `revoked_at is null`; FK cascade                                                                                                         |
| `cloud_jobs`   | `user_id`, `source` (`api`, `web`, `desktop`), `api_key_id`, `status` (`created`, `uploaded`, `queued`, `running`, `succeeded`, `failed`, `cancelled`), `input_format`, `target_format`, `options` (jsonb), `input_key`, `input_bytes`, `output_keys` (jsonb), `attempt`, `max_attempts` (3), `lease_owner`, `lease_expires_at`, `reserved_bytes`, `reserved_cents`, `reservation` (`open`, `settled`, `released`), `error_code`, `error_detail`, `queued_at`, `started_at`, `finished_at`, `expires_at` | index (`queued_at`) where `status = 'queued'`; index `lease_expires_at` where `status = 'running'`; index (`user_id`, `created_at desc`); index `expires_at`; FK cascade on user, set null on API key |
| `usage_events` | `user_id`, `subscription_id`, `api_key_id`, `job_id` (text, no FK), `kind` (`api_conversion`, `pro_bytes`, `correction`), `quantity`, `amount_cents`, `corrects` (nullable, references `usage_events`), `occurred_at`, `reported_at`, `provider_event_id`                                                                                                                                                                                                                                                | unique (`job_id`, `kind`) where `corrects is null`; index (`user_id`, `occurred_at`); index `occurred_at` where `reported_at is null`; FK set null on user, subscription and key                      |

`attempt` is P9's fencing token, the lease columns serve `FOR UPDATE SKIP LOCKED` claims, and the reservation columns hold P9's limit check without another table. `usage_events.job_id` has no foreign key so the billing record outlives the job row, and its unique index makes metering idempotent. The API page's "Failed" count comes from `cloud_jobs`.

Usage facts are append-only, enforced in the database:

- `convt_web` and `convt_server` have no `DELETE` on `usage_events`, and `UPDATE` only on `reported_at` and `provider_event_id`.
- A `BEFORE UPDATE` trigger allows only two changes: setting `reported_at` and `provider_event_id` once, from null; and setting `user_id`, `subscription_id` or `api_key_id` to null. The second is how foreign key `SET NULL` actions anonymize rows when a user, subscription or key is deleted; those actions run as the table owner, so no app role needs the column grant. A `BEFORE DELETE` trigger rejects every delete, including the owner's.
- A mistake is fixed with a new `correction` row whose `corrects` names the original and whose quantity and amount are the signed difference.

Orders, invoices and licenses get no `DELETE` grant either; their updates (refund status, revocation) stay normal updates in P7.

P7 adds `webhook_events` (unique provider event id, raw body, processed time) and `email_outbox`, both new tables.

### Deletion

- Hard delete: users (account deletion), sessions, accounts, verifications, rate limit rows, and expired cloud jobs after their files are purged. Deleting a user cascades to sessions, accounts, devices, API keys and jobs.
- Soft delete, kept as records: licenses (`revoked_at`), API keys (`revoked_at`, so a revoked key's hash still blocks reuse), devices (`revoked_at`, purged after 90 days).
- Financial rows (orders, subscriptions, licenses, invoices, usage events) are never deleted. Account deletion sets their `user_id` to null and keeps the email, because tax records and license resends need it. The privacy policy (P10) has to say so.
- convt-server runs a cleanup task every 10 minutes that deletes expired rows from `verifications`, `rate_limits` and `otp_send_limits`.

## 2. Auth

### Better Auth configuration

One `createAuth(db, env)` in `apps/web/src/server/auth.ts`, built per request because a Worker cannot share a database connection across requests:

- `database`: Drizzle adapter, `provider: "pg"`, `usePlural: true`. Email and password sign-in is off.
- `emailOTP` plugin: 6 digits, `expiresIn: 900` (the plugin's default is 300; 15 minutes matches `magicLinkMinutes`), `allowedAttempts: 3`, `storeOTP: "hashed"`, `resendStrategy: "rotate"` (with hashed storage `reuse` falls back to rotate anyway), `changeEmail: { enabled: true, verifyCurrentEmail: false }`. Confirmed in the 1.7.7 source: a correct code is consumed atomically, so it works once; three wrong tries lock it; change-email is off unless enabled.
- `disabledPaths`: `/email-otp/request-password-reset`, `/forget-password/email-otp`, `/email-otp/reset-password` and `/email-otp/check-verification-otp`. The plugin mounts password reset even with passwords off; a reset would add a password to an account.
- `tanstackStartCookies()` from `better-auth/tanstack-start`, last in `plugins`. Without it, cookies set by `auth.api` calls inside server functions never reach the browser (confirmed in the Better Auth TanStack Start docs).
- Social providers: production registers `socialProviders.github` (scope `user:email`) and `socialProviders.google`. Without credentials (every environment in P6), the `genericOAuth` plugin registers providers with the same ids against the local OAuth mock. In 1.7.7 generic providers are merged into the social provider list and use the same `/sign-in/social`, `/link-social` and `/callback/:id` endpoints (confirmed in `plugins/generic-oauth/index.mjs`), so the client code and callback URLs are identical in both setups.
- `session`: `expiresIn` 30 days, `updateAge` 1 day, `freshAge` 1 hour, `cookieCache` off so revocation takes effect on the next request.
- `account.accountLinking`: `enabled: true`, `trustedProviders: []`, `disableImplicitLinking: true`, `allowDifferentEmails: true` (explicit linking only), `requireLocalEmailVerified` left at its default `true`. `encryptOAuthTokens: true`.
- `advanced`: `cookiePrefix: "convt"`, `useSecureCookies` in production, `ipAddress.ipAddressHeaders: ["cf-connecting-ip"]`, `generateId` as above.
- `trustedOrigins`: `BETTER_AUTH_URL` only.
- `logger`: a function that drops request bodies and passes messages through the redactor described under the emailed link.
- `hooks.before` and `hooks.after`, and `databaseHooks`, as described below.

### When an email counts as verified

`users.email_verified` means convt has evidence the user controls the address. Only two sources count:

- A convt email code to that address (sign-in, email verification or email change).
- An authoritative identity provider: Google when `email_verified` is true and the address is `@gmail.com` or `@googlemail.com`, or the ID token's `hd` claim is present and equals the address's domain. Google's flag alone is not enough: for other addresses it can describe a past check of an address Google does not control.

GitHub is never authoritative. Its `/user/emails` `verified` flag means GitHub once confirmed the address, not that the GitHub user controls it now, and Better Auth reads the public profile email before the primary one (confirmed in `social-providers/github.mjs`). Each provider's `mapProfileToUser` overrides `emailVerified` with this rule; Better Auth spreads the mapped fields after its own (confirmed in `google.mjs`, `github.mjs` and the generic plugin), so the override wins.

Consequences:

- Implicit linking is off. Signing in with GitHub or Google whose email belongs to an existing account fails with `account_not_linked`; the sign-in page then says to sign in with the email code and connect the provider from settings.
- A new GitHub sign-up, or a Google sign-up with a non-authoritative address, creates a user with `email_verified = false`. The shell guard sends such a user to `/sign-in/verify-email`, which sends a code to the address (`/email-otp/send-verification-otp`, type `email-verification`) and checks it (`/email-otp/verify-email`). The dashboard, purchase claiming and the account data stay unavailable until then.
- If the real owner of that address later signs in with an email code from a different browser, they land in that unverified account. An after hook on `/sign-in/email-otp` notices the user was unverified before the request, then unlinks every OAuth identity on it and revokes every other session, because none of them was proven to belong to the address owner. Verifying from the unverified account's own session keeps its identity, since that session proved both sides.

### Linking from settings

Better Auth's `/link-social` uses the plain session middleware, not the fresh one; only `/unlink-account` checks `freshAge` (confirmed in `api/routes/account.mjs`). So:

- A `hooks.before` on `/link-social` rejects with `SESSION_NOT_FRESH` when the session is older than `freshAge`, and rejects any request body with `idToken`, which would link without a redirect. This applies to direct calls to `/api/auth/link-social` as well as the client.
- The same hook attaches `{ linkIntent: { userId, sessionId, provider, at } }` with `addOAuthServerContext`. Values set that way are stored in the OAuth state and cannot be set by the client (confirmed in `api/state/oauth.mjs`).
- `databaseHooks.account.create.before` reads `getOAuthState()`. When it carries a `link`, the hook aborts unless `serverContext.linkIntent` exists, names the same user and provider as `link`, is at most 10 minutes old, and its session still exists, belongs to that user and is still fresh. Better Auth already makes the state single-use, binds it to the browser with a state cookie and expires it after 10 minutes. The spike confirms that the database hook sees the parsed callback state; if it does not, the same check runs in a `hooks.before` on `/callback/:id` after parsing the state there.
- Unlinking uses `/unlink-account`, which already requires a fresh session. The email method cannot be removed.

### Changing the email

The plugin's flow, not core `changeEmail`: `POST /api/auth/email-otp/request-email-change` (sends a code to the new address; refuses an address already in use) and `POST /api/auth/email-otp/change-email` (checks the code, checks for a collision again, sets the email and `email_verified`). Neither endpoint checks `freshAge` (both use `sensitiveSessionMiddleware`, which only skips the cookie cache), so the same freshness hook covers both. A `hooks.after` on `/email-otp/change-email` revokes every other session and every device. Purchase claiming for the new address follows from the `user.update.after` hook. `verifyCurrentEmail` stays off so a user who lost the old mailbox can still move; freshness plus the new-address code is the protection.

### The emailed link

The email holds the code and a link to `/sign-in/verify#email=<address>&code=<code>`. The fragment never reaches the server, the referrer or logs. The page reads it on the client, removes it at once with `history.replaceState`, and shows the address with one "Continue" button that posts the code to `/api/auth/sign-in/email-otp`. Nothing is sent on load, so mail scanners cannot use the code. The page sends `Cache-Control: no-store` and `Referrer-Policy: no-referrer`. A redactor used by every server log call replaces values of fields named `otp` or `code` and any 6-digit run in auth paths; Mailpit holds plain codes, which is acceptable only because it runs locally.

### Redirects after sign-in

`safeRedirect(value, origin)` is the only way a `redirect` parameter becomes a navigation. It rejects any value containing a backslash or a control character (before and after one round of percent-decoding), resolves the rest with `new URL(value, origin)`, requires the result's origin to equal the configured origin exactly, and returns only `pathname + search + hash`. Anything rejected becomes `/dashboard`. Better Auth applies `trustedOrigins` to its own callback URLs. Unit tests cover `//evil.example`, `/\evil.example`, `/%5Cevil.example`, `%2F%2Fevil.example`, `https://evil.example`, `javascript:alert(1)`, tabs and newlines inside the path, and a normal path with a query and hash.

### Cookies, sessions and CSRF

The session cookie is `HttpOnly`, `Secure` in production (`__Secure-` prefix), `SameSite=Lax`, `Path=/`, and host-only on `convt.app`. Lax is needed because the OAuth callback is a top-level cross-site navigation. `api.convt.app` never sees the cookie; P9's Worker calls convt-server with its own short-lived token.

Better Auth checks `Origin` on its own POST endpoints. TanStack Start server functions get the same check from a global request middleware: any request that is not GET or HEAD must carry `Origin` equal to the site origin, or `Sec-Fetch-Site: same-origin`, else 403. Dashboard responses send `Cache-Control: private, no-store`.

Hyperdrive caches read queries for up to 60 seconds by default, which would let a revoked session keep working. The Hyperdrive config must be created with caching disabled.

### Rate limits

Better Auth's limiter with `storage: "database"`, on in tests too, keyed by IP and path:

- OTP check paths (`/sign-in/email-otp`, `/email-otp/verify-email`, `/email-otp/change-email`): 10 per IP per minute, on top of 3 attempts per code.
- OAuth start and callback: 20 per IP per minute.
- Everything else under `/api/auth`: Better Auth's default of 100 per minute.

Sending a code has its own limit, because the plugin issues and stores the new code before it calls `sendVerificationOTP` (confirmed in `routes.mjs`). A check inside that callback would come too late: a denied resend would already have replaced the previous code. Instead, a `hooks.before` on every path that sends a code (`/email-otp/send-verification-otp` and `/email-otp/request-email-change`; the password reset senders are disabled) consumes two buckets in `otp_send_limits` before the handler runs: `email:<sha256 of the target address>` (3 per 15 minutes) and `ip:<ip>` (10 per hour). Each bucket is one statement, `INSERT ... ON CONFLICT (key) DO UPDATE` that restarts the window when it has expired, otherwise increments, and returns the count, so concurrent requests cannot both pass. Over the limit returns 429 before any code is issued, so the previous code stays valid. Rows are kept for the window, independent of verification rows, and the cleanup task removes expired ones.

Responses never say whether an email has an account.

### Sign out

"Sign out" ends the current session. "Sign out everywhere else" revokes every other session and every device. Per-row "Sign out" revokes one session or one device. Changing the email signs out every other session.

### Threat model in brief

| Threat                                                       | Mitigation                                                                                                                                                                                                     |
| ------------------------------------------------------------ | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Guessing a sign-in code                                      | 6 digits, 3 attempts per code, per-IP limits, 15-minute expiry, hashed at rest.                                                                                                                                |
| Flooding a mailbox or burning a user's code with resends     | Atomic per-email and per-IP send buckets checked before a code is issued.                                                                                                                                      |
| Link prefetch, logs, history and referrers                   | Code in the URL fragment, stripped on load, sent only by the button; logs redacted.                                                                                                                            |
| Takeover through an OAuth identity with someone else's email | Only authoritative identities count as verified; no implicit linking; unverified accounts cannot claim purchases; verifying by code from elsewhere drops unproven identities.                                  |
| Linking an attacker's identity through a stolen, old session | Fresh session required on `/link-social` and email change; the callback must match a server-recorded link intent.                                                                                              |
| Stolen session                                               | HttpOnly, Secure, Lax cookie; 30-day expiry; session list with revoke; no cookie cache; Hyperdrive cache off.                                                                                                  |
| CSRF                                                         | Lax cookie plus Origin checks on Better Auth and server functions.                                                                                                                                             |
| Open redirect                                                | `safeRedirect`; `trustedOrigins` for Better Auth callbacks.                                                                                                                                                    |
| One user reading another's data                              | Every query takes the user id from the session in server middleware. Integration tests read across two seeded users.                                                                                           |
| Leaked license key from the dashboard                        | The full token is fetched only by the "Copy key" and "Activate" server function, with `no-store`. Masked everywhere else.                                                                                      |
| Rewriting billing history                                    | Append-only usage enforced by grants and triggers; no deletes on financial tables.                                                                                                                             |
| Database leak                                                | OTPs and API keys hashed, OAuth tokens encrypted. Session tokens are stored as Better Auth stores them (plain); accepted, and the database is reachable only through Hyperdrive and Railway's private network. |
| A dev mail sink or OAuth mock reaching production            | Both are refused at startup unless their URL is loopback and `ENV` is not `production`.                                                                                                                        |

## 3. Routes and data layer

### Routes

| Path                                     | Kind                     | Behavior                                                                                                                                          |
| ---------------------------------------- | ------------------------ | ------------------------------------------------------------------------------------------------------------------------------------------------- |
| `/api/auth/$`                            | server route (GET, POST) | `auth.handler(request)`                                                                                                                           |
| `/sign-in`                               | page                     | Signed in: redirect through `safeRedirect`. The form calls `/email-otp/send-verification-otp` (type `sign-in`), then goes to check-email.         |
| `/sign-in/check-email`                   | page                     | Code entry signs in; "Resend link" sends a new code. Errors (wrong, expired, used, rate limited) show inline. The "Preview only" note is removed. |
| `/sign-in/verify`                        | new page, `AuthLayout`   | Confirm button for the emailed link, as described above.                                                                                          |
| `/sign-in/verify-email`                  | new page, `AuthLayout`   | Email verification for an OAuth sign-up whose address convt has not verified. Reuses the code input from check-email.                             |
| `/_app/_shell` (layout)                  | `beforeLoad`             | `getSession()`. No session: redirect to `/sign-in?redirect=<path>`. Unverified email: redirect to `/sign-in/verify-email`.                        |
| `/dashboard`, `/dashboard/*`, `/account` | pages under the shell    | Loaders call the server functions below.                                                                                                          |

Pages call server functions (`createServerFn`); only Better Auth's handler is an API route. Every server function that reads or writes account data uses an `authed` middleware that validates the session cookie again (route context is never trusted on its own), refuses unverified users, and passes `userId` and a request-scoped Drizzle client in context. Query functions live in `packages/db/src/queries` and take `(db, userId, now)`, so integration tests run them in Bun. Mapping to the view types in `src/lib/types.ts` (masked keys, "Updates until Aug 20, 2027", "Seen today") lives in `apps/web/src/server/views.ts` as pure, unit-tested functions.

`src/lib/account.ts` keeps its exports and names; their bodies become server function calls. `placeholder.ts` is deleted.

| Function                                               | Reads or does                                                                                                 |
| ------------------------------------------------------ | ------------------------------------------------------------------------------------------------------------- |
| `getSession`                                           | Session and account for the header.                                                                           |
| `getOverview`                                          | State, entitlements, newest license, API month count and key count, active devices.                           |
| `getLicenses`                                          | All licenses, newest first, and active devices.                                                               |
| `getLicenseKey(id)`                                    | Full token for "Copy key" and "Activate on this computer", owner-checked, `no-store`.                         |
| `getBilling`                                           | Pro subscription or null, API enrollment or null, invoices. Card details stay null until P7.                  |
| `getApiOverview`                                       | Keys, month and 30-day counts, per-day series, failed jobs.                                                   |
| `getAccountSettings`                                   | Profile, sign-in methods (email plus `accounts` rows), web sessions and devices.                              |
| `updateName`                                           | Better Auth `updateUser`.                                                                                     |
| change email                                           | `/email-otp/request-email-change`, then `/email-otp/change-email`, with a code step on the settings page.     |
| `revokeSession`, `revokeOtherSessions`, `revokeDevice` | As described under sign out.                                                                                  |
| connect and remove a method                            | `/link-social` and `/unlink-account`; a stale session gets "Sign in again to continue" and a link to sign-in. |

Still placeholder actions after P6, with the existing notice: billing actions (P7), create and revoke API key (P9, which owns the shown-once screen), delete account (P7), Apple sign-in, and the download links. The "Sample data" badge in the header is removed, since every page now shows the signed-in user's own rows. Seeded fixtures exist only in local databases.

### Activate on this computer

`docs/plan.md` P6 lists an activate button on the license page. Each license card on `/dashboard/licenses` gets "Activate on this computer" next to "Copy key", shown for unrevoked licenses only. It fetches the token through `getLicenseKey` on click (the token is never in the page HTML) and opens `convt://activate?key=<token>` with `window.location.assign`. The desktop app already parses that link and shows the key in Settings for the user to confirm with a click (`crates/convt-app/src/request.rs`); the app needs no change.

What works in P6: on Linux, after `integrations/linux/install.py` registers `convt-app.desktop` as the `convt://` handler, the link opens the app with the key filled in, and a seeded key activates because the seed signs with the local dev key the dev build embeds. On macOS and Windows the link does nothing until P4 and P5 register the scheme; the button stays visible because a user who installed the app has the handler. Acceptance: an e2e check stubs `window.location.assign` and asserts the URL is `convt://activate?key=` followed by the token for that license, that `getLicenseKey` refuses another user's license, and that no token appears in the server-rendered HTML. Opening the link in the real app on Linux is a GUI check, run only with Leo's permission and with `HOME` and the XDG directories in a `mktemp` directory and `CONVT_LICENSE_STORE=file`; otherwise it is reported as not checked.

### Overview and billing states

`Overview` gains `state` and nullable `plan`, `license` and `api`. `deriveAccountState(subscriptions, licenses, now)` is a pure function with this precedence: `pro` (active or past due) over `trial` (Pro trialing) over `desktop` (an unrevoked Desktop license) over `pro_lapsed` (a Pro subscription that ended) over `api_only` (an API subscription and nothing else) over `new`. API enrollment is independent of the state: the API card shows whenever there is an API subscription. A missing plan, license or API enrollment renders the same `SummaryCard` with a short empty line and a link (pricing or the API docs).

`Billing` changes the same way: `plan` becomes nullable and covers only the Pro subscription; `api` is a separate nullable `{ status, spendCapCents }`. With no Pro plan, the plan card shows "No plan" and a link to pricing in place of the plan and its actions; Desktop purchases appear only in the invoice list. An API enrollment shows as its own row under the plan card. Lapsed Pro shows the plan with the `CANCELED` badge.

Seeded fixture accounts, all on the reserved `.test` domain, each checked on `/dashboard`, `/dashboard/licenses`, `/dashboard/billing` and `/dashboard/api`:

| Email                  | State        | Data                                                                                                                                           |
| ---------------------- | ------------ | ---------------------------------------------------------------------------------------------------------------------------------------------- |
| `new@convt.test`       | `new`        | Nothing.                                                                                                                                       |
| `trial@convt.test`     | `trial`      | Pro monthly trialing, ends in 3 days, trial key.                                                                                               |
| `desktop@convt.test`   | `desktop`    | Desktop order, key and invoice, one device.                                                                                                    |
| `pro@convt.test`       | `pro`        | The current sample data: Pro yearly, an older Desktop key, two devices, API with two keys and 30 days of usage, three invoices, GitHub linked. |
| `lapsed@convt.test`    | `pro_lapsed` | Pro ended last month, last key covering builds to that date, a past-due invoice.                                                               |
| `api@convt.test`       | `api_only`   | API subscription, one key, usage, some failed jobs.                                                                                            |
| `unclaimed@convt.test` | none         | A Desktop order and license with no user, for the claim tests.                                                                                 |

Seed dates are relative to the seed run. License tokens are real: the seed signs them with the local dev key (`.convt-dev/license.key`, created in the dev-keys format if missing, never the test-vector seed) through a TypeScript Ed25519 signer in `packages/license` that must pass the P0 vectors. P7 reuses the signer.

## 4. Local development and testing

### Per-checkout services

`scripts/db.sh` manages the local services for one checkout. The repo is not a git repository, so "worktree" means the checkout directory.

- Names: `convt-pg-<h>` and `convt-mail-<h>`, where `<h>` is the first 8 hex characters of the SHA-256 of the checkout's absolute path. Containers and the volume `convt-pg-<h>-data` carry the label `convt.checkout=<path>`.
- Images: `postgres:17-alpine` (match Railway's major version when Leo creates the database) and `axllent/mailpit`, both pinned by digest.
- Ports: published on `127.0.0.1` with a random host port (`-p 127.0.0.1::5432`), read back with `docker port`. Nothing binds `0.0.0.0`.
- On first `up`, after creating the roles, `db.sh` creates schema `convt_dev` with table `owner(checkout, container_id, created_at)` and one row. Migrations never touch `convt_dev`, and it never exists in production.
- `up` writes `.convt-dev/services.env` (gitignored, 0600) with the container id, `DATABASE_URL` (as `convt_web`), `SERVER_DATABASE_URL` (as `convt_server`), `OWNER_DATABASE_URL`, `MAILPIT_URL`, `OAUTH_MOCK_URL` and a generated `BETTER_AUTH_SECRET`.
- Commands: `up` (idempotent), `down`, `status`, `url`, `psql`, `prune`.

Before `db:seed`, `db:rollback`, `db:reset` or `down`, a guard checks all of these and refuses otherwise:

1. The URL's host is `127.0.0.1` and its port equals the host port `docker port` reports for the container named for this checkout.
2. That container's id matches `services.env` and its `convt.checkout` label equals this checkout's absolute path.
3. The database has `convt_dev.owner` with a row naming this checkout and that container id.

`down` removes only the container and volume whose names and labels both match. `prune` removes only containers and volumes whose `convt.checkout` label names a path that no longer exists, and lists them first. Test containers get the label `convt.test-run=<random id>`, their id is recorded when created, and cleanup removes exactly that id; they use tmpfs, so they have no volume. Nothing touches unlabeled containers or volumes; other projects run Postgres on this machine.

Root scripts: `db:up`, `db:down`, `db:migrate`, `db:rollback` (one step), `db:reset` (down to zero, up, seed), `db:seed`, `db:ci`. `bun run dev:web` becomes `scripts/dev-web.sh`: `db up`, migrate, start the OAuth mock on a free port, write `apps/web/.dev.vars` from `services.env` (gitignored), and start Vite on 3000 or the next free port with `BETTER_AUTH_URL` set to match.

### Workers dev without Hyperdrive

`wrangler.jsonc` gets a `HYPERDRIVE` binding with a placeholder id. Locally, the Cloudflare Vite plugin and `wrangler dev` read `CLOUDFLARE_HYPERDRIVE_LOCAL_CONNECTION_STRING_HYPERDRIVE`, which `dev-web.sh` sets to the container URL as `convt_web`. The code always reads `env.HYPERDRIVE.connectionString` and connects with `pg` per request. A Start request middleware opens the client, puts the Drizzle instance in context, and closes it after the response, or in `waitUntil` when background work (such as Better Auth's `runInBackgroundOrAwait`) is still running.

### Email and OAuth in development

- Mail goes through `sendMail(message)`, with transports `mailpit` (Mailpit's HTTP send API; its web UI shows the messages), `log` and later `resend`. `mailpit` and `log` are refused in production.
- `tools/oauth-mock` is a small Bun server that speaks enough OAuth 2 and OIDC for both providers: an authorize page listing fixture identities, a token endpoint, a userinfo endpoint and, for Google, a signed ID token with its JWKS. Fixture identities: Gmail with `email_verified` true; Workspace with a matching `hd`; a non-Gmail address with `email_verified` true and no `hd`; Google unverified; GitHub with a verified primary email; GitHub whose public email differs from its primary; GitHub with no email; an error response. Tests pick an identity with a query parameter.

### Test layers

| Layer       | Runs                                                                                                            | Covers                                                                                                                                                                                                                                                                                                                                                                                                                                   |
| ----------- | --------------------------------------------------------------------------------------------------------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Unit        | `bun test` in `apps/web`, `packages/*`                                                                          | View mapping, state precedence, billing view per state, key masking, `safeRedirect` cases, the Origin middleware, the log redactor, the authoritative-email rule per mock identity, the TS signer against the P0 vectors, the API key hash and id generator vectors, Better Auth table shape.                                                                                                                                            |
| Integration | `bun test` against a disposable Postgres, connected as the app roles                                            | Listed below.                                                                                                                                                                                                                                                                                                                                                                                                                            |
| Rust        | `cargo test -p convt-server` with `CONVT_TEST_DATABASE_URL`                                                     | API key lookup by hash, revoked keys rejected, `usage_events` idempotent on job id, the startup migration check (missing, edited and newer migrations), the cleanup task. Without the variable these tests skip; with `CONVT_REQUIRE_DB=1` a skip fails.                                                                                                                                                                                 |
| Drift       | `bun run db:ci`                                                                                                 | See below.                                                                                                                                                                                                                                                                                                                                                                                                                               |
| End to end  | `apps/web/e2e/*.sh` driving `agent-browser` against `dev-web.sh`, then once against `wrangler dev` on the build | Sign in by emailed link and by code (read from Mailpit's API), with the fragment gone from the address bar afterwards; OAuth mock sign-up with each identity; the verify-email step; connect GitHub from settings; every fixture on all four dashboard pages at 1280 and 390 px; copy key; activate link; rename; change email; sign out one session from a second browser session; sign out everywhere; signed-out redirect and return. |

Integration tests, each in Bun with Better Auth in process (`auth.handler(new Request(...))`) and an injected clock:

- Queries for every fixture; two users never see each other's rows; `getLicenseKey` refuses another user's license.
- License uniqueness rejects a second Desktop key per order and a second Pro key per period; a reissue is allowed.
- Claiming: a verified sign-up claims `unclaimed@`; an unverified OAuth account (GitHub, and Google with the third-party fixture) with that email claims nothing, and claims after it verifies by code; an email change to that address claims after confirmation.
- Codes: sent and captured, used once, rejected when expired, locked after three wrong tries, the old code rejected after a resend.
- Send limits: the fourth send to one email in 15 minutes gets 429 and the third code still works; the limit holds across both send paths; 20 concurrent sends to one email issue exactly 3 codes; the window resets after it expires; the IP bucket limits sends across different emails.
- Linking: an OAuth sign-in to an existing email fails with `account_not_linked` and creates nothing; a session older than one hour gets `SESSION_NOT_FRESH` from `/link-social`, called directly; an `idToken` body is rejected; a callback whose link intent is missing, names another user or names a revoked session links nothing; an unverified account taken over by a code sign-in from elsewhere loses its OAuth identities and other sessions.
- Email change: the code goes to the new address only; a stale session is refused; an address taken by another user is refused at request and again at confirmation; after the change, other sessions are rejected and the current one shows the new address.
- Usage immutability: as `convt_server` and `convt_web`, updating `quantity`, `amount_cents` or `job_id`, setting `reported_at` a second time, and deleting a row all fail; deleting the user nulls `user_id` and keeps the row; a correction row is accepted.
- Cookies: attributes of the session cookie, and that a cookie set inside a server function reaches the response (the `tanstackStartCookies` bridge).

The disposable Postgres for tests is a labeled tmpfs container with a random port, started by the test script and removed by id on exit. It holds one template database migrated once; each test file and each Rust test gets `CREATE DATABASE ... TEMPLATE`.

`db:ci` is the CI check, run locally until the GitHub repository exists, then as a job with a Postgres service:

1. Start the disposable Postgres and create the roles.
2. `drizzle-kit check`, then `drizzle-kit generate` in a temporary copy must produce no new migration.
3. Build database A by running the migrations. Build database B from the current schema alone with `drizzle-kit export` (SQL straight from the Drizzle definitions, no migration history) plus the roles, grants and triggers file. Dump both with `pg_dump --schema-only --no-owner`, drop the `drizzle` and `convt_dev` schemas from the dumps, sort the statements, and compare. A hand edit to a migration or a schema change without a migration fails here.
4. On A: run every down file, check that only the `drizzle` schema remains and its table is empty, migrate up again, and compare with the first dump.
5. Run the integration tests.
6. From `crates/convt-server`, with `DATABASE_URL` set to A: `cargo sqlx prepare --check -- --all-targets`. Then, with `DATABASE_URL` unset and `SQLX_OFFLINE=true`, `cargo build -p convt-server --all-targets`, which proves the committed `.sqlx` data compiles offline. Then the Rust database tests with `CONVT_REQUIRE_DB=1`.

The `.sqlx` data is generated with `cargo sqlx prepare -- --all-targets` from `crates/convt-server`, so it lives in `crates/convt-server/.sqlx`. `scripts/setup.sh` installs `sqlx-cli` with only the `postgres` and `rustls` features, pinned to the same version as the `sqlx` crate.

No test touches a keyring or real user data. P6 adds no keyring code. Any check that activates a seeded key in the CLI or the app runs with `CONVT_LICENSE_STORE=file` and `HOME`, `XDG_CONFIG_HOME` and `XDG_DATA_HOME` set to `mktemp` directories.

### Changes to the test-convt-web skill

The skill currently says the site has no auth, database or environment. It changes to:

- Doctor: `docker info`, `scripts/db.sh status`, the ports of this checkout's services.
- Launch: `bun run dev:web` (services, migration, OAuth mock, Vite), then `bun run db:seed`. Readiness adds `/api/auth/ok` returning 200.
- Sign in as a fixture: the fixture table, how to read the code from Mailpit's API, how to pick an OAuth mock identity.
- Evidence: the e2e scripts and the screenshots each fixture needs on each page.
- Preview on another device: secure cookies are off in development, but `BETTER_AUTH_URL` and the OAuth mock redirect must use the tailnet origin.
- Cleanup: stop the OAuth mock and Vite by PID; `scripts/db.sh down` only when the database is no longer wanted.

test-convt-server gains the database steps, and CLAUDE.md gains `packages/db`, `packages/license`, `tools/oauth-mock` and the `db:*` commands.

## 5. Work breakdown

The checkout is shared and not under git. The Linux packaging agent works in `packaging/` and `integrations/linux` at the same time, so P6 does not check those directories; it tracks its own writes.

- P6 writes only inside this allowlist: `apps/web`, `packages/db`, `packages/license`, `tools/oauth-mock`, `crates/convt-server`, `scripts/db.sh`, `scripts/dev-web.sh`, `scripts/setup.sh`, `docs/p6-accounts-plan.md`, `.agents/skills/test-convt-web`, `.agents/skills/test-convt-server`, `CLAUDE.md`, `.gitignore`, and the shared files below.
- Before step 2, record a manifest (path, size, SHA-256) of the tree without `node_modules`, `target`, `dist`, `.wrangler`, `.convt-dev` and `vendor`. Each step appends the paths it wrote to `.convt-dev/p6-writes.txt`. The final check diffs the tree against the manifest: every changed path inside the allowlist must be in the write log, every path in the write log must be inside the allowlist, and changes outside the allowlist are listed and attributed to whoever owns them, not counted as P6's. `crates/convt-cli`, `convt-app`, `convt-license`, `convt-engines` and `convt-ffi`, which no other agent is editing, must be unchanged.
- Shared files (root `package.json`, `bun.lock`, root `Cargo.toml`, `Cargo.lock`) are edited in one place each: step 2 adds every root script and workspace entry P6 needs in a single edit, step 4 and step 6 run the only `bun install`s, and step 5 the only Cargo change. Each edit re-reads the file immediately before writing, changes only P6's keys, and diffs the file right after to confirm nothing else moved. If the packaging agent has an install or edit in flight, P6 waits.

| Step | Work                                                                                                                                                                                                                                                                                                                                                                                      | Checks                                                                                                                                                                                                                                                                                                                                                                 | Depends on |
| ---- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ---------- |
| 1    | Spike in a `mktemp` project with the pinned versions, under both the Vite plugin and `wrangler dev`: Hyperdrive local connection string with `pg`; a server function that signs in through `auth.api` with `tanstackStartCookies`; email OTP and a `genericOAuth` round trip; the link-intent check in `account.create.before`; Mailpit send. Record any change of plan in this document. | In both modes: the session cookie set by a server function persists in the browser across reloads; a session past `updateAge` is refreshed and the new expiry reaches the cookie; after awaited and background queries, the `pg` client is closed (server-side connection count returns to baseline); OTP and mock OAuth round trips; a forged link intent is refused. | none       |
| 2    | `scripts/db.sh` with the ownership guard, `roles.sql`, `.gitignore` entries (`.dev.vars`), `.dev.vars.example`, root `db:*` scripts, the write manifest.                                                                                                                                                                                                                                  | `up` twice is a no-op; a copy of the checkout at another path gets other names and ports; the guard refuses a URL for another container, a container with another label, and a database without the marker; `down` and `prune` remove only matching resources (list before and after).                                                                                 | 1          |
| 3    | `packages/license`: TS Ed25519 signer and verifier, id generator, API key hash.                                                                                                                                                                                                                                                                                                           | Passes `crates/convt-license/tests/vectors.json`; a token it signs verifies with `convt-license`.                                                                                                                                                                                                                                                                      | none       |
| 4    | `packages/db`: schema, migration 0000 with grants and triggers, its down file, migrate and rollback CLIs (rollback updates the migrations table), query functions, `claimPurchases`, seed, integration tests for queries, claims and usage, `db:ci` steps 1 to 5.                                                                                                                         | `bun run db:ci` passes; the seed is idempotent; the guard refuses a database it does not own.                                                                                                                                                                                                                                                                          | 2, 3       |
| 5    | convt-server: sqlx pool, startup migration check, cleanup task, API key lookup, usage insert, `.sqlx` data, template-database test helper, `db:ci` step 6.                                                                                                                                                                                                                                | `cargo test` passes with no database (skips) and with one; `bun run rs:check`; the offline build; `db:ci` passes end to end.                                                                                                                                                                                                                                           | 4          |
| 6    | Auth core in `apps/web`: `createAuth`, `/api/auth/$`, hooks (send limits, freshness, link intent, takeover cleanup, email change), the authoritative-email mapping, mail transports, log redactor, `tools/oauth-mock`, Origin and `authed` middleware, `getSession`, `safeRedirect`; auth integration tests.                                                                              | Integration tests in section 4; `bun run check && bun run check-types && bun run build`.                                                                                                                                                                                                                                                                               | 4          |
| 7    | Sign-in pages wired, `/sign-in/verify`, `/sign-in/verify-email`, shell guard and redirects, sign-out link.                                                                                                                                                                                                                                                                                | e2e: sign in by link and by code, OAuth mock identities, verify-email, signed-out redirect; with Leo's permission for the browser.                                                                                                                                                                                                                                     | 6          |
| 8    | Data layer swap: server functions, `views.ts`, overview and billing states, copy key and activate, API read paths, settings actions including change email and connect, device revoke; delete `placeholder.ts` and the header badge.                                                                                                                                                      | Unit tests for views and state; e2e of every fixture on all four dashboard pages at both widths; activate link acceptance; settings flows; `grep -r placeholder src/lib` is empty.                                                                                                                                                                                     | 7          |
| 9    | Skill, CLAUDE.md and README updates; e2e scripts made repeatable.                                                                                                                                                                                                                                                                                                                         | A fresh run of the updated skill reaches a signed-in dashboard.                                                                                                                                                                                                                                                                                                        | 8          |
| 10   | Independent GPT-6.1 Sol review of the diff, fixes, full verification.                                                                                                                                                                                                                                                                                                                     | All of the above, plus the write-log check against the manifest.                                                                                                                                                                                                                                                                                                       | 9          |

Parallel lanes: steps 1 and 3 together; after 4, steps 5 (`crates/convt-server`) and 6 (`apps/web`, `tools`) together, with step 5 owning `Cargo.lock` and step 6 owning `bun.lock`. Steps 7 and 8 edit the same route files and run in sequence. The skill text in step 9 can start during step 8.

## Accounts and secrets Leo creates later

None of these are needed to build and verify P6.

| What                                                                                                 | Where it goes                                                                                                                            |
| ---------------------------------------------------------------------------------------------------- | ---------------------------------------------------------------------------------------------------------------------------------------- |
| Railway project with a Postgres database and the convt-server service                                | Run `packages/db/roles.sql` once. `DATABASE_URL` (as `convt_server`) on convt-server. A public TCP proxy on the database for Hyperdrive. |
| Cloudflare Hyperdrive config for that database as `convt_web`, **caching disabled**                  | Its id in `apps/web/wrangler.jsonc` (`hyperdrive[0].id`).                                                                                |
| The `convt_owner` connection URL                                                                     | Only in the environment that runs `bun run db:migrate` for deploys. Never on the Worker or convt-server.                                 |
| `BETTER_AUTH_SECRET` (32 random bytes)                                                               | `wrangler secret put BETTER_AUTH_SECRET`. `BETTER_AUTH_URL=https://convt.app` as a var in `wrangler.jsonc`.                              |
| GitHub OAuth App, callback `https://convt.app/api/auth/callback/github`                              | `GITHUB_CLIENT_ID` as a var, `GITHUB_CLIENT_SECRET` as a Worker secret.                                                                  |
| Google OAuth client (Web) with consent screen, callback `https://convt.app/api/auth/callback/google` | `GOOGLE_CLIENT_ID` as a var, `GOOGLE_CLIENT_SECRET` as a Worker secret.                                                                  |
| Email provider account (Resend by default) and its DNS records on convt.app                          | `RESEND_API_KEY` as a Worker secret, `MAIL_FROM` as a var. The DNS change needs Leo's approval.                                          |

## Step 1 spike results (2026-10-04)

The spike ran in a `mktemp` copy of `apps/web` with the pinned versions, a tmpfs `postgres:17-alpine`, Mailpit and a small OAuth mock, under the Vite plugin and under `wrangler dev` on the build. Both modes passed every check: the `HYPERDRIVE` binding's local connection string with `pg`; the Start request middleware's context reaching server routes and server functions; a session cookie set by `auth.api.signInEmailOTP` inside a server function persisting across reloads; a session past `updateAge` refreshed during SSR with the new expiry in the browser's cookie; the `pg` client closed after an awaited query and after a background query (connections as `convt_web` went 0, 1, 0); `waitUntil` from `cloudflare:workers` in both modes; OTP by Mailpit; `genericOAuth` sign-up and linking; `getOAuthState()` in `account.create.before` showing `link` and `serverContext.linkIntent`; and a link without an intent creating nothing.

What changed in this plan because of it:

- `account.accountLinking.trustedProviders` is `["github", "google"]`, not empty. Better Auth refuses an explicit link (`linkOAuthAccount`, and the `idToken` path of `/link-social`) when the provider is untrusted and the mapped `emailVerified` is false, which our mapping makes true for every GitHub identity. With `disableImplicitLinking: true`, `trustedProviders` has no other effect in 1.7.7 (checked every use), so sign-in still never links implicitly.
- Better Auth 1.7.7 already does the takeover cleanup on `/sign-in/email-otp`: `revokeUnprovenAccountAccess` deletes every account row and every session of an unverified user before the new session is created. The after hook only adds device revocation, and the integration test checks the combined result.
- `/email-otp/request-email-change` inserts its code without the delete-and-retry that `resolveOTP` has, so with the unique `verifications.identifier` a second request for the same new address would fail. The send-limit hook deletes the old change-email row after the limit passes, which gives the same rotate behavior as the other paths.
- For an address already in use, `/email-otp/request-email-change` answers success and sends nothing, so responses do not reveal accounts; `/email-otp/change-email` refuses it. The tests check that no code is sent and that confirmation fails.
- The link-intent check returns `false` from `account.create.before` instead of throwing, so the callback redirects to the error page instead of answering 403 JSON.
- Better Auth's default limiter rule gives every `/sign-in*` path 3 requests per 10 seconds, and `customRules` override plugin rules, so every path named under rate limits gets an explicit custom rule, including the send paths (100 per minute, leaving the send buckets in charge).
- `createServerFn().inputValidator()` is deprecated in this Start version; the code uses `.validator()`.
- The session cookie is `convt.session_token` (`__Secure-convt.session_token` in production).
- `vite build` copies `apps/web/.dev.vars` into `dist/server`. `dist` is gitignored and `wrangler deploy` does not upload `.dev.vars`, so this only matters for anyone copying build output by hand.

## Changes made during implementation (2026-10-04)

These differ from the sections above. Each was needed to make the plan work as written or to test it.

- **rusqlite 0.39.0 instead of 0.40.2.** Cargo enforces `links = "sqlite3"` across the whole lockfile, optional dependencies included, so sqlx 0.9.0 (whose SQLite crate allows `libsqlite3-sys` below 0.38) cannot sit next to rusqlite 0.40 (0.38) even though convt-server never uses SQLite. rusqlite 0.39 uses `libsqlite3-sys` 0.37 and keeps `bundled`; convt-app's 80 tests, including the history ones, pass unchanged. This keeps sqlx on crates.io instead of a git pin. Move both forward once a sqlx release allows 0.38.
- **Sign-in codes are stored by our hook, not the plugin.** The email OTP plugin inserts a code and, when the unique `identifier` refuses it, deletes the old row and inserts again; after a wrong guess it consumes the row and writes the same code back with one more attempt. Concurrent sends turned the first into a duplicate-key error, and a resend landing inside the second could bring a replaced code back. `databaseHooks.verification.create.before` now writes OTP rows (and only those) and returns `false` so the plugin skips its own insert. A new code first upserts an `otp-issued:<identifier>` marker holding the code hash, with an expiry forced strictly later than the previous issuance's, then the code row with that expiry, in one transaction. A wrong-guess write-back inserts only while the marker, locked with `FOR UPDATE`, still names the same hash and expiry. So there is one current code per address and purpose, the newest wins, and no replaced code returns in any order of events. This replaced the delete hack for `/email-otp/request-email-change` described in the spike results.
- **Better Auth's inline expiry deletes are off** (`verification.disableCleanup`). Its bulk delete of expired verification rows on every lookup could deadlock with a code being issued. Better Auth already treats expired rows as invalid; convt-server's cleanup task deletes them every 10 minutes, markers first, then the rest, in separate statements.
- **Takeover cleanup does not depend on Better Auth's lock.** Better Auth removes an unverified account's OAuth identities and sessions under a short lock row and, if another attempt holds that lock past two seconds, signs in without the cleanup. After a successful code sign-in to an account that was unverified, our after hook does the same cleanup itself in one transaction (every identity, every session but the new one, every device, then `email_verified` and claiming), and the before hook drops an expired lock row so Better Auth's own path works too. The before hook also records which account owned the address when the request started. If the new session belongs to a different account, because the address moved to another account mid-request, the sign-in is refused with 409 `ACCOUNT_CHANGED`: the new session row is deleted, its cookie expired, and a fresh code is required. The cleanup also runs whenever the signed-in account is still unverified, which covers Better Auth giving up on its lock. Accepted edge: if two code sign-ins to the same unverified account finish at the same moment while a stalled attempt holds the lock, each removes the other's new session and both sign in again; keeping sessions created during the request would instead risk keeping one an attacker opened.
- **`/email-otp/verify-email` only works from the account's own session** while the account is unverified. From anywhere else it answers 403 `VERIFY_FROM_ACCOUNT`; the mailbox owner signs in with a code instead, which removes the account's unproven OAuth identities, sessions and devices. Without this, the owner could verify a squatted account while the squatter stayed signed in.
- **The ownership guard accepts one URL shape**, `postgresql://user:password@127.0.0.1:<port>/convt`, with no query string or fragment, because libpq and node-postgres read parameters like `?dbname=` differently.
- **`safeRedirect` checks its result as well as its input**, since dot segments can normalize `/x/..//evil.example` into `//evil.example`.
- **Only trialing, active and past-due subscriptions count as live** everywhere. An `incomplete` one (an unfinished checkout) shows nowhere.
- **`allowUnlinkingAll: true`.** Better Auth refuses to unlink an account's last `accounts` row, but at convt the verified email always signs in by code, so GitHub or Google can always be removed. `/unlink-account` takes the `accounts` row id (`acc_...`) as `accountId`.
- **Activate uses a temporary link instead of `location.assign`.** Chrome does not let page script replace `location.assign`, and it reports no CDP event for an external-protocol launch, so the planned stub could not intercept the URL. `openActivationLink` clicks a temporary `<a href="convt://activate?key=...">`, which hands the link to the protocol handler the same way, and the e2e check stubs `HTMLAnchorElement.prototype.click`.
- **`invoices.email`.** Claiming attaches unclaimed rows by email, so invoices carry the email like orders, subscriptions and licenses.
- **Id prefixes `ver_` and `rl_`.** Verification and rate-limit rows also get prefixed ids from `generateId`.
- **convt-server is a library plus a thin binary.** The key lookup and usage insert have no route until P9; as library items they are tested without dead-code exceptions.
- **The drizzle schema is created by `roles.sql`.** convt-server reads `drizzle.__drizzle_migrations` at startup, so `roles.sql` creates the `drizzle` schema owned by `convt_owner` and grants `convt_server` read access (also by default privileges for the table the migrator creates).
- **`drizzle-kit export` ignores `casing`.** It prints camelCase column and constraint names, so `db:ci` converts quoted identifiers to snake_case before building database B. The dump comparison proves the conversion is exact: any difference fails the check.
- **The cookie bridge is tested in the running app.** `tanstackStartCookies` only acts inside a TanStack Start request, so the "cookie set inside a server function reaches the response" check runs in the spike and the e2e sign-in (both Workers dev modes), not in the Bun integration suite, which checks cookie attributes.
- **Server-only code stays out of the browser.** The Start compiler drops imports that only server handler bodies use. Helpers that touch the database or Better Auth live in `src/server/context.ts` and `lazy-client.ts`, which only handler bodies import, and the Better Auth route defines its handlers inline. `vite build` then ships no `pg`, Drizzle or Better Auth server code to the client.
- **Settings shows `ID <provider account id>` for GitHub and Google.** Better Auth stores the provider's numeric user id, not the handle the design shows. Showing the handle needs a stored profile field (a later change).
- **`/email-otp/request-email-change` to an address in use answers success and sends nothing**, as found in the spike, so the settings page shows the code step either way; the confirmation is refused.

## 6. Risks

- Hyperdrive under the Vite plugin and the cookie bridge in server functions are the least certain pieces; step 1 tests both before anything depends on them.
- The link-intent check depends on `getOAuthState()` being readable in `account.create.before` during a callback. The source suggests it is; step 1 confirms it, and the fallback hook point is named above.
- The OAuth mock uses `genericOAuth`, while production uses `socialProviders`. They share endpoints and the `mapProfileToUser` override, but real GitHub and Google flows (scopes, real `hd` tokens) stay unverified until credentials exist, and are reported as pending.
- A GitHub or non-Gmail Google user has to confirm their email once with a code. That is the cost of not trusting those providers' email claims.
- Better Auth upgrades can change its tables or endpoint middleware. The versions are pinned; the table-shape test and the freshness and send-limit tests catch the rest.
- Sessions are stored unhashed, as Better Auth does by default; revisit at the P12 security review.
