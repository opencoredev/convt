# convt

Local file conversion: right-click a file, pick a format. The engine is Rust; the desktop UI is GPUI; the site is TanStack Start. Licensed AGPL-3.0-only.

Users are people who want to convert files without uploading them. Conversions run on the user's machine; the paid cloud tier (convt-server and convt-worker) is the only part that receives files, and it is not built yet. Pricing and licensing are product decisions: do not add a free tier, telemetry, or network calls to the desktop app or CLI without being asked.

## Layout

- `crates/convt-core`: format table, `Engine` trait, `Registry` that routes multi-hop conversions (BFS, max 3 hops). No native dependencies.
- `crates/convt-engines`: FFmpeg (subprocess), `image`, resvg, PDFium (dynamically loaded from `vendor/pdfium`), LibreOffice (headless subprocess), HEIC via libheif (dynamically loaded) or `sips` on macOS. `default_registry()` registers whatever runs on this machine.
- `crates/convt-cli`: the `convt` binary. `convt <files or folders> --to <fmt>` with options, `--preset`, `--json` progress, `-r` and `-j`; `convt formats [--json]`, `convt targets <file>`, `convt engines`, `convt presets`, `convt license [status|activate|remove]`.
- `crates/convt-app`: GPUI desktop app. Excluded from `default-members` because it needs system UI libraries.
- `crates/convt-ffi`: uniffi bindings for the OS integrations.
- `crates/convt-license`: Ed25519 offline license keys with an `updates_until` window, and (feature `client`) the trial and key storage every client shares.
- `crates/convt-server`, `crates/convt-worker`: cloud API and workers, deployed to Railway.
- `integrations/`: Finder Sync (macOS), Explorer plan (Windows), Nautilus/Dolphin/Nemo/Thunar (Linux).
- `apps/web`: convt.app on Cloudflare Workers.

## Commands

```sh
bun run setup                 # system packages, toolchains, PDFium
cargo test                    # Rust tests (default members)
bun run rs:check              # fmt + clippy -D warnings
cargo run -p convt-cli -- photo.png --to webp
cargo run -p convt-app        # needs the GPUI system libraries
bun run dev:web               # website on :3000
bun run build && bun run check-types && bun run check
```

## Conventions

- Adding a conversion means declaring `steps()` on an engine; the registry finds chains. Give a native or hardware path a higher `priority()` than the fallback.
- Engines that shell out find their tool via `CONVT_<TOOL>`, then next to the executable, then `PATH`.
- Multi-hop routes may not turn a still format into video or audio; only a direct step can.
- Tests that need FFmpeg, PDFium or LibreOffice belong in engine crates and must skip cleanly when the tool is missing.
- `convt-core` stays free of native and platform dependencies. Platform code lives in engines, the app, or `integrations/`.
- Every client (CLI, app, Finder extension, Linux menus, worker) gets conversions from `convt_engines::default_registry()`. Do not add a conversion path that bypasses it.
- License verification is offline. Never commit a signing key; tests generate their own.

## Terms

- **Format**: an entry in `convt_core::FORMATS`, identified by `id` (`jpeg`, `mp4`). Extensions map to formats.
- **Engine**: one backend that declares direct `steps()` between formats.
- **Route**: the chain of steps the registry picks, at most 3 hops.
- **Target**: a format a given input can reach. `convt targets` and the context menus list targets.

## Testing

Project skills live in `.agents/skills` (`.claude/skills` links there). Read the matching one before testing:

- `test-convt-cli`: engines, formats, routing, the CLI, and `integrations/linux`. Most changes start here.
- `test-convt-desktop`: anything in `crates/convt-app`.
- `test-convt-web`: anything in `apps/web`.
- `test-convt-server`: `crates/convt-server` and `crates/convt-worker`.

`crates/convt-ffi` builds anywhere (`cargo build -p convt-ffi`), but the Finder extension in `integrations/macos` needs Xcode and has no Linux test path; say so instead of claiming it works. `integrations/windows` is a plan only.

Verification standard:

- Prove behavior with a real run plus inspected output, not only a passing build. Run `cargo test` and `bun run rs:check` for Rust changes, and `bun run check && bun run check-types && bun run build` for web changes.
- A routing or format change affects every client. Check `convt targets` for the formats around your change, including that nothing new is offered that should not be.
- Report blocked checks plainly: a missing engine, missing system libraries, or a GUI or browser check you had no permission to run.
- Launching the desktop app or a browser needs the user's explicit permission in the current request. The skills say what to do without it.

## Parallel work

Several agents may share this machine. Use temporary directories from `mktemp`, pick free ports, track the PIDs you start, and kill only those. Never `pkill` by name. Each worktree has its own `target/` and `vendor/pdfium`; run `bash scripts/fetch-pdfium.sh` in a new worktree.
