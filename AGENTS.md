# convt

Local file conversion: right-click a file, pick a format. The engine is Rust; the desktop UI is GPUI; the site is TanStack Start. Licensed AGPL-3.0-only.

Users are people who want to convert files without uploading them. Conversions run on the user's machine; the paid cloud tier (convt-server and convt-worker) is the only part that receives files, and it is not built yet. Payments run through convt-billing (Polar as merchant of record); locally only against a mock. Pricing and licensing are product decisions: do not add a free tier, telemetry, or network calls to the desktop app or CLI without being asked.

## Layout

- `crates/convt-core`: format table, `Engine` trait, `Registry` that routes multi-hop conversions (BFS, max 3 hops). No native dependencies.
- `crates/convt-engines`: FFmpeg (subprocess), `image`, resvg, PDFium (dynamically loaded from `vendor/pdfium`), LibreOffice (headless subprocess), HEIC via libheif (dynamically loaded) or `sips` on macOS. `default_registry()` registers whatever runs on this machine.
- `crates/convt-cli`: the `convt` binary. `convt <files or folders> --to <fmt>` with options, `--preset`, `--json` progress, `-r` and `-j`; `convt formats [--json]`, `convt targets <file> [--menu]` (`--menu`: the short list right-click menus offer), `convt engines`, `convt presets`, `convt license [status|activate|remove]`.
- `crates/convt-app`: GPUI desktop app. Excluded from `default-members` because it needs system UI libraries.
- `crates/convt-ffi`: uniffi bindings for the OS integrations.
- `crates/convt-license`: Ed25519 offline license keys with an `updates_until` window, and (feature `client`) the trial and key storage every client shares.
- `crates/convt-server`, `crates/convt-worker`: cloud API and workers, deployed to Railway.
- `integrations/`: Finder Sync (macOS), Explorer plan (Windows), Nautilus/Dolphin/Nemo/Thunar (Linux).
- `apps/web`: convt.app on Cloudflare Workers: the landing page, sign-in (Better Auth: email code, GitHub, Google), the dashboard, and the checkout pages. It reaches Postgres through Hyperdrive and billing through the `BILLING` service binding; it holds no billing secret.
- `apps/docs`: convt.app/docs, a static [Blume](https://useblume.dev) site deployed as the `convt-docs` Worker on the `convt.app/docs*` routes. The API reference renders `crates/convt-server/openapi.json` through the overlay in `apps/docs/openapi/public.yaml`; the formats page is generated from `crates/convt-server/cloud-formats.json`.
- `apps/billing`: the `convt-billing` Worker: the Polar webhook route, the billing crons and the `BillingRpc` entrypoint. The only holder of the license signing key and the Polar and Resend secrets; connects as `convt_billing`.
- `packages/billing`: the billing logic both Workers and the tests share: the catalog, the Polar adapter and webhook verifier, ingest, license issuance, the email outbox, the reconciler and account deletion.
- `packages/mail`: the transactional email templates and transports (Resend, Mailpit, log).
- `packages/db`: the Postgres schema (Drizzle), migrations with hand-written down files, grants and triggers, the query functions the web app calls, the fixture seed, and the drift check. convt-server reads the same tables through sqlx.
- `packages/license`: the TypeScript license signer and verifier (byte-compatible with `convt-license`), id generator and API key hash.
- `tools/oauth-mock`: a local stand-in for GitHub and Google sign-in, used in development and tests only.
- `tools/billing-mock`: a local stand-in for Polar's API, checkout and portal pages and signed webhooks, and Resend's `/emails`, used in development and tests only.

## Commands

```sh
bun run setup                 # system packages, toolchains, PDFium
cargo test                    # Rust tests (default members)
bun run rs:check              # fmt + clippy -D warnings
cargo run -p convt-cli -- photo.png --to webp
cargo run -p convt-app        # needs the GPUI system libraries
bun run dev:web               # website and convt-billing on :3000 or the next free port, with local Postgres, Mailpit, OAuth and billing mocks
bun run db:seed               # fixture accounts (*@convt.test) in this checkout's database
bun run db:migrate            # also db:up, db:down, db:rollback, db:reset
bun run db:ci                 # schema drift, down files, DB integration tests, convt-server sqlx check
bun run billing:outbox list   # ambiguous or dead emails; `resolve <id> sent|resend`
bun run license:keygen PATH   # a production signing key, written outside the repo
bun run billing:backfill      # dry-run Polar orders; `--apply` writes; prod also needs `--confirm-production`
bun run build && bun run check-types && bun run check
bun run --cwd apps/docs dev     # docs on :4321; `validate` checks links, `deploy` ships convt.app/docs
```

## Conventions

- Adding a conversion means declaring `steps()` on an engine; the registry finds chains. Give a native or hardware path a higher `priority()` than the fallback.
- Engines that shell out find their tool via `CONVT_<TOOL>`, then next to the executable, then `PATH`.
- Multi-hop routes may not turn a still format into video or audio; only a direct step can.
- Tests that need FFmpeg, PDFium or LibreOffice belong in engine crates and must skip cleanly when the tool is missing.
- `convt-core` stays free of native and platform dependencies. Platform code lives in engines, the app, or `integrations/`.
- Every client (CLI, app, Finder extension, Linux menus, worker) gets conversions from `convt_engines::default_registry()`. Do not add a conversion path that bypasses it.
- License verification is offline. Never commit a signing key; tests generate their own. Only convt-billing signs keys, and only from paid coverage.
- `packages/db` owns the schema. A schema change is a Drizzle edit plus `drizzle-kit generate`, its down file in `migrations/down`, its grants in `sql/privileges.sql`, and `cargo sqlx prepare` if convt-server's queries change; `bun run db:ci` checks all of it. Production migrations are forward-only.
- Every account query takes the user id from the verified session, never from the request. Financial rows are never deleted, and `usage_events` is append-only in the database.
- Local services belong to one checkout: `scripts/db.sh` names and labels them by checkout path, and destructive commands pass its ownership guard. Never point them at another database.

## Terms

- **Format**: an entry in `convt_core::FORMATS`, identified by `id` (`jpeg`, `mp4`). Extensions map to formats.
- **Engine**: one backend that declares direct `steps()` between formats.
- **Route**: the chain of steps the registry picks, at most 3 hops.
- **Target**: a format a given input can reach. `convt targets` and the context menus list targets.

## Testing

Project skills live in `.agents/skills` (`.claude/skills` links there). Read the matching one before testing:

- `test-convt-cli`: engines, formats, routing, the CLI, and `integrations/linux`. Most changes start here.
- `test-convt-desktop`: anything in `crates/convt-app`.
- `test-convt-web`: anything in `apps/web`, `apps/billing`, `packages/db`, `packages/billing`, `packages/mail`, `packages/license`, `tools/oauth-mock` or `tools/billing-mock`.
- `test-convt-server`: `crates/convt-server` and `crates/convt-worker`.

`crates/convt-ffi` builds anywhere (`cargo build -p convt-ffi`), but the Finder extension in `integrations/macos` needs Xcode and has no Linux test path; say so instead of claiming it works. `integrations/windows` is a plan only.

Verification standard:

- Prove behavior with a real run plus inspected output, not only a passing build. Run `cargo test` and `bun run rs:check` for Rust changes, and `bun run check && bun run check-types && bun run build` for web changes.
- A routing or format change affects every client. Check `convt targets` for the formats around your change, including that nothing new is offered that should not be.
- Report blocked checks plainly: a missing engine, missing system libraries, or a GUI or browser check you had no permission to run.
- Launching the desktop app or a browser needs the user's explicit permission in the current request. The skills say what to do without it.

## Parallel work

Several agents may share this machine. Use temporary directories from `mktemp`, pick free ports, track the PIDs you start, and kill only those. Never `pkill` by name. Each worktree has its own `target/` and `vendor/pdfium`; run `bash scripts/fetch-pdfium.sh` in a new worktree.
