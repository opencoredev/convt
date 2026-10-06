# Local launch load test

Run `bun run test:load` from the repository root after starting the real web, billing, API, sandbox worker, Postgres and MinIO services. Follow `.agents/skills/test-convt-web/SKILL.md` and `test-convt-server/SKILL.md` for the sandbox gate, free ports, readiness and owned-process cleanup. It checks the container ID, checkout label and in-database owner marker with the existing database guard before connecting. This tool refuses remote targets and an owner URL different from `.convt-dev/services.env`. Never run it against production.

```sh
export WEB_URL=http://localhost:<owned-web-port>
export API_URL=http://127.0.0.1:<owned-api-port>
export LOAD_DATABASE_URL=$(bash scripts/db.sh url owner)
CONVT_LOAD_LOCAL=1 LOAD_CONCURRENCY=25 bun run test:load
```

Use 20 to 50 concurrent API keys. The default is 25. There are 300 page requests, 40 email-code requests, 50 device token requests, one full SVG-to-PNG job per key, a 30-request burst against a three-cent subscription, and 100 signed replays of a genuine local paid order. The quota belongs to the account subscription, rather than an individual key. Accepted quota jobs finish and a further create must still be refused. The device endpoint is a PKCE token exchange; convt does not implement an OAuth device-code polling grant. Repeated unknown-code requests exercise its database path and rate limit.

The billing mock does not implement Polar event ingestion. Start `bun tools/loadtest/src/meter.ts`, retain its PID and printed loopback origin, and start your owned worker with `POLAR_API_URL` set to that origin and a local-only `POLAR_ACCESS_TOKEN`. Set `LOAD_METER_URL` to the same origin to verify delivery quantities, customer IDs, stable job IDs and reported usage. This sink retains only one event per external ID. It does not certify Polar billing or invoices. Stop it by its retained PID after testing.

The test warms the three public routes before measuring. It follows the pricing redirect and consumes each response body. Output reports nearest-rank p50, p95 and p99 in milliseconds, status counts, unexpected-response error rates, 5xx counts, peak database connections, and SQL accounting. Expected 400, 403 and 429 responses appear separately from unexpected errors. Conversion latency includes upload, queue wait, polling and download; the two-second polling interval adds observation delay. HTTP timeouts are 30 seconds and job waits are bounded to three minutes.

Each run uses unique fixture accounts and documentation-range synthetic client IPs in the local Worker's trusted Cloudflare header, leaving existing rate-limit buckets intact. The email-cap burst uses a different IP per request so the IP cap cannot mask it; the IP-cap burst uses one fresh IP and a different address per request. SQL checks `usage_events` and the `cloud_jobs.reservation` field; there is no separate reservations table. It checks settled charges, duplicate facts, quantities, open commitments and lease/attempt anomalies. One worker processes jobs sequentially, so concurrent clients measure queue buildup as well as HTTP contention. This run does not force lease replacement or a worker crash; database regressions and the server skill cover those separately.

The tool revokes its keys, cancels its subscriptions and expires its jobs in `finally`. Leave the API running for its next minute-long cleanup tick, then check that those expired jobs have no `input_key` and no open reservation. Accounts and append-only financial facts remain as local audit evidence. The paid Desktop order and its licence also remain. No existing accounts or usage facts are deleted. Local Mailpit receives test emails; no real mail or payment service is used.

Save stdout and service logs outside the checkout. Search API, worker and web logs for connection exhaustion, database failures, lost leases and failed conversions. Run the five root verification commands after edits. These short local development measurements do not predict Cloudflare or Railway capacity, sustained throughput, large media conversion costs or production object-storage enforcement.
