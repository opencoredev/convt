# Run the P9 cloud service

P9 is implemented locally and is not deployed. Leo selected Railway for the API, worker replicas, Postgres and Buckets. The worker uses a Linux process sandbox by default, with Docker available as an optional backend. Production acceptance remains open: the process sandbox needs kernel features and delegated resources that a Railway deployment has not yet demonstrated. The desktop consent flow is outside this implementation.

## Deploy to Railway

Create separate API and worker services, using the repository root as each build context. The API can build from Git. The worker requires ignored native payloads, so first stage the audited `packaging/out/convt` payload locally, build the image and publish it to a private OCI registry, then deploy that image on Railway. A plain Git checkout cannot build the worker image. The worker config is also available for an artifact-staged source upload. Select these config files for source builds in Railway:

| Service | Config                                            | Dockerfile                                      |
| ------- | ------------------------------------------------- | ----------------------------------------------- |
| API     | `crates/convt-server/deploy/railway/railway.json` | `crates/convt-server/deploy/railway/Dockerfile` |
| Worker  | `crates/convt-worker/deploy/railway/railway.json` | `crates/convt-worker/deploy/railway/Dockerfile` |

The API listens on Railway's `PORT` at `0.0.0.0` and has a `/health` check. The worker config starts three replicas. Each replica handles one job at a time; increase replicas to add conversion capacity. Postgres leases and attempt fencing coordinate the replicas. Allocate enough memory for each replica's parent and a 4 GB job, plus space for one 2 GB input and output collection. Set draining time long enough for forced process cleanup.

The worker image includes bundled Linux x86_64 FFmpeg, FFprobe, PDFium and libheif, plus system LibreOffice and fonts. Build it only after `packaging/out/convt` exists. The Dockerfile-specific ignore files exclude development credentials, caches and unrelated packaging outputs. No Docker daemon or socket is needed by the process backend.

Apply the additive migrations with the owner role. Give the web Worker `convt_web` credentials and both API and workers `convt_server` credentials. Startup checks migration hashes. Never use the owner connection for a service. Connect the web Worker to Railway Postgres through Hyperdrive as described in P6.

## Pass the worker host gate

The parent runs as root to prepare a private chroot, assign a separate never-reused UID and drop the child's groups and capabilities. It alone reads database and storage credentials. Every conversion starts in a fresh child with an empty environment and closed inherited descriptors. The private root contains its input, trusted engine binaries and libraries, selected configuration and a writable output directory. It has no `/proc`, parent files or service secrets. Scratch is inside the output directory.

Landlock ABI 6 or newer restricts reads to the input and trusted runtime, writes to output, TCP connections, abstract Unix sockets and signals across domains. Seccomp blocks network sockets except AF_UNIX, process-group escape, namespaces and privilege operations. The child sets no-new-privileges, drops all capabilities and uses CPU, address-space, file-size, process-count and descriptor rlimits. FFmpeg and FFprobe also enforce local protocols and reject network manifest demuxers. A protected guardian watches the supervisor and kills the whole process group on parent death or the 600-second deadline. Cancellation and lease loss kill and reap the group before output publication or filesystem cleanup.

Per-process rlimits do not bound a process tree's combined consumption. Each job therefore also requires its own cgroup v2 with two CPUs, 4 GB aggregate memory, zero swap and 128 processes. Output is a private 4 GB tmpfs mounted with `nodev,nosuid,noexec`; this bounds even unlinked open files. Collection accepts at most 256 regular files, at most 2 GB each and 4 GB in total.

The production worker host must allow all of the following:

- Root parent operations: chroot, setuid, setgid, clearing groups and creating accessible null/random character devices.
- Landlock ABI 6 or newer, seccomp filters and no-new-privileges.
- A writable delegated cgroup v2 subtree containing only this service, with CPU, memory and PID controllers. Set `CONVT_SANDBOX_CGROUP_ROOT` only to that scope. The worker refuses ambiguous host roots.
- Private tmpfs mounts and unmounts, including the required mount capability and a compatible host security policy.

A Dockerfile cannot grant those host permissions. Railway kernel support, cgroup delegation and mount permissions are **NOT CHECKED** until a real deployment passes the gate. Ordinary restricted Docker cannot satisfy the resource gate. Do not enable uploads or use the unsafe override to make production start. If Railway does not provide these requirements, the selected worker deployment is blocked; resolve hosting with Leo rather than weaken the sandbox.

Startup performs actual isolation and resource probes and refuses to start when protections are unavailable. `CONVT_SANDBOX_ALLOW_UNSAFE=1` is a diagnostic override and reports missing protections explicitly. Leave it unset in production. Run the production binary's `--sandbox-gate` and `--sandbox-test` on the deployed worker before setting `CONVT_SANDBOX_VERIFIED=1` on the API. Rerun after kernel, host, payload or policy changes. Worker startup also verifies its confined format registry supports the advertised cloud targets.

Local image acceptance uses a container with no nested daemon. Its test launcher binds only that container's cgroup subtree, never the host root:

```sh
export PATH=$HOME/.cargo/bin:$HOME/.bun/bin:$PATH CONVT_LICENSE_STORE=file
docker build -f crates/convt-worker/deploy/railway/Dockerfile -t convt-railway-worker:local .
python3 crates/convt-worker/sandbox/process-gate.py
cargo build -p convt-server -p convt-worker
```

The local gate checks parent environment and filesystem reads, `/proc/1/environ`, TCP connections, writes outside output, fork exhaustion, per-process and aggregate limits, unlinked storage exhaustion, cancellation, wall time and supervisor death. A local pass proves that configuration, not Railway compatibility or immunity to kernel vulnerabilities.

For a host that can launch containers, `CONVT_SANDBOX_BACKEND=docker` retains the earlier backend. Build it with `sandbox/build.sh`, run `sandbox/gate.py`, and configure `CONVT_SANDBOX_IMAGE` and optional `CONVT_SANDBOX_RUNTIME`. Its labeled attempt reaper handles orphan containers. This is optional and is not the Railway default.

## Configure Railway Buckets

Create a private Railway Bucket and give only the API and worker its credentials. Set `S3_ENDPOINT`, `S3_REGION`, `S3_BUCKET`, `S3_ACCESS_KEY` and `S3_SECRET_KEY` to the provider values. `S3_PATH_STYLE` defaults to `false`, using a virtual-hosted bucket hostname. Set it to `true` for local MinIO or an endpoint that requires path addressing. Do not guess the region or endpoint.

Apply CORS for the actual web origin using `crates/convt-worker/deploy/railway/bucket-cors.json` as the template. It allows browser PUT, GET and HEAD with content headers and exposes ETag and content length. Keep the bucket private. Upload signatures bind the exact Content-Length that browsers derive from the File or Blob. Start validates staging size, copies to an immutable sealed key and validates the sealed size before queueing. Reusing the PUT URL cannot change the queued input.

Upload URLs last 15 minutes. Download URLs last at most five minutes and never beyond job expiry. Jobs expire after 24 hours. The API invalidates expired leases and deletes expired object prefixes every minute; physical removal depends on successful cleanup. Configure a one-day lifecycle if Buckets supports it, but lifecycle support is not assumed. Monitor cleanup failures and retained prefixes.

With credentials loaded privately, run the provider acceptance test with a bounded deadline:

```sh
CONVT_ACCEPTANCE_ORIGIN=https://convt.app timeout 180s cargo run -p convt-server --example bucket_acceptance
```

It checks exact-length PUT, rejection of short and oversized PUT, CORS headers, GET bytes, expired PUT and GET signatures, and scoped prefix deletion. MinIO passed these checks. Actual Railway Buckets and browser CORS remain **NOT CHECKED** until this test and the dashboard upload/download flow run against the real bucket.

## Configure credentials and billing

Set `DATABASE_URL` and the storage variables on API and worker. Set a random `CONVT_WEB_TOKEN_SECRET` of at least 32 characters on the API and web Worker, and `CONVT_API_URL=https://api.convt.app` on the web Worker. The Railway hostname `https://convt-api-production.up.railway.app` remains an operational fallback. It issues five-minute account tokens after checking a verified session and paid Pro coverage. The API needs `CONVT_BIND_HOST=0.0.0.0`, Railway's `PORT`, and the gate flag only after production acceptance. The worker defaults to `CONVT_SANDBOX_BACKEND=process`; its trusted template is `/srv/convt-template` and private jobs are `/srv/convt-jobs`.

Create the API product and metered price in Polar as described in P7. The current price is one cent per successful conversion and needs confirmation before launch. Set an `events:write` scoped `POLAR_ACCESS_TOKEN` on the worker. The sender emits `api_conversion`, quantity 1, external customer ID equal to the account ID, and `external_id` equal to the immutable job ID. Duplicate acknowledgements do not charge again. A mock checked shape and retries; real ingestion, card enrollment and invoices remain separate provider checks.

Configure API DNS and TLS, then verify authentication, CORS, uploads and downloads against the real web origin. Publish `@convt/sdk` before advertising npm installation. No production resources, DNS or billing settings were changed by this implementation.

## Run locally

Use the guarded per-checkout Postgres and MinIO helpers:

```sh
bash scripts/db.sh up
bun run db:migrate
bash crates/convt-worker/sandbox/build-minio.sh
bash crates/convt-worker/storage-dev.sh up
```

The storage helper labels the checkout, chooses a random port and stores credentials in a gitignored mode-0600 file. Source `.convt-dev/p9-storage.env` without printing it. Use `scripts/db.sh url server` for the API and worker database connection. Bound readiness waits and retain only your own PID and container IDs. `storage-dev.sh down` removes only this checkout's MinIO. Leave shared Postgres and Mailpit running.

Use `crates/convt-worker/examples/replica_acceptance/` inside a gate-approved image with `packages/db/scripts/test-db.sh` for a disposable database. It starts an actual API and three worker processes, converts real image, SVG, video, PDF and document files, checks all three lease owners, attempts, output signatures and usage facts, and removes only its own processes, database and object prefixes. Set `CONVT_ACCEPTANCE_CANCEL_FIXTURE` to a long MP4 to test API cancellation against observed process identities. The optional private `CONVT_ACCEPTANCE_SDK_EXCHANGE` lets an external SDK client convert four types while the stack runs.

For browser checks, follow the remote-preview workflow and use `agent-browser` session `convt-p9` with profile `~/.agent-browser/profiles/convt-p9`. Configure the API URL, shared token secret and reachable storage endpoint in the local web Worker. The dashboard converter is `/dashboard/api/convert`; keys, usage and spend cap are on `/dashboard/api`.

The canonical contract is `/openapi.json`; the web copy is `apps/web/src/generated/openapi.json`. Regenerate it with `bun packages/sdk/scripts/generate-openapi.ts`. SDK types come from `convt formats --json`; cloud target manifests come from the actual sandbox registry.

## Accounting and recovery

Job creation locks the subscription row and reserves against settled usage plus every open commitment. API enrollment requires active status and card evidence. Paid Pro includes 50 GB per UTC calendar month, including yearly subscriptions, and a 2 GB maximum input. Trial and lapsed subscriptions cannot create jobs. Start seals the upload to a unique key and validates the sealed size before queueing, so a reusable PUT URL cannot change queued input.

Workers claim with `FOR UPDATE SKIP LOCKED`, renew a 30-second lease every five seconds and use the attempt number as a fencing token. Attempts have separate output keys. Completion requires the current live lease and unexpired job. Success, reservation settlement and append-only usage insertion share a transaction; failures and cancellation release conversion reservations. A separate account storage budget allows at most 100 job prefixes and 50 GB of declared inputs. API and Pro creates share an account lock. Cancelled and expired inputs count until their object prefixes are successfully deleted, so retained PUT URLs cannot bypass this budget. Usage is attributed to job creation, so an old reservation does not charge a new period. Open commitments remain counted across rollover. Expiry invalidates the job and its lease before object deletion.

Monitor queue age, repeated attempts, cleanup failures, disk space and unreported usage. Meter delivery is at least once; provider deduplication uses the stable job ID. A provider timeout leaves the usage row unreported for retry.
