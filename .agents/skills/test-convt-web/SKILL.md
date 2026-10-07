---
name: test-convt-web
description: Run and verify the convt.app website (apps/web, TanStack Start on Cloudflare Workers) and its billing Worker (apps/billing) locally, with Postgres, Mailpit, the OAuth mock and the Polar and Resend mock, signed in as a fixture account, in a browser at desktop and mobile widths. Use when changing anything under apps/web, apps/billing, packages/db, packages/billing, packages/mail, packages/license, tools/oauth-mock or tools/billing-mock, or when asked to preview, screenshot or check the site.
---

# Test the convt website

`apps/web` is a TanStack Start app built with Vite and the Cloudflare plugin, deployed with Wrangler as the `convt-web` Worker. `apps/billing` is the `convt-billing` Worker: the Polar webhook route, the billing crons, and the `BillingRpc` entrypoint the site calls through its `BILLING` service binding. It holds every billing secret and the license signing key; the site holds none. The landing page is static. The account pages (`/sign-in`, `/dashboard`, `/dashboard/licenses`, `/dashboard/billing`, `/dashboard/api`, `/account`) use Better Auth and read Postgres through the `HYPERDRIVE` binding. Locally everything runs on this machine: a Postgres and a Mailpit container per checkout, an OAuth mock that stands in for GitHub and Google, and a billing mock (`tools/billing-mock`) that speaks the parts of Polar's API and Resend's `/emails` that convt-billing calls, serves Polar-like checkout and portal pages, and signs webhooks with Polar's real scheme. No real email, OAuth app, payment account or database is ever used, and convt-billing signs keys with the local dev key in `.convt-dev/license.key`.

## Doctor

```sh
docker info --format '{{.ServerVersion}}'     # Docker must run; no sudo needed
bash scripts/db.sh status                      # this checkout's containers and ports
ss -ltnp 'sport = :3000'                       # who owns the default web port
```

`scripts/db.sh` names its containers `convt-pg-<hash>` and `convt-mail-<hash>` after a hash of the checkout path and labels them `convt.checkout=<path>`, so other checkouts and other projects' Postgres containers are never touched. Ports are random on 127.0.0.1; `.convt-dev/services.env` (mode 0600, gitignored) records them with generated passwords and `BETTER_AUTH_SECRET`.

## Launch

```sh
work=$(mktemp -d /tmp/convt-web.XXXXXX)
setsid bun run dev:web >"$work/dev.log" 2>&1 </dev/null &
sleep 1; ps -o pgid= -p $! | tr -d ' ' >"$work/pgid"
for _ in $(seq 90); do grep -q 'dev-web: http' "$work/dev.log" && break; sleep 1; done
origin=$(grep -o 'dev-web: http://localhost:[0-9]*' "$work/dev.log" | cut -d' ' -f2)
curl -fsS "$origin/api/auth/ok"                # {"ok":true}
bun run db:seed                                # fixture accounts; safe to repeat
```

`bun run dev:web` runs `scripts/dev-web.sh`: `db.sh up`, migrations, the OAuth mock on `OAUTH_MOCK_PORT`, the billing mock on `BILLING_MOCK_PORT` (delivering webhooks to the site's `/webhooks/polar`), `apps/web/.dev.vars` and `apps/billing/.dev.vars` from `services.env`, then Vite on 3000 or the next free port with `BETTER_AUTH_URL` set to match. convt-billing runs inside Vite as an auxiliary Worker; outside production the site forwards `/webhooks/*` and `/__billing/*` to it, because an auxiliary Worker has no port of its own. After seeding, mirror the fixtures into the billing mock with `bun tools/billing-mock/src/preload.ts` (it is in-memory, so repeat it after every restart). Readiness is `/api/auth/ok` returning 200. Server-side errors appear in `dev.log`, not the browser console. `setsid` puts Vite and the mock in one process group so cleanup can stop exactly those.

For a production-like check, build and serve the Worker with Wrangler. Pass the database through the Hyperdrive variable and point `BETTER_AUTH_URL` at Wrangler's port:

```sh
set -a; . .convt-dev/services.env; set +a
bun run build
cd apps/web && CLOUDFLARE_HYPERDRIVE_LOCAL_CONNECTION_STRING_HYPERDRIVE=$DATABASE_URL \
  setsid bunx wrangler dev --port 8788 --ip 127.0.0.1 --var BETTER_AUTH_URL:http://localhost:8788 &
```

It reads the other values from `.dev.vars`, which the build copies into `dist/server`. Never run `bun run deploy` or `wrangler deploy` without the user's explicit authorization.

## Sign in as a fixture

All fixtures are on the reserved `.test` domain and exist only in local databases:

| Email                   | Shows                                                                                |
| ----------------------- | ------------------------------------------------------------------------------------ |
| `new@convt.test`        | Signed in, bought nothing.                                                           |
| `trial@convt.test`      | Pro monthly trial ending in 3 days, trial key.                                       |
| `desktop@convt.test`    | Desktop order, key, invoice and one Mac.                                             |
| `pro@convt.test`        | Pro yearly, older Desktop key, two Macs, API with two keys and usage, GitHub linked. |
| `lapsed@convt.test`     | Pro ended last month, last key, a failed invoice.                                    |
| `api@convt.test`        | API only: one key, usage, six failed jobs.                                           |
| `unclaimed@convt.test`  | No account: a Desktop purchase that signing up with this address claims.             |
| `refunded@convt.test`   | A Desktop purchase refunded in full: the key shows REFUNDED.                         |
| `pastdue@convt.test`    | Pro monthly whose renewal failed: past due, last month's key.                        |
| `disputed@convt.test`   | A Desktop purchase with a lost chargeback: the key shows DISPUTED.                   |
| `apipending@convt.test` | API enrollment waiting for a card (pending).                                         |

`trial@` has no key (a trial is not paid coverage); a database seeded before P7 keeps its old trial key, because licenses are never deleted. Every subscription fixture has payment coverage and a `billing_customers` row with mock ids.

Sign in at `/sign-in` with the email, then read the code from Mailpit (its web UI is `$MAILPIT_URL`):

```sh
curl -fsS "$MAILPIT_URL/api/v1/search?query=to:pro@convt.test" \
  | python3 -c 'import json,re,sys; print(re.search(r"\d{6}", json.load(sys.stdin)["messages"][0]["Subject"]).group())'
```

Each address may receive 3 codes per 15 minutes and each IP 10 per hour; every local request comes from the same IP. To clear the limits in this checkout's own database: `bash scripts/db.sh psql owner -c 'delete from otp_send_limits; delete from rate_limits'`.

GitHub and Google buttons go to the OAuth mock's authorize page, which lists its identities: `google-gmail` and `google-workspace` (verified addresses), `google-thirdparty` and `google-unverified` (must confirm by code), `github-verified`, `github-public-differs`, `github-no-email`, `github-pro` (signs in to `pro@convt.test`), and `error` (access denied). Append `&identity=<name>` to the authorize URL to skip the page and `&email=<address>` to sign up with another address.

## Drive and evidence

Use headless `agent-browser` for UI verification, with a named session and its own persistent profile. Put both `--session` and `--profile` on every invocation. Never use a personal browser profile. UI work includes opening the real page and inspecting desktop and mobile screenshots in both themes.

The repeatable checks are scripts in `apps/web/e2e`, each printing PASS or FAIL lines and saving screenshots:

```sh
cd apps/web
E2E_SESSION=convt-web E2E_SHOTS=$work/shots bash e2e/sign-in.sh   # code, emailed link, OAuth identities, verify-email
E2E_SESSION=convt-web E2E_SHOTS=$work/shots bash e2e/fixtures.sh  # every fixture, 5 pages, 1280 and 390 px, light and dark
E2E_SESSION=convt-web E2E_SHOTS=$work/shots bash e2e/account.sh   # activate, copy key, rename, change email, connect, sign out
E2E_SESSION=convt-web E2E_SHOTS=$work/shots bash e2e/billing.sh   # checkout, keys, trial, switch, refund, API, emails, deletion
```

They read the site origin from `apps/web/.dev.vars`; set `E2E_URL` to test Wrangler instead. `billing.sh` drives the mock's hosted checkout and its `/admin/*` endpoints (end a trial, renew, fail a card, refund, change settings), runs crons through `$E2E_URL/__billing/scheduled?cron=...`, and screenshots each billing screen and the four emails (Mailpit's `/view/<id>.html`) in both themes. Run it once per fresh `dev:web`: it changes mock settings and deletes an account. `account.sh` calls a source module directly to prove `getLicenseKey` refuses another user's license, which only works under Vite. A layout change needs desktop and 390 px screenshots in both themes; look at each one with the Read tool.

Activate opens `convt://activate?key=<token>` through a temporary link. `account.sh` stubs `HTMLAnchorElement.prototype.click` to check the URL, because Chrome lets no page script stub `location`. Opening the link in the real desktop app is a GUI check (see test-convt-desktop): register the handler with `integrations/linux/install.py` and use `CONVT_LICENSE_STORE=file` with `HOME` and the XDG directories in a `mktemp` directory. Seeded keys are signed with `.convt-dev/license.key`, whose public half local builds embed.

## Desktop sign-in

`/device` is the page the desktop app opens to sign in (P8). It needs a signed-in, verified session, so a signed-out visit goes through `/sign-in` and comes back. Approve stores a five-minute one-time code in `verifications` and opens `convt://auth?state=...&code=...`; Cancel opens `...&error=access_denied`. The app then calls `POST /api/device/token` (code and verifier), `POST /api/device/license` (bearer device token; returns the Pro key from convt-billing's `currentProKey`) and `POST /api/device/sign-out`. These routes use no cookies and skip the Origin check; the logic and limits are in `src/server/device-auth.ts`. `test/integration/device.test.ts` covers the flow, one-use codes, revocation, cross-account isolation and rate limits. A full run with the real app is in `test-convt-desktop`, "Sign-in and renewal". Signed-in devices appear under Macs on the dashboard, where Sign out revokes their token.

## Billing locally

- The mock's admin API: `curl -X POST $BILLING_MOCK_URL/admin/<name> -d '{...}'` with `state` (GET), `clock` (`advanceDays`), `trial-end`, `renew`, `card` (`decline`), `retry-payment`, `refund` (`order_id`, `amount`), `dispute` (`open`, then `lost`/`won`/`prevented`), `settings` (`allow_multiple_subscriptions`), `product` (price drift), `webhooks` (`mode` auto or hold, `dropNext`, `duplicate`, `delayMs`, `reorder`, `forgeNext`, `scheme` standard or legacy, `url`), `flush`, `resend-fault` (`timeout`, `delay`, `5xx`), `complete-checkout`. Polar sends no dispute webhook; disputes arrive through the reconciler (`cron=*/15 * * * *`).
- Crons: `curl "$origin/__billing/scheduled?cron=*+*+*+*+*"` drains the outbox and advances deletions; `*%2F15+*+*+*+*` runs the reconciler; `17+3+*+*+*` the daily check and digest (to `alerts@convt.test` in Mailpit).
- `bun run billing:outbox list` shows ambiguous and dead emails; `resolve <id> sent|resend` records Leo's decision.
- The second dev mode is `cd apps/billing && CLOUDFLARE_HYPERDRIVE_LOCAL_CONNECTION_STRING_HYPERDRIVE_BILLING=$BILLING_DATABASE_URL bunx wrangler dev --port 8790 --test-scheduled`, with the mock's webhook URL set to it (`admin/webhooks` `url`). The site under `wrangler dev` reaches it through the dev registry; on a machine short of file watches (EMFILE in the log) that registry never finishes starting.
- Billing Worker settings and guards: `packages/billing/src/env.ts`. In production it refuses loopback provider or mail URLs and a signing key that does not match `LICENSE_PUBLIC_KEY` or is a dev key.

## Tests

```sh
cd apps/web && bun test test/unit                              # views, redirects, Origin, redactor, env, table shape
bun run --cwd apps/web test:integration                         # Better Auth in process against a throwaway Postgres
bun run db:ci                                                   # schema drift, down files, all integration tests, convt-server
cd packages/billing && bun test test/unit && bun run test:integration   # verifier, catalog, guards; ingest, keys, outbox, reconciler, deletion
cd tools/billing-mock && bun test                               # the mock through the real SDK client, validateEvent, Resend idempotency
cd packages/mail && bun test                                    # template snapshots and escaping, Resend outcomes
```

The billing integration tests run the mock in process through the real Polar SDK client, as `convt_billing` against a disposable database, with an injected clock; one test builds `convt-cli` and activates an issued key with `CONVT_LICENSE_STORE=file` in a temporary HOME. A change to an email template changes its snapshot: bump `templateVersion` in `packages/mail/src/templates.ts` with it.

Integration tests and `db:ci` start their own labeled tmpfs Postgres and remove it by id on exit.

## Preview for another device

Secure cookies are off in development, but the sign-in origin must be the one the device uses. Start with `PUBLIC_ORIGIN=http://<tailnet-host>:<port> PORT=<port> MOCK_HOST=0.0.0.0 MOCK_PUBLIC_URL=http://<tailnet-host>:<mock-port> bun run dev:web`, which binds Vite to all interfaces and sets `BETTER_AUTH_URL` and the mock's authorize URL to those addresses. This path was not exercised when P6 was verified, so check sign-in end to end the first time. Verify the URL from the device that will open it, and report which devices actually loaded the page. Read the remote-preview skill before starting or sharing a preview. Bind to the verified Tailscale address and report which devices actually loaded the page.

## Checks

```sh
bun run check && bun run check-types && bun run build
```

## Cleanup

`kill -- -"$(cat "$work/pgid")"` stops Vite and the OAuth mock you started (only that process group). Close browser sessions with `agent-browser --session <name> close`. The containers stay for reuse; `bun run db:down` removes this checkout's containers and its database volume (after the ownership guard), and `bash scripts/db.sh prune` removes those of checkouts that no longer exist.

## P9 cloud converter and API keys

Read test-convt-server and `docs/p9-cloud-plan.md` to start the API, MinIO and sandbox worker. Configure `CONVT_API_URL` and the shared `CONVT_WEB_TOKEN_SECRET` in the local web Worker. Use the task session `convt-p9` and profile `~/.agent-browser/profiles/convt-p9`, on every command.

Check `/dashboard/api`: create a named key, inspect the shown-once field, dismiss it, reload, revoke it and verify a revoked key fails authentication. Inspect spend, reservations, cap reached and not-enrolled states. Check `/dashboard/cloud`: pick a real file, choose a supported target, observe upload and job progress, download and inspect the result. The Cloud tab is not gated on paid Pro — trial and paid Pro both convert; do not expect a 50 GB meter. Test malformed input, cancellation, an unavailable cancel endpoint and the retry button, the 2 GB file guard, unpaid and lapsed states. Temporary reservations used to reach a limit must be cancelled afterward; restore any temporary cap only if it still has the value the test set. Never rewrite settled usage to stage a screenshot.

Capture desktop and 390 px screenshots in light and dark, inspect each image, and verify no horizontal overflow. Check keyboard focus and accessible names. Save P9 evidence in `/home/leo/projects/convt-ui-brief/screens/p9/`. If Vite hits EMFILE, restart only the owned process with `CHOKIDAR_USEPOLLING=1`. Hot reload can reset a selected file; finish source edits before the final browser run. Production Worker, Railway Buckets CORS and real payment enrollment remain separate launch checks.

## Billing security regressions

Run `bun test apps/billing/test` to verify the public webhook body's streaming reader. A body without Content-Length must stop and cancel when it crosses 256 KiB, exact-limit raw bytes must survive unchanged, and non-POST requests must consume no body. The billing integration deletion case includes Pro usage and pending API usage: only the API fact for the subscription being revoked may delay deletion. `bun audit` and the database dependency test must reject vulnerable esbuild versions; `bun run db:ci` checks the overridden loader against real migration tooling.
