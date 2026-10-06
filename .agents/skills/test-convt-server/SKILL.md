---
name: test-convt-server
description: Run and verify the convt jobs API, Postgres queue, object storage, sandbox worker and metering locally.
---

# Test the convt cloud service

Read `docs/p9-cloud-plan.md` for the deployment contract. The API serves health, formats, OpenAPI and authenticated create, start, status, download and cancel operations. The worker claims Postgres leases and runs each conversion in a fresh Linux process sandbox by default, with Docker available as an optional backend. Neither service is deployed by these tests.

## Sandbox gate

Build the Railway worker image from the bundled Linux payload, then run the escape and aggregate-resource probes before enabling uploads:

```sh
export PATH=$HOME/.cargo/bin:$HOME/.bun/bin:$PATH CONVT_LICENSE_STORE=file
docker build -f crates/convt-worker/deploy/railway/Dockerfile -t convt-railway-worker:local .
python3 crates/convt-worker/sandbox/process-gate.py
cargo build -p convt-server -p convt-worker
```

The launcher scopes writable cgroups to its own labeled test container. It checks a fresh empty environment, separate UID, absent parent files and `/proc`, denied TCP and outside writes, Landlock, seccomp, capability removal, protected guardian, per-process limits, aggregate CPU and memory, unlinked storage exhaustion, cancellation, wall timeout and supervisor death. It verifies startup refusal when Landlock or seccomp is denied. Ordinary read-only cgroups or denied tmpfs mounts must also refuse startup. The explicit unsafe override reports missing protections; never enable it in production.

The default backend needs root setup operations, Landlock ABI 6, seccomp, writable delegated cgroup v2 controllers and private tmpfs mounts. Railway compatibility remains NOT CHECKED until the actual deployment passes `--sandbox-gate` and `--sandbox-test`. Only then set `CONVT_SANDBOX_VERIFIED=1` on the API. Docker's optional backend uses `sandbox/build.sh` and `sandbox/gate.py`, with `CONVT_SANDBOX_BACKEND=docker` and the intended image/runtime.

## Local storage and launch

Use `scripts/db.sh up` and `bun run db:migrate` for the guarded per-checkout database. Build MinIO with `sandbox/build-minio.sh`, then run `crates/convt-worker/storage-dev.sh up`. The helper labels the checkout, chooses a random port, configures a one-day lifecycle and writes mode-0600 credentials under `.convt-dev`. Do not print that file. The helper's `down` command removes only its labeled container.

Source the storage environment, use `scripts/db.sh url server` for `DATABASE_URL`, and set a random `CONVT_WEB_TOKEN_SECRET` of at least 32 characters. Give the web Worker the same secret and `CONVT_API_URL`. Launch `target/debug/convt-server` and `target/debug/convt-worker` with separate retained PID files and logs. Choose an unused `PORT` and `CONVT_BIND_HOST`; the default API listener is loopback. Follow the remote-preview skill before starting or sharing a Tailscale preview. Bound readiness waits and stop only owned PIDs.

Readiness requires `/health` returning `ok`, a matching migration hash, and worker startup verifying the confined registry supports the advertised formats (or the image ID for the Docker backend). `/openapi.json` is the canonical jobs contract. Upload exactly the declared byte count using the returned PUT URL, whose signature binds Content-Length, then call start; only the sealed, size-checked object enters the queue.

## Automated checks

```sh
cargo test
bun run rs:check
bun run db:ci
```

`db:ci` creates a disposable labeled Postgres, checks migration drift and down files, exercises role grants and integration tests, verifies sqlx offline data and builds without a database. Server database tests must not silently skip:

```sh
bash packages/db/scripts/test-db.sh env CONVT_REQUIRE_DB=1 cargo test -p convt-server
```

Cover cross-account read and download denial, stale lease completion after replacement, concurrent reservations at the cap, signed upload length, staging and sealed size limits, cancelled upload commitments until successful cleanup, concurrent API and Pro storage reservations, expiry fencing, monthly Pro allowance for annual subscriptions, and idempotent meter delivery. Worker tests reject archive links and traversal. New SQL uses checked sqlx macros. After editing queries, regenerate metadata:

```sh
bash packages/db/scripts/test-db.sh bash -c 'cd crates/convt-server && DATABASE_URL="postgresql://convt_owner:$TEST_OWNER_PASSWORD@127.0.0.1:$TEST_PG_PORT/convt_template" cargo sqlx prepare -- --all-targets'
```

## Real artifact checks

Run `crates/convt-worker/examples/replica_acceptance.rs` in a gate-approved image with a disposable database from `packages/db/scripts/test-db.sh`. It starts one actual API and three worker binaries on the same queue, converts real image, SVG, video, PDF and document inputs, checks all three lease owners, attempt fencing, signatures and exactly one usage fact per job, then checks an empty queue. Pass a long MP4 through `CONVT_ACCEPTANCE_CANCEL_FIXTURE` to observe and cancel the actual process tree. Use the optional private SDK exchange for a separate Bun or Node client converting four real types. Never print API or storage credentials.

Run `@convt/sdk` unit tests and inspect downloaded signatures and artifacts. The process gate kills an owned supervisor with SIGKILL and checks the guardian removes the whole group. Terminate owned stack workers with SIGTERM and assert their groups and resource domains are cleaned. Never signal a worker you did not start. Meter tests use a mock and verify `external_id`, customer, quantity, version header and duplicate acknowledgement. Real Polar metering requires provider credentials and an invoice check.

Run `convt-server`'s `bucket_acceptance` example with a bounded timeout against MinIO and separately Railway Buckets. It checks exact upload length, CORS headers, expiry and scoped deletion. A header check does not replace an actual browser upload. Actual Railway kernel and bucket tests remain NOT CHECKED until deployment.

The dashboard browser checks are in test-convt-web. Record logs and screenshots outside the repository, with PASS, FAIL or NOT CHECKED for each requested result. Shut down owned services and containers after temporary verification. Leave the shared Postgres and Mailpit running.
