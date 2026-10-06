# convt build plan

This plan takes convt from the current scaffold to a shippable product: the desktop app with OS right-click menus, licensing and the 7-day trial, accounts and billing on convt.app, and the cloud API. Each phase ends with something that runs and is verified. Phases marked **Design** need screens from the Paper design thread; engineering builds the working piece with plain UI first, and the designed layer goes on top when the screens land.

Status as of 2026-10-02: the scaffold builds, `cargo test` passes, the CLI converts across all five engines on Linux, the Linux menus install, the server answers `/health` and `/v1/formats`, and the website has one placeholder route. The landing page is designed and approved in Paper (file `01M3YMT3MHKBQA2F22QPSHG030`, artboard "convt — Landing").

## What the landing page commits us to

These lines are on the approved page, so the product has to back them:

- "40 formats, one menu." Images (14, including HEIC and SVG), video (5), audio (7) and documents (14), plus PDF to images.
- "Nothing gets uploaded." The desktop app and CLI never send a file anywhere unless the user explicitly sends that job to the cloud (Pro only, see P9). Whether the hero line needs softening is a decision below.
- Desktop, $29 once: every format offline, macOS, Windows and Linux, 12 months of updates, batch folders and presets. "Keep your version forever."
- Pro, $12/month or $8/month yearly: everything in Desktop, convert from phone or browser, heavy video jobs in the cloud, every future update included.
- API: same engines, pay per conversion, no plan needed. The sample uses an `@convt/sdk` package with `convt.convert("report.docx", { to: "pdf" })` and `out.save(...)`.
- "Both plans start with a 7-day free trial."
- Footer links: Download, Formats, Pricing, Changelog, API docs, GitHub, Status, Privacy, Terms, Contact.

## HEIC

HEIC decoding and encoding are implemented through dynamically loaded libheif. HEIC output is offered only when libheif can instantiate a HEVC encoder. Linux bundles x265, libde265 and aom plugins; macOS also declares a native sips encode path, which is unverified here.

- macOS: a native engine using ImageIO, which ships with the OS and covers the HEVC licensing. Highest priority on macOS.
- Windows and Linux: libheif is loaded in process from `CONVT_LIBHEIF_DIR`, next to the executable, fixed bundle directories, then system paths. The bundled library exports `heif_convt_init_no_plugins`. Unpatched system libraries are unavailable because their automatic initialization can load plugins from inherited paths. Only absolute bundle plugin directories and an absolute `CONVT_LIBHEIF_PLUGIN_DIR` are loaded explicitly; relative overrides and `LIBHEIF_PLUGIN_PATH` are ignored by the engine.
- HEIC output is in scope. Quality, alpha, orientation and colour are checked by semantic round trips. ICC profiles are retained; PNG colour metadata maps to NCLX, with sRGB assumed for untagged inputs. Sixteen-bit inputs attempt 10-bit HEIC; an encoder that explicitly rejects that depth falls back to 8-bit. AVIF retains 10-bit output for those inputs. See [HEIC and AVIF engine](heic.md) for colour mapping, security requirements and verification. Raster to SVG remains unsupported.

If libheif bundling on Windows turns out to be a licensing or size problem, the fallback is FFmpeg's HEIF demuxer (FFmpeg 7.1 and later handle tiled HEIC), which is already a dependency.

## Decisions for Leo

Each has a default the plan assumes. Changing one changes the named phases.

| Decision                | Default                                                                                                                                                                                                                                                                            | Affects     |
| ----------------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ----------- |
| Payments                | Polar as merchant of record: handles sales tax and VAT, one-time products, subscriptions with trials, and usage meters for the API. Stripe with Stripe Tax is the alternative if Leo wants to own tax registration.                                                                | P7, P9      |
| Auth                    | Better Auth inside the TanStack Start app: email magic link plus GitHub and Google sign-in. Sessions in Postgres.                                                                                                                                                                  | P6          |
| Database                | Postgres on Railway, next to convt-server. The Worker reaches it through Cloudflare Hyperdrive.                                                                                                                                                                                    | P6, P9      |
| Cloud worker host       | Railway API, worker replicas, Postgres and Buckets. Process sandbox fails closed unless the host provides Landlock, seccomp, delegated cgroups and private tmpfs mounts. Actual Railway gate remains open.                                                                         | P9          |
| Desktop trial           | Local and account-free: the first conversion starts a 7-day clock stored on the machine. No network call. It can be reset by a determined user, which is acceptable for AGPL software anyone can build.                                                                            | P3          |
| Pro trial               | 7-day subscription trial through the payment provider, card required.                                                                                                                                                                                                              | P7          |
| CLI gating              | Packaged builds (app, CLI, OS menus) all check the license, so the CLI cannot be used to get around it. Builds from source are unrestricted; the AGPL allows removing the check anyway.                                                                                            | P3          |
| Pro cloud limits        | Fair use: 50 GB of input per month and 2 GB per file, shown on the dashboard.                                                                                                                                                                                                      | P9          |
| API price               | Placeholder of $0.01 per conversion, billed monthly in arrears through usage metering, with a user-set spend cap. Needs a real number before launch.                                                                                                                               | P9          |
| "Nothing gets uploaded" | Keep the hero line, and make cloud conversion from the desktop a per-job, explicit choice that Pro users opt into, with its own consent screen. The alternative is cloud conversion only in the browser, which keeps the line literally true. Needs Leo's and the designer's call. | P9, P10     |
| API billing             | A separate API product with no base fee and a metered price. Adding a payment method enrolls the account; API keys work only after that. Independent of Desktop and Pro.                                                                                                           | P7, P9      |
| Lifetime plan           | Not on the page. Remove `Plan::Lifetime` from convt-license until a lifetime offer exists.                                                                                                                                                                                         | P3          |
| Apple Developer account | Required for Developer ID signing, notarization and the Finder extension. $99/year.                                                                                                                                                                                                | P4, P11     |
| Windows signing         | Azure Trusted Signing (about $10/month) rather than a hardware-token EV certificate.                                                                                                                                                                                               | P5, P11     |
| GitHub repository       | Public `convt` repo, created when Leo says so. CI on hosted runners is how macOS and Windows builds get verified.                                                                                                                                                                  | P4, P5, P11 |

## Phases

The order runs Linux-verifiable work first, because this server is Linux. macOS work needs a Mac or a macOS CI runner, and Windows work needs leopc or a Windows runner. Those parts are written here but marked blocked until the hardware or CI exists.

### P0. Foundations

- Fix the verified findings from the scaffold review.
- Add cross-language test vectors for license tokens (fixed seed, fixed payload, expected token) so the TypeScript signer in P7 can be checked against the Rust verifier.
- Add `tower` as a dev-dependency and in-process route tests for convt-server.

Done when `cargo test`, `bun run rs:check`, `bun run check`, `bun run check-types` and `bun run build` all pass.

### P1. Conversion jobs (no design)

The engine layer today converts one file and returns a path. Every client needs more than that.

- Job model in convt-core: a `Job` with input, target, options and output policy; progress events (fraction where the engine can report it, indeterminate otherwise); cancellation; a typed error taxonomy (unsupported input, engine missing, engine failed with stderr tail, output exists, cancelled).
- Outputs are a collection, not one path. PDFium writes one image per page today (`crates/convt-engines/src/pdfium.rs`), and `Registry::convert` passes only the first file to the next hop, so PDF to WebP through PNG drops every page after the first. Engines return all the artifacts they wrote, and each later hop runs on every artifact.
- Output rules: never overwrite the input or any existing file. Engines write into a private temporary directory; publishing reserves each final name with an exclusive create (` (1)`, ` (2)` on collision, pages as `-2`, `-3`) and moves the file there, so two jobs that pick the same name cannot replace each other. Failure or cancellation removes everything the job wrote. The default destination is next to the input.
- Tests: a multi-page PDF through a two-hop route, two simultaneous jobs with the same output name, an existing file at the target name, and cancellation mid-job.
- Progress from FFmpeg via `-progress pipe:1` and the probed duration. LibreOffice and PDFium report per page or not at all.
- Options and presets: a small, typed option set per target (image quality and max size, video resolution and quality, audio bitrate, PDF page range and DPI). Presets are named option sets stored as TOML.
- Batch: a list of files or a folder (recursive opt-in) with bounded concurrency. Default is CPU count for images, one at a time for video.
- HEIC engines as described above.
- CLI picks up all of this: `--preset`, `--quality`, `--jobs`, a progress bar on a TTY, and JSON progress lines with `--json` for the integrations.

Verified with the `test-convt-cli` skill, plus new fixtures for HEIC (generated with `heif-enc` when available) and a folder batch.

### P2. Desktop app (Design)

The GPUI app becomes the place every entry point lands.

- Single instance: the first launch owns a local socket in the user runtime directory; later launches (from a menu, open-with, the CLI or a `convt://` URL) forward their request and exit.
- Entry points: `convt-app open <files> [--to fmt]`, file associations for open-with, and the `convt://convert?...` URL scheme.
- Linux menus switch to the app. Nautilus (`integrations/linux/nautilus/convt_nautilus.py`) and the Dolphin, Nemo and Thunar entries from `install.py` call the CLI directly today, so a menu conversion has no progress, no cancel and nowhere to show an error or an expired trial. They change to `convt-app open ... --to ...` and keep using `convt targets` to build the menu. Tests cover filenames with spaces and quotes.
- Quick-convert window: a small window that opens at the cursor or centered, shows the file, the targets from the registry, and a preset choice, then shows progress and a done state with "Show in folder".
- Main window: a drop zone, the queue with per-job progress and cancel, history (stored in SQLite under the app data directory), the format picker, presets, and settings (default destination, concurrency, notifications, update checks).
- System notifications when a job finishes while the app is in the background.
- Tray or menu bar presence while jobs run.

Engineering ships the full behavior with plain gpui-component widgets. The designed screens replace the views without changing the job layer.

**Screens needed from design:**

1. Quick-convert window: choose target, choose preset, converting, done, failed, batch of several files.
2. Main window: empty state with drop zone, queue with running and finished jobs, history, format picker.
3. Presets: list and edit.
4. Settings.
5. Notifications: done and failed copy, and the menu bar or tray menu.

### P3. Licensing and trial in the app (Design)

- Trial: the first conversion records the start date. Days left are shown in the app. When the trial ends, conversions stop and the app offers Buy and Enter license.
- Activation: paste a key, or click a `convt://activate?key=...` link from the purchase email or dashboard. Keys are stored in the OS credential store (Keychain, Windows Credential Manager, Secret Service), with a file fallback on Linux when no secret service runs.
- Update window: each build embeds its build date. A Desktop key covers builds dated on or before `updates_until`; a newer build tells the user which version their license covers and links to it. Old builds keep working forever.
- Pro keys carry `updates_until` equal to the paid-through date. Renewing them needs accounts and billing, so it is built in P8, not here. In P3 a Pro key is verified offline like any other key.
- The CLI and the OS menu entry points share the same check through convt-license.
- Remove `Plan::Lifetime`, add a `trial` state type, and add the public key embedding at build time (`CONVT_LICENSE_PUBKEY`, with a dev key for local builds).

**Screens needed from design:**

1. Trial banner and days-left indicator.
2. Trial ended.
3. Enter license, success, and invalid key.
4. "This build is newer than your license" with the covered version.
5. License section in settings.

### P4. macOS integration (Design, blocked on a Mac and an Apple Developer account)

- App bundle assembly: `convt.app` with the GPUI binary, the `convt` CLI, FFmpeg, ffprobe, PDFium and libheif. Discovery today checks only `CONVT_<TOOL>`, the executable's directory and `PATH` (`crates/convt-engines/src/lib.rs`, `pdfium.rs`), so P4 adds bundle-aware discovery: `Contents/MacOS`, `Contents/Frameworks`, and for LibreOffice `/Applications/LibreOffice.app`. LibreOffice is a per-user document pack, downloaded only on an explicit Install action. See [document-pack.md](document-pack.md).
- Finder Sync extension (sandboxed): it runs from its own executable inside `Contents/PlugIns`, so probing tools from there would give different targets than the app. Instead the app writes its available targets to the shared App Group container at launch and whenever engines change, and the extension builds the "Convert with convt" submenu from that list (with no valid list, it shows only "Open in convt…" and lets the app work out the targets, rather than guessing from the static format table). It hands the selection to the app through the `convt://` URL scheme. The app is not sandboxed (Developer ID distribution), so it can read the files.
- Verification runs against the packaged, signed `.app`, not development binaries.
- A Services menu entry as a fallback where Finder Sync is disabled.
- uniffi XCFramework build script for convt-ffi.
- Codesigning with hardened runtime, notarization and stapling, scripted.
- Onboarding to enable the extension in System Settings, with a check that it is enabled.

**Screens needed from design:**

1. The Finder submenu: wording, icon, grouping and order of targets, and a "More…" item that opens quick-convert.
2. First run on macOS: welcome, enable the Finder extension (with the System Settings steps), done.
3. App icon and menu bar icon.

### P5. Windows and Linux integration

- Windows: an `IExplorerCommand` COM DLL in Rust (windows-rs) for the Windows 11 top-level context menu, registered through a sparse MSIX package, plus classic registry verbs as the Windows 10 fallback. MSI or MSIX installer bundling FFmpeg, PDFium and libheif. Signed. Blocked until leopc can be used for testing or a Windows CI runner exists.
- Linux x86_64 packaging is done. `bun run bundle:linux` rebuilds the current CLI, app and codecs in pinned manylinux/AlmaLinux 8 and enforces glibc 2.28. `bun run bundle:appimage` wraps the payload; `bun run package:deb`, `bun run package:rpm` and `bun run package:linux` reuse those binaries with hash-locked tools in pinned offline containers. Native packages install under `/opt/convt`, with `/usr/bin` launchers, desktop metadata, an icon, AppStream metadata and system Nautilus, Nemo and Dolphin integration. Thunar remains an explicit per-user setup because it has no system custom-action directory. `install.py` stays available and avoids duplicate system entries. Ubuntu 22.04, Ubuntu 24.04, Debian 12, Fedora 44 and AlmaLinux 8 pass installed-CLI offline matrices of 349 cases without documents, 579 with distro LibreOffice, and 349 after Office removal. Upgrade, reinstall, removal, metadata, GUI library resolution and package reproducibility are verified in [Linux packaging evidence](../packaging/linux/VERIFICATION.md). Packages are unsigned; signing is P11. Windows remains blocked as described above. Flatpak remains deferred.

**Design:** Windows and Linux reuse the quick-convert window and onboarding from P2 and P4. Windows needs one first-run screen: on Windows 11 the entry is in the top-level menu through the sparse package; on Windows 10 it is in the classic menu. "Show more options" is mentioned only if the Windows 11 package fails and the classic verb is the fallback.

### P6. Accounts and dashboard shell (Design)

- `packages/db`: Drizzle schema and migrations for users, sessions, licenses, subscriptions, API keys, usage events and cloud jobs. convt-server reads and writes the same tables through sqlx; a CI check runs both against a disposable Postgres.
- Better Auth in apps/web with magic link, GitHub and Google.
- Dashboard routes: overview, license (key, plan, updates-until, download links, activate-on-this-computer button), billing, API keys and usage. Pages render real data from the local database with seeded fixtures before billing exists.
- Local development uses a Postgres container per worktree; the `test-convt-web` skill gains the database and sign-in steps.

P6 is done locally, as specified in [the P6 plan](p6-accounts-plan.md). Sign-in works by email code or the emailed link (Better Auth's email OTP plugin), and GitHub and Google work through a local OAuth mock. Only Gmail and Workspace addresses count as verified without a code. The dashboard, licenses, billing, API and settings pages render each seeded account state from Postgres in the existing design. Settings can rename, change the email by code, connect and remove GitHub or Google, and sign out one session or every other session and Mac. "Activate on this computer" opens `convt://activate`. `packages/db` owns the schema, grants and append-only usage triggers. convt-server checks migration hashes at startup and has sqlx offline data. `bun run db:ci` checks schema drift, down migrations, the integration tests and the sqlx data against a disposable Postgres. Everything runs against per-checkout containers; nothing is deployed. Still to come: billing actions and real purchases (P7), API key creation (P9), account deletion (P7), Apple sign-in (P4). Railway, Hyperdrive, the OAuth apps and the email provider are listed in the P6 plan as accounts for Leo to create. The real GitHub and Google flows stay unverified until those credentials exist.

**Screens needed from design:**

1. Sign in and sign up, including the magic-link sent state.
2. Dashboard shell and navigation.
3. Overview for each state: trial, Desktop owner, Pro, Pro lapsed, API-only.
4. License page.

### P7. Billing and license issuance (Design)

- Products in the payment provider: Desktop $29 one-time, Pro monthly $12 and yearly $96 with the 7-day trial, and the API product (no base fee, metered price per conversion). API enrollment is a checkout that only collects a payment method; API keys are usable after it succeeds.
- Checkout from the pricing page and the app's Buy button, then a success page that shows the key and an "Open in convt" activation link.
- Webhook handler in the Worker. It issues Desktop keys signed with Ed25519 through WebCrypto, creates and updates Pro and API subscriptions, reissues Pro keys on renewal, and handles refunds by revoking the key in the dashboard (offline keys cannot be revoked in the app; this is accepted). Because this code holds the production signing key, it must:
  - verify the provider's signature over the raw request body before parsing anything;
  - check product, price, amount and customer against our own records, and treat a trial as a trial entitlement, never a paid one;
  - enforce uniqueness on the business fact (one Desktop license per order, one Pro key per billing period), not only on the event id, since different events can describe the same purchase and can arrive out of order;
  - record the event, the entitlement change and the issued license in one database transaction, and send email through an outbox table so a crash cannot issue twice or lose a key;
  - reconcile against the provider's API on a schedule to catch missed events.
    Tests cover forged signatures, duplicate and reordered deliveries, and a crash between issuing and emailing.
- Customer portal link for invoices, payment method and cancellation.
- Transactional email: purchase receipt with the key, trial ending, renewal failed. Through the provider's email or Resend.
- The signing key lives only as a Worker secret. The TypeScript signer must pass the P0 test vectors.

P7's local work is done, as specified in [the P7 plan](p7-billing-plan.md). Pricing buttons start real checkouts (Desktop signed out; Pro after sign-in, with the 7-day trial only for an account that never had Pro), and the success page shows the key once with Copy and Open in convt, released only to the browser holding the checkout's nonce cookie or the owning account. A new Worker, convt-billing, holds the signing key and the Polar and Resend secrets: it verifies Polar's webhook signatures before parsing, checks every fact against our own checkouts, catalog and customers, and issues keys only from paid coverage (Desktop per order; Pro per paid period, none during a trial), with refunds and lost disputes revoking them on the dashboard. Emails go through an outbox to Resend with frozen payloads and idempotency keys. A reconciler catches dropped webhooks, late refunds and disputes, and runs a daily drift and invariant check with a digest. The billing page switches between monthly and yearly, cancels and resumes, opens Polar's portal and fetches receipts; the API page enrolls with a spend cap; settings deletes the account after ending its subscriptions. All of it runs against a local mock of Polar and Resend and is covered by integration tests and `apps/web/e2e/billing.sh`. Nothing is deployed and no real payment account was used.

Pending on Leo: the Polar sandbox and production organizations with their products, settings, access token and webhook endpoint; the production signing key (`bun run license:keygen`); the `convt_billing` database role password and its Hyperdrive config; `RESEND_API_KEY` and `ALERT_EMAIL` on convt-billing; and a real API price. Then the sandbox check in step 10 of the P7 plan, which must confirm what a checkout for the metered-only API product produces before launch, what a trial and a lost chargeback do to orders, and whether the checkout lets a buyer change the email we pass. The P7 Paper screens do not exist; the pages use the existing design system until they do.

**Screens needed from design:**

1. Checkout success with the license key.
2. Billing page: plan, next charge, switch monthly and yearly, cancel.
3. API enrollment: add a payment method, enrolled, payment failed.
4. Email templates: purchase, trial ending, payment failed.

### P8. Desktop sign-in and Pro renewal (Design)

Pro keys expire with the billing period, so the app needs a way to get a fresh one.

- Desktop sign-in through the browser: the app opens convt.app, the user signs in, and the site returns a device token through `convt://auth?...`. The token is stored in the OS credential store and can be revoked from the dashboard.
- Renewal endpoint that returns the current Pro key for the signed-in account. The app calls it at launch at most once a day and from a "Refresh license" button in settings, independent of the update-check setting. This is a second automatic network call, made only while signed in, and settings and the privacy policy describe it next to the update check. Offline, the app keeps using the key it has. If the subscription lapsed, the last key stays valid for every build it covers.
- Desktop owners never need to sign in; this is only for Pro and for the cloud jobs in P9.

P8 is done locally. Sign-in is optional and starts from Settings, License, or the first-run plan step; starting the trial no longer opens a browser. The app makes a one-time state and a PKCE verifier and opens `/device?state=...&challenge=...&name=...&os=...&version=`. After the user approves, the site stores a five-minute one-time code with the user id and the challenge in `verifications` and opens `convt://auth?state=...&code=...` (or `&error=access_denied` on Cancel). The link carries a code rather than the token, so a link caught by another handler is useless without the verifier, which never leaves the app. The app accepts the link only while it holds a pending flow with that state, uses the flow up, and posts the code and verifier to `/api/device/token`. The site deletes the code before checking anything, checks the verifier, and creates a `devices` row holding only the SHA-256 of a new `cvd_` token, which it returns once. The app keeps the token and email in the credential store next to the license key. A link the app didn't start, one from a cancelled or expired flow, and a replay are all dropped without a network call, and the app says so. Renewal is `/api/device/license` with the token as a bearer: the site finds the unrevoked device and asks convt-billing's `currentProKey`, the newest unrevoked paid Pro key whatever the subscription's status, so a lapsed account gets its last key. The app calls it at launch at most once a UTC day while signed in, right after signing in, and from Refresh license. A key that comes back is stored without a confirm click only if it verifies, is Pro, and covers later builds than the stored key, so renewal never shortens what a machine may run. Offline or with no Pro key, the stored key stays. A 401 (the device was signed out on the dashboard, or by "sign out everywhere else" or an email change) signs the app out and keeps the key. The app's Sign out forgets the token and revokes it. Settings, General, lists the network use next to each other: the update check and the license refresh. The /device page states what the refresh sends, and the privacy page draft (P10) says the same. Per-hour limits: 10 approvals per user, 30 code exchanges per IP, 120 renewals per IP and 30 per device. The `/api/device` routes use no cookies and skip the Origin check. Migration 0002 grants `insert on devices` to `convt_web`; there is no table change. The CLI is unchanged and makes no network calls; there is no `convt license refresh`.

Verified with headless app tests for every flow (including unsolicited, replayed, stale and expired links, offline renewal, a lapsed account, a revoked device and Sign out), web integration tests against a disposable Postgres (one-use codes, wrong verifiers, expiry, revocation, cross-account isolation, rate limits), and an end-to-end run of the real app under Xvfb against the local dev server with the browser approval in agent-browser and the link delivered through `convt-app '<link>'`, as the desktop file's `%U` does. Real Keychain, Credential Manager and Secret Service storage, and the macOS and Windows URL handlers, are unverified here. The P8 Paper screens do not exist; the screens use the existing GPUI theme and site design.

**Screens needed from design:** sign in from the app (waiting for browser, signed in, failed), signed-in state in settings, and refresh failed.

### P9. Cloud conversions and the API (Design)

P9 is implemented locally. The jobs API, fenced queue, reservations, hashed keys, metering sender, SDK, generated OpenAPI contract and dashboard converter are wired together. The default process sandbox and optional Docker backend passed local gates and real conversions. Leo selected Railway API, worker replicas, Postgres and Buckets. Railway kernel, delegated cgroup and mount compatibility remains unverified; the worker fails closed when required protections are absent. Production provisioning, the target-host gate, real Polar metering and SDK publication remain launch checks. See [the P9 runbook](p9-cloud-plan.md). Desktop cloud consent is owned by the desktop implementation.

- Jobs API in convt-server (`api.convt.app`): create a job and get a presigned upload URL, start it, poll or subscribe for status, download the result. Files go directly to private Railway Buckets. Jobs expire after 24 hours and per-minute cleanup removes their object prefixes.
- Sandbox first. Before the API accepts any upload, prove on the target host that a conversion runs under a separate unprivileged user in a private chroot with only its input, output and trusted engine runtime visible, no network, no inherited secrets or environment, CPU, memory, file-size and wall-clock limits, and that cancelling kills the whole process tree. PDFium runs in-process today, so the worker runs each job as a separate `convt` process inside that sandbox. This is the acceptance gate for P9. The process backend requires Landlock, seccomp, delegated cgroup v2 limits and private tmpfs mounts. If Railway cannot provide them, deployment remains blocked until hosting is resolved. The unsafe diagnostic override is not a production option.
- Queue in Postgres. Jobs move through created, uploaded, queued, running, succeeded, failed and cancelled. Workers claim with `FOR UPDATE SKIP LOCKED` and hold a lease they renew while running; an expired lease returns the job to the queue, with at most three attempts. Each claim gets an attempt number that acts as a fencing token. Outputs go to attempt-specific keys, and marking the job finished, publishing its outputs and recording usage all require that the attempt still holds the current lease, so a worker whose lease expired cannot overwrite a retry's result. A test lets the expired worker finish after its replacement. Workers run `default_registry()` with the same engines as the desktop.
- Authorization: every job operation checks that the caller owns the job. API calls authenticate with an API key; the Pro web converter calls the Worker with the user's session, and the Worker calls convt-server with a short-lived token naming the account. Download URLs are presigned per job and expire. Tests cover one account reading or downloading another's job.
- Limits: at job creation the server makes a reservation in one transaction, checking settled usage plus open reservations: bytes against the monthly allowance for Pro, the per-conversion charge against the spend cap for the API. Success settles the reservation; failure, cancellation or expiry releases it. The server also rejects files over 2 GB, and checks the real uploaded size before queueing. Trial and lapsed accounts cannot create jobs. Tests run concurrent jobs against a nearly used-up cap.
- API keys: prefixed, shown once, stored hashed, scoped to an account. Rate limits per key.
- Metering: the job's success and a usage record keyed by the job id are written in one transaction; a separate sender reports usage records to the payment provider with that id as the idempotency key, so retries and crashes bill once.
- `@convt/sdk` in `packages/sdk`: the `Convt` class from the landing page, Node and browser, with a typed format list generated from the registry.
- Pro web converter on the dashboard: upload from a phone or browser, pick a target, download. Pro's "heavy video jobs in the cloud" is the desktop app offering to send a job to the cloud when the user chooses it for that job, never automatically, subject to the "Nothing gets uploaded" decision above. The app uses the P8 sign-in.
- API docs on convt.app, generated from an OpenAPI spec that convt-server serves.

**Screens needed from design:**

1. API keys: list, create, shown-once state, revoke.
2. Usage: conversions per day, spend, spend cap.
3. Web converter for Pro, desktop and phone widths: upload, pick target, progress, download, errors.
4. Desktop app: the "send to cloud" choice with its consent wording, and its progress state.
5. Limit reached (Pro allowance, API spend cap) and API not enrolled.

### P10. Website

- Build the approved landing page from Paper in apps/web, desktop and mobile. Pull exact values from Paper with `get_jsx` and `get_computed_styles`.
- Pricing with the monthly and yearly toggle wired to checkout.
- Download page that picks the right build for the visitor's OS.
- Formats page generated from `convt formats --json` at build time.
- Changelog, privacy policy, terms, contact, and API docs (from P9).
- Status links to an external status page.

**Screens needed from design:** download, formats, changelog, API docs layout, and one legal-page template. Mobile versions of all of them, including the landing page.

### P11. Releases and updates

- Release pipeline that builds, signs and notarizes macOS (universal), Windows and Linux artifacts, and uploads them to R2.
- A source archive for every release, built from the same commit, published next to the binaries, plus the build scripts and the exact source of every bundled third-party component (FFmpeg, PDFium, libheif and libde265). The download page, the About window and the API's responses link to the matching source, which covers both binary distribution and the AGPL's network-use clause for convt-server.
- A signed update manifest (Ed25519, separate key from licenses) listing builds with their dates. The app checks it at most daily when update checks are on, shows only builds its license covers, and links to the purchase page for the rest. Document-pack downloads require an explicit user action and do not run during discovery. Apart from Pro renewal for signed-in users (P8), this is the only network call the desktop app makes without a user action, and it is documented in settings and the privacy policy.

  The app side is built (`crates/convt-app/src/update.rs`). While update checks are on (Settings, General; on by default), it fetches the manifest at launch at most once a UTC day and on Check now, from the placeholder `UPDATE_MANIFEST_URL` in `src/placeholder.rs` (`https://convt.app/updates/manifest.json` until the release host exists; builds from source can set `CONVT_UPDATE_URL` and `CONVT_UPDATE_PUBKEY`). It verifies the manifest with `convt-update` against the embedded update key, keeps the highest accepted `sequence` in `settings.toml` and refuses older ones, and selects for this install's platform and package kind with the license's `updates_until` (a trial or an unlicensed build counts as covering every build). A covered newer build shows "Update available" in the main window's sidebar and Settings and opens the download page; nothing is downloaded or installed. A newer build outside the license shows "New version" with Renew, linking to the manifest's `purchase_url`. Offline, bad signatures, rollbacks and stale manifests are silent apart from a note in Settings. A build without an update key never fetches. The request carries the app version as its user agent and nothing else. Real hosting, the production key and the macOS and Windows package kinds are unverified.

- Homebrew cask and winget manifest. P5 supplies reproducible unsigned Linux deb/rpm artifacts; P11 signs copies after reproducibility checks and publishes them with their matching source and manifests.

**Design:** the update-available prompt and the "update not covered by your license" state reuse the P3 license screens; one more screen for update available.

### P12. Launch readiness

- Security review of the worker sandbox, webhook handling, API key storage and license signing. The five verified local findings in [the launch security review](p12-security-review.md) are fixed with regressions: upload size and storage commitments, offline media protocols and demuxers, API deletion metering, bounded webhook streams, and esbuild. The independent follow-up found no remaining findings in that scope. Production checks remain open.
- A real API price, refund policy, and the legal pages reviewed.
- Linux verification inventory is in `packaging/linux/INVENTORY.md`, and the payload copies notices and FFmpeg configure flags. Complete corresponding source for the static FFmpeg build and all its linked libraries, a Rust dependency inventory, and public document-pack hosting remain release gates. The local bundle is a verification artifact.
- License inventory of what actually ships: the FFmpeg build's configuration (the ffmpeg engine passes `-c:v libx264`, which makes the build GPL), PDFium, libheif and libde265, and every Rust crate. Exclude anything that cannot be redistributed, include notices and any relinking material required, and confirm everything is compatible with AGPL-3.0.
- Codec patents reviewed separately from copyright: H.264, HEVC (HEIC) and AAC in the bundled FFmpeg and libheif builds, per platform. On macOS, prefer the system codecs where they cover the format.
- Load test the cloud queue with the agreed limits.
- Support inbox and a way to resend a license key.

## Design track

The designer can work in this order while engineering runs ahead with plain UI:

1. P2 quick-convert window and main window. These are the core of the product and everything else reuses their components.
2. P3 license and trial states, and P4 macOS onboarding and Finder menu.
3. P6, P7 and P8: sign-in, dashboard shell, overview states, license, billing, checkout success, API enrollment, emails, and desktop sign-in.
4. P9 API keys, usage and the Pro web converter.
5. P10 site pages and mobile landing.

Each screen needs its states, not only the happy path: empty, loading or running, done, error, and where relevant trial, expired and lapsed.

## How the work runs

- All work stays local: no pushes, PRs or deploys until Leo asks.
- Each phase is implemented, then reviewed by an independent GPT-6.1 Sol agent, then fixed and verified with the matching test skill.
- GUI and browser checks need Leo's permission in the request that asks for them; without it they are reported as pending.
- Parts that need macOS, Windows, an Apple Developer account, real payment credentials or a real database are built as far as they can be verified here and reported as blocked past that point.
