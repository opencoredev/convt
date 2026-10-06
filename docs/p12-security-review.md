# P12 launch security review

Reviewed on 2026-10-05 for the Wednesday, 2026-10-07 launch. This review covers the local working tree, including its uncommitted implementation. It does not certify a production deployment. CLAUDE.md and docs/plan.md supplied the security contract. No implementation files were edited.

Five findings were verified: four medium and one low. No critical or high finding was verified. Resolve the medium findings before launch, and complete the production checks below before treating the cloud service as ready.

## Findings

### Medium: presigned uploads do not enforce the reserved size

Location: `crates/convt-server/src/storage.rs:45`, with the late check at `crates/convt-server/src/routes.rs:190`.

The PUT signature has no custom headers or size policy. Its payload is unsigned in rust-s3, and the reservation's byte count is not supplied to storage. The API checks the actual size only after copying the uploaded object into a sealed key. That protects conversion execution, but does not bound the upload itself.

An enrolled user can reserve one byte, receive the 900-second PUT URL, and upload an object much larger than the reservation, up to the storage provider's own limit. They can omit start entirely, or cancel to release the reservation and repeat. Cancellation does not revoke an already issued storage signature. These objects consume storage outside the Pro byte allowance and API spend cap until cleanup. Starting an oversized upload also makes a copy before rejecting it. This conclusion follows from the signing implementation and the create, start, cancel and expiry paths; no large upload was sent to a real bucket.

Suggested fix: enforce the exact reserved length at the upload boundary, using a signed Content-Length where the supported clients and provider can enforce it, or a streaming upload gateway with a hard byte limit. Keep the sealed-object check as a second check. Add bounded outstanding upload commitments and account for cancelled URLs that remain usable until expiry.

### Medium: local media can trigger HTTP requests during conversion and thumbnail generation

Location: `crates/convt-app/src/thumbs.rs:211` and `crates/convt-engines/src/ffmpeg.rs:41`.

Neither invocation restricts FFmpeg's input protocols or demuxers. Prefixing the thumbnail input with `file:` restricts the top-level path, but does not restrict URLs referenced inside the file. The extension-based format table does not ensure that the contents are actually that format.

A crafted DASH document presented as a local MP4 can reference an attacker endpoint or a service on the user's loopback or private network. The app attempts thumbnails when it displays local video files, so a conversion approval is not needed for that path. This breaks the offline-file boundary and permits attacker-selected GET requests from the user's machine.

Verification used a DASH document held in a Linux memfd and an owned loopback HTTP listener. FFprobe with the engine's duration arguments requested `/probe.mp4`. FFmpeg with the thumbnail's `file:` prefix, seek, scale and image2pipe arguments requested it again. Both eventually rejected the media, after issuing the requests. The tested tools were `/usr/bin/ffmpeg` and FFprobe, Ubuntu FFmpeg 6.1.1, which the supported system-tool fallback can select. The packaged static build was not tested for this exploit, and successful response extraction was not demonstrated. The cloud container's network isolation blocks this network path on the tested host.

Suggested fix: apply an explicit local-only protocol policy to both FFmpeg and FFprobe, including thumbnail generation, and reject playlist demuxers for the supported local media formats. Verify that both nested network references and nested local-file references are refused. Preserve ordinary conversion support with regression fixtures.

### Medium: Pro usage permanently blocks deletion of an account with API billing

Location: `packages/billing/src/deletion.ts:97`; the metering selection is at `crates/convt-server/src/meter.rs:41`.

Before revoking an API subscription, deletion waits for every usage row belonging to the user whose reported_at is null. Successful Pro conversions create pro_bytes rows with reported_at null. The metering sender selects only api_conversion rows, so it never clears that condition for Pro usage.

A user with both subscriptions who has completed one Pro cloud conversion cannot delete the account through this workflow. Retries continue to fail, retaining the account and its personal data. The API subscription cannot be revoked by this deletion path either.

Verification used the existing billing harness and a disposable Postgres. After creating Pro and API subscriptions and inserting one normal pro_bytes usage fact, two calls to the real advanceDeletion returned failed. The stored error was `waiting for API usage to be reported`. The Pro usage remained unreported, and the metering selection returned no eligible rows. The existing deletion tests pass because their combined-subscription case has no Pro usage.

Suggested fix: wait only for usage that the API sender actually reports, scoped to the API subscription being revoked. Add a combined Pro/API deletion test with completed Pro usage and pending API usage, verifying that only the latter delays deletion.

### Medium: unauthenticated webhook bodies are buffered before the actual size check

Location: `apps/billing/src/index.ts:96`.

The public handler trusts Content-Length for its early 256 KiB limit, then reads the entire body with arrayBuffer. Only afterward does verifyWebhook check the actual byte count. A request with no Content-Length can force buffering far beyond 256 KiB without a valid signature. A large streaming request can exhaust the Worker's isolate memory and disrupt billing requests before verification returns 413. The provider's overall request limit does not enforce this application's much smaller budget.

Verification evaluated the actual handleFetch function body in memory with its setup dependency stubbed. A request with no Content-Length streamed sixteen 64 KiB chunks. All sixteen were consumed, and 1,048,576 bytes reached the service before the response was 413. This confirms the buffering order; production memory exhaustion was not attempted.

Suggested fix: reject methods other than POST before reading the body, and read the stream with a running byte count. Cancel immediately when it exceeds 256 KiB. Keep raw bytes for signature verification and retain the declared-length check as an early optimization.

### Low: a development dependency has a known cross-origin read advisory

Location: `bun.lock:997`, introduced through `bun.lock:741` and `bun.lock:249`.

Bun audit reports GHSA-67mh-4wv8-2f99 for esbuild versions through 0.24.2. The installed vulnerable copy is esbuild 0.18.20 under drizzle-kit's @esbuild-kit loader dependency. Other installed copies are 0.25.12 and 0.28.1 and fall outside that advisory's affected range.

If a developer runs this vulnerable copy's esbuild serve API and visits an attacker page, that page can request and read served content across origins. No invocation of that serve API was identified in convt's normal development or production paths, so this is a dependency finding with conditional development exposure, not a demonstrated production exploit.

Suggested fix: update or replace the loader dependency, or use a tested dependency override that removes the affected esbuild version. Run Bun audit again and verify migration tooling still works.

## Checks that found sound protections

- Session and account isolation: account server functions revalidate the session through authed and derive userId from it. Account queries filter by that id. Checkout disclosure also checks the owning session or nonce cookie. Database and web tests covered cross-account reads, revocation and license disclosure.
- Authentication and device sign-in: email codes are hashed, rotate, expire, have attempt limits and use atomic send buckets. OAuth linking requires a fresh, live session and server-side link intent. PKCE codes are random, hashed, five-minute and atomically deleted before verifier checks. Device bearer tokens are hashed and revocable. Origin checks protect cookie-authenticated mutations; device endpoints use code/verifier or bearer credentials instead. Redirect normalization tests rejected external hosts, encoded separators and dot-segment escapes.
- Billing: webhook verification authenticates raw bytes, bounds timestamps and checks both supported Polar signing schemes before parsing. Forged events and incorrect customer, checkout, product and price associations were refused in tests. Desktop and Pro signing require paid order or funded invoice coverage; trial and unfunded coverage tests issued no paid key. Transactional issuance, uniqueness, refunds and crash recovery passed. Production configuration rejects mock sinks and invalid, mismatched or development signing keys. Local secret files are ignored by Git.
- Cloud authorization and accounting: API keys have random secrets stored as SHA-256 hashes. Every job route checks ownership. Reservation creation and settlement lock the subscription row. Concurrent cap tests passed. Claims use SKIP LOCKED; renew and finish require a live attempt fence. A superseded worker could not publish a result or record usage. Usage is append-only and metering retries use the job id. Download signatures name individual result objects and last at most 300 seconds, bounded by job expiry. Sealed upload snapshots prevent later staging overwrites from changing queued input.
- Sandbox: the existing gate passed locally with runc. It verified an unprivileged uid, a read-only filesystem, absent inherited secrets and Docker socket, no external network, file-size, memory, CPU and wall limits, whole-container termination, and a container deadline without the worker. A real SVG conversion produced a PNG with the expected signature. Worker output extraction tests rejected links and traversal; collection uses O_NOFOLLOW and regular-file, count and byte budgets. This certifies only the tested local gate.
- Desktop and engines: update verification uses strict Ed25519 verification with a separate signing domain, schema and HTTPS URL checks, expiry, sequence rollback rejection and version/date downgrade rejection. The app persists the accepted sequence before selection. Account HTTP requests prohibit redirects and bound time and response size. Source inspection found URL conversions require a user click and auth callbacks require a pending state. Unix fallback credential files use exclusive creation and mode 0600. Engine subprocess arguments are passed separately, artifact names derive from fixed formats and numeric page indices, and output publication uses private staging and exclusive name reservation. Native-library discovery tests rejected relative overrides and inherited libheif plugin paths.

PASS: `bun run db:ci`: 30 database tests, 54 web integration tests and 21 server tests, plus migration drift, reversal, grants and sqlx checks.

PASS: billing integration suite under the existing disposable-database wrapper: 86 tests. Web, billing and license unit suites: 100 tests.

PASS: Rust license/client and vector tests: 30 passed, one Secret Service test ignored. Update tests: four passed. Worker tests: two passed. Core tests: 20 passed. Engine unit suite: 57 passed, two helper tests ignored; some native tests can return early when a dependency is unavailable, so this is not a complete native-engine certification.

PASS: local sandbox gate, including the real SVG-to-PNG conversion. Its owned containers were removed by the gate.

FAIL: Bun dependency audit, with the low finding above. The targeted deletion, webhook buffering and media HTTP probes reproduced the reported failures.

## NOT CHECKED

- Rust dependency advisories: cargo audit is not installed. Calling Cargo by its full path confirmed `no such command: audit`.
- Full desktop test suite: cargo test -p convt-app failed at the fontconfig build dependency because fontconfig.pc was unavailable. No GUI or browser was launched. The real app's callback and automatic-thumbnail behavior were inspected in source; the shared PKCE client and exact media-tool arguments were exercised separately.
- Production deployment: real GitHub/Google OAuth, Polar purchases and invoice reconciliation, Cloudflare Hyperdrive roles, deployed secrets, production update key and hosting, and live R2 policies and lifecycle. No real provider or production bucket was mutated.
- Deployed security headers and framing behavior. Source inspection found no global CSP, frame-ancestors/X-Frame-Options, nosniff or HSTS policy in the web Worker configuration or request middleware. Whether Cloudflare supplies them is unverified. No clickjacking exploit is asserted from that absence alone.
- The production worker host and runtime gate, production-size load, and end-to-end API cancellation or worker-crash cleanup with real object storage. The local gate has smaller resource budgets than production conversions; it is not a substitute for the target-host acceptance run.
- Packaged FFmpeg builds on all platforms, adversarial LibreOffice document behavior, and a full malicious-file corpus or fuzz campaign against FFmpeg, PDFium and libheif. The verified network finding is limited to the system FFmpeg fallback tested here. No native parser vulnerability is asserted.
- Real macOS Keychain, Windows Credential Manager, Linux Secret Service round trip, macOS/Windows URL handlers and signed release packages. Unix file fallback tests do not establish Windows fallback-file ACL protection.

## Resolution on 2026-10-05

The five verified findings were fixed in the shared checkout without commits or packaging changes. Each finding has a regression that failed against the previous behavior and passes with the fix. Logs are under `/home/leo/.agents/verify-notes/convt/evidence/2026-10-05-security-fixes/`.

Upload signatures now bind the exact reserved Content-Length. Local MinIO accepted a one-byte upload and rejected two bytes using the same URL with HTTP 403. Start checks staging size before copying, then checks the sealed snapshot. A separate account storage budget bounds declared inputs to 50 GB and 100 job prefixes. Cancelled and expired jobs retain their storage commitment until prefix deletion succeeds. Concurrent API and Pro creates share an account lock. The cancelled-upload regression failed before this accounting change; cancellation, expiry, cleanup and concurrent budget tests now pass.

Duration probing, conversion and desktop thumbnails share a file-and-pipe protocol whitelist and an allowlist of self-contained media demuxers. HLS, DASH and concat are refused, including references to other local files. The DASH regression issued eight HTTP requests before the fix. The final reproduction tests both a disguised MP4 and a Linux memfd through probe, conversion and the shared thumbnail command. System tools and `packaging/out/convt` tools each produced zero requests. The semantic matrix remains 579 passed, zero failed and zero skipped.

Account deletion now waits only for unreported `api_conversion` facts belonging to the API subscription being revoked. Its combined Pro/API regression first waits on a pending API fact, then completes after that fact is reported while Pro usage remains unreported.

The public webhook handler now reads raw bytes with a running 256 KiB cap. It rejects non-POST requests without reading their bodies and cancels a stream immediately on crossing the cap. The streaming regression previously consumed sixteen 64 KiB chunks; it now stops after the fifth chunk and cancels the reader. Exact-limit bytes remain unchanged for signature verification.

The esbuild override selects 0.28.1. The locked-version regression and Bun audit pass. Database CI verifies the loader through real Drizzle migration checks, schema export, reversal and sqlx preparation.

An independent GPT-6.1 Sol review at medium effort on Standard found the cancellation commitment gap after the first fixes. That gap was fixed with the failing regression above. Its second review found no concrete remaining findings in the five-fix scope. Production R2 enforcement and deployment checks remain outside this local result.

PASS: the final full `cargo test`, `bun run rs:check`, `bun run db:ci` including sqlx, `bun run check-types`, and `bun run build`. Billing integration passed 87 tests; the selected Bun unit suites passed 150. Four real SDK conversions passed against the local stack, and their disposable account and objects were cleaned up.

FAIL: the root `bun run check` reports only formatting in `packaging/macos/README.md`, which belongs to the separate packaging agent. Oxlint and formatting of all files changed by these fixes pass.

NOT CHECKED: the separate desktop crate test could not build because this host lacks `fontconfig.pc`. The shared thumbnail command was tested against real system and bundled FFmpeg. Desktop GUI launch and production provider checks remain unverified.
