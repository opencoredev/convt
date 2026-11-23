# Cloud API: deploy, reliability and growth notes

What the `api-hill-climb` branch changes about running the cloud API, what a deploy needs, what is tied to Railway, and the recommendation for a free starter allowance. Nothing here has been deployed.

## Deploy order

1. **Migrate first.** `0009_api_dx` adds nullable columns and two indexes to `cloud_jobs` (idempotency key and request hash, `source_url`, webhook delivery state). convt-server and convt-worker refuse to start until the database has every migration they were built with, so run `bun run db:migrate` against production before the new images roll out. The old server keeps working on the migrated database: it never reads the new columns.
2. **Set `CONVT_WEBHOOK_SECRET`** (at least 32 random characters) on convt-api. Without it the API starts, refuses `webhook_url` with `400 webhooks_unavailable`, and logs a warning. Rotating it changes every account's webhook signing secret at once; there is no per-account rotation yet.
3. **Deploy convt-worker, then convt-api.** A new worker understands `source_url` jobs; a new API is the only thing that creates them. Rolling the API first would queue `source_url` jobs that an old worker fails as a missing upload.
4. Check: `GET /ready` returns `{"status":"ready","database":true,"storage":true}`, `POST /v1/convert` converts a small PNG end to end with a real key, and a job with `webhook_url` delivers to a request bin.

`railway.json` for convt-api now gates deploys on `/ready` instead of `/health`, so a release that cannot reach Postgres or the bucket never takes traffic.

## Reliability on Railway

| Concern                                | Behavior now                                                                                                                                                                                                                                                                                                                                |
| -------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Liveness vs readiness                  | `/health` answers `ok` without touching anything. `/ready` checks Postgres (`select 1`) and storage (`HEAD`) with 3-second timeouts, and answers 503 while draining.                                                                                                                                                                        |
| API deploys and restarts               | SIGTERM stops new connections and gives in-flight requests 25 seconds (`drainingSeconds` is 30). A `POST /v1/convert` still waiting answers `202` with the job id as soon as draining starts, so the client can poll instead of losing the result.                                                                                          |
| Worker deploys and restarts            | On SIGTERM a running conversion gets 20 seconds to finish. If it doesn't, the worker stops it, deletes that attempt's outputs and puts the job back in the queue (`jobs::requeue`) without spending one of its 3 attempts. Before this branch the job failed with `worker_shutdown`, and the customer had to start over after every deploy. |
| Worker crash (SIGKILL, OOM, host loss) | Unchanged and still correct: the 30-second lease lapses, another worker claims the job with a higher `attempt`, and the stale worker's `finish` is fenced off. After 3 lost attempts the job fails with `expired_or_exhausted` and its reservation is released.                                                                             |
| Spend-cap reservations                 | Released by finish, cancel, expiry and the exhaustion sweep, as before. Requeue keeps the reservation open because the job is still going to run. `/v1/convert` without an `Idempotency-Key` cancels its job when the client hangs up, so abandoned requests release theirs at once instead of at expiry.                                   |
| Expiry sweep                           | Ran an `UPDATE` over every expired row every minute, forever, and turned cancelled jobs into `failed`. It now touches only rows that still hold a reservation or an unfinished status.                                                                                                                                                      |
| Webhooks                               | Delivery state is on the job row; a delivery is claimed with `FOR UPDATE SKIP LOCKED` and a 60-second lease, so several API replicas never double-send and a restart loses nothing.                                                                                                                                                         |

## Portability

Nothing in the API or worker depends on Railway APIs. What is Railway-shaped:

- **`PORT` and `CONVT_BIND_HOST`**: Railway injects `PORT`; any platform that does the same works.
- **`railway.json`**: replicas, restart policy, `drainingSeconds` and the healthcheck path. Fly.io (`fly.toml`: `kill_timeout = 30`, an HTTP check on `/ready`) and systemd or Docker on Hetzner (`stop_grace_period: 30s`, a healthcheck on `/ready`) take the same values.
- **Object storage**: plain S3 (`S3_ENDPOINT`, `S3_BUCKET`, `S3_ACCESS_KEY`, `S3_SECRET_KEY`, `S3_REGION`, `S3_PATH_STYLE`). Railway Buckets, R2, Tigris, MinIO and AWS all fit. Lifecycle cleanup is also done by the API (`routes::expire`), so a bucket without lifecycle rules still empties.
- **Postgres**: any Postgres 15+ with the roles from `packages/db/roles.sql`.
- **The worker sandbox** is the real constraint. The process backend needs root for setup, Landlock ABI 6, seccomp, writable delegated cgroup v2 controllers and private tmpfs mounts (`.agents/skills/test-convt-server/SKILL.md`). A Hetzner VM or a dedicated host gives all of that. Fly Machines are Firecracker VMs, so the gate should pass but must be run (`convt-worker --sandbox-gate` and `--sandbox-test`) before `CONVT_SANDBOX_VERIFIED=1` is set on the API. Do not move the worker to a platform where the gate fails.
- **Outbound network**: the worker now fetches `source_url` inputs and the API sends webhooks. Both go through `net_guard`, which refuses private, loopback, link-local, CGNAT, unique-local IPv6 and metadata addresses and pins the address it checked. If a platform routes egress through an HTTP proxy, note that the guard disables proxies on purpose.

## Free starter allowance: recommendation

Today a developer must subscribe to API billing in Polar and have a card seen (`subscriptions.card_seen_at`) before `createApiKey` lets them make a key, and `jobs::create` refuses API jobs without that subscription. That is the biggest drop-off between reading `/api` and a first successful call.

**Recommendation: 25 free conversions per account, lifetime, no card, `POST /v1/convert` only.** Changes needed, none of them in Polar:

1. **Database**: a `usage_events.kind` of `api_trial` (extend its check constraint), counted per user; or a `users.api_trial_remaining smallint default 25`. The usage-event route keeps the audit trail append-only and needs no new table.
2. **convt-server `jobs::create`**: when an API key's owner has no usable API subscription, allow the job if their `api_trial` count plus open trial reservations is below 25. Reserve 0 cents and record `api_trial` on success instead of `api_conversion`, so `meter::drain` never bills it. Refuse `source_url`, `webhook_url` and the 2 GB jobs routes for trial jobs, so the allowance can't be used for heavy video.
3. **`createApiKey`**: allow one key without enrollment; keep the 20-key limit for enrolled accounts.
4. **Dashboard**: show "N of 25 free conversions left" and the enrollment card when it reaches zero; the error for job 26 is `403 trial_exhausted` with a link to add a card.
5. **Abuse guard**: sign-in already needs a verified email or OAuth. Add a global daily ceiling on trial conversions (an env var, say 2,000 a day) so a signup farm can only burn a bounded amount of worker time, and alert when it trips.

Cost: 25 conversions of files under 25 MB is a few cents of worker time per account. Polar needs no product change. Leo's approval is needed because it is a pricing decision and adds a free tier, which AGENTS.md says not to add unasked.
