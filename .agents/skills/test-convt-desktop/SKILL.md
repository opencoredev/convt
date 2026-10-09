---
name: test-convt-desktop
description: Run the headless window tests for the convt GPUI desktop app (crates/convt-app), and build, launch and screenshot it under Xvfb or natively on macOS and Windows. Use when changing convt-app, its window, views or styling, or when asked to show or verify the desktop UI.
---

# Test the convt desktop app

`crates/convt-app` is a GPUI app built on the `gpui-kit` crate. One process runs per user; a second launch forwards its request to the first over a socket in the runtime directory and exits. The look follows the Paper design (file "convt", page "Desktop app"): Inter and Geist Mono are bundled in `crates/convt-app/assets/fonts`, the light and dark palettes live in `src/ui/theme.rs`, and the theme follows the system appearance. `theme.rs` is the one visual system every window uses: the `space`, `radius` and `size` scales, `Button`, a pill in four looks (primary: ink, light ink in dark mode; secondary: soft gray; ghost; brand: the green, only for buying or a trial) and three heights (`small` 28px, regular 32px, `large` 42px for onboarding), with an optional Hugeicons icon; `choice` and `choice_tile` for anything picked from a set (a 2px green ring marks the pick); `pill_track` for tabs and segmented controls; `glow`, a faint version of the onboarding glow for the empty Activity page, the License status and the popover header; `badge`, `callout`, `card`, `group` and `row` for grouped settings, and the brand `mark` and `lockup` drawn from `design-assets/brand`. Build new UI from those instead of one-off colors and sizes, and add any new icon to `the_icons_the_windows_draw_are_bundled`. Windows:

- The main window (1040x640, title "convt"): a sidebar with Activity, Automations and Settings, plus a trial card in licensed builds. Activity lists running jobs and history in one list, with Add files, which opens Quick convert for the picked files (as a right-click without a target does). Dropping files on the window does the same. Stop cancels a running job; Retry runs a failed or cancelled one again with the options and output folder it had, which history keeps.
- Quick convert (600x680, 600x704 on macOS where it draws its own title bar; title "Convert"): format cards, quality and size options, Codec (H.264 or HEVC, for MP4, MOV and MKV) and Keep audio for video, Background for images (Transparent where the format keeps it, White, Black, or a preset's color; ids `background` and `background-<id>`), Save to and File name. A preset sets the controls; Balanced and Original clear what it set. It opens for files sent without a target (`convt-app <files>`, `convt-app open -- <files>`, "More options…" in the file manager menus, "Open With Other Application"), for every `convt://convert` link, and when a request with a target can't run silently (a file that can't be converted, an unknown preset, or a license that stops conversions). Video files show a frame as their thumbnail, here and in Activity (see "Video thumbnails").
- The document pack (`src/pack.rs`, `src/ui/pack.rs`, design in `docs/document-pack.md`): documents (Word, Excel, PowerPoint, OpenDocument, RTF, text, HTML, CSV) need LibreOffice. When nothing can convert a document, Quick convert shows a "Document support isn't installed" card with the pinned size and one Download button instead of format cards; Add files, drops, the popover and a right-click `open --to` on a document all end up there, and a silent convert never downloads. The card follows the download (progress, Cancel), then the checksum and the install; on success the registry is rebuilt, document targets appear and the target a right-click asked for is picked, but converting still takes Convert. Failures read "Couldn't download document support" (Try again), "The download didn't check out" (checksum) or, for a pack that discovery rejects, "Document support needs reinstalling" with the reason in plain words and the engines' message under it. Settings → General → Documents shows the status, Download, and Remove (asks first, waits for document jobs). A build with no pin (any source build without the `CONVT_DOCUMENT_PACK_*` variables) says it has nothing to download and offers no button. Status checks are offline; the only network request is the Download click.
- Settings (620x600): General, Presets and License tabs. "Jobs at once" defaults to Auto, shown as "Auto (N)" with N from the CPU count.
- Onboarding (title "Welcome to convt", `src/ui/first_run.rs`): three quarters of the display it opens on, at least 900x640 where that fits (960x675 on a 1280x900 Xvfb), over a dithered green glow (`assets/onboarding`). The account screen first: Continue with Google, Continue with Email, "I have a license key", then whatever the account allows (`Stage`): "You have convt Pro", "Your free trial is on" with the end date, "Start your 7-day free trial", "Your Pro plan has ended", or "Couldn't reach convt.app" with Retry. Then one question per screen, only when it applies: "Convert PDFs and documents too?" (a pinned pack and no working document engine; Yes starts the download), and "Add convt to Finder?" (macOS). Last, "Setting up convt" (`onboarding-title`): a thin green arc turning around the mark (`assets/onboarding/spinner-*.svg`, from `generate.py`) and that one line, nothing else. It lasts 2.3 s with its fade (`CALIBRATE`) and never waits for the download, which it doesn't show: Activity shows `activity-pack` with its progress, or the failure with `activity-pack-settings` (Try again in Settings). It shows until that moment ends (`first_run_done`), and only in builds that check licenses. After that, macOS Activity shows a recover card until the Finder extension is on. Ids: `onboarding-google`, `onboarding-email`, `onboarding-key-link`, `onboarding-primary` (Continue, Start free trial, Get convt Pro), `onboarding-reopen`, `onboarding-cancel`, `onboarding-retry`, `onboarding-not-now`, `account-status`, `question-yes`, `question-no`, `onboarding-title`. Signed-in and license addresses show masked everywhere (`account::masked_email`, `l***@gmail.com`, as convt.app's `maskEmail`), so tests use plain identities and read `***`; `addresses_show_masked` builds a real-looking address at runtime.
- The tray icon (`src/tray.rs`): the menu bar item on macOS (`tray-icon`, the colored mark), the notification area icon on Windows (`tray-icon`) and a StatusNotifierItem on Linux (`ksni`, no GTK). Its menu: Open convt, Settings…, Quit convt; a left click on Windows and Linux opens convt; the tooltip reads "convt: converting N files" while jobs run. While "Keep running in the background" (Settings, General, id `menu-bar-icon`, key `menu_bar_icon`) is on and the icon is up, closing the last window keeps the app running (on macOS it also leaves the Dock). Without an icon (the setting off, or a Linux desktop with no tray host) the app quits with its last window, or once its jobs finish. Quit is ⌘Q on macOS and Ctrl+Q elsewhere, from any window (bound in `src/ui/menus.rs`; every quit goes through `menu::quit`, which waits for an update install). Tests in `ui/tests/background.rs` give the app a fake icon through `tray::init` and count quits with `menu::Quits`, because the test platform's quit does nothing.
- The menu bar popover (340x520): drop bar (converts to the `defaults` in `settings.toml`, photos to JPEG and other stills to PNG, and copies the result to the clipboard), running jobs, automation switches, Open convt and Settings. The tray shows a native menu instead, so only tests open it (`ui::open_popover`).

Requests come from `convt-app [files...]`, `convt-app open [--to <fmt>] [--preset <name>] [--] <files...>`, `file://` URIs from file managers, `convt://convert?file=<path>&to=<fmt>` links, `convt://activate?key=<key>` links (open the License tab with the key filled in, never activate without a click) and `convt://auth?state=<state>&code=<code>` links (the browser's answer to a desktop sign-in; see "Sign-in and renewal"). A command-line request with `--to`, or a preset that names a format, converts in place with no window: the output goes next to the original whatever Settings says, no notification is shown and nothing is revealed, and the app quits when the batch is done if nothing else is open and no tray icon keeps it running. Links never convert without a click.

Licensing: a build from source needs no license and never shows onboarding or a trial card. With the check on, conversions need a license key or an account trial; there is no local trial any more (`model.rs` calls `disable_local_trial` outside `cfg(test)`, so the app tests still have the old local trial unless a test turns it off). The states (`convt_license::client::State`): `AccountTrial` (the sidebar card says "Pro trial", the License tab "Pro trial: N days left."), `SignInNeeded` ("Sign in to start your free trial."; the sidebar's Sign in and Quick convert's `sign-in-banner` open the License tab), `Licensed`, `NotCovered`, and `TrialEnded` only for a computer that started the old local trial. Quick convert shows why conversions stopped, and the License tab has the key field, Activate and Remove license. `convt-app --help` prints the usage without opening a window.

Example automation rules live in `src/placeholder.rs` (screenshots → PNG with copy, screen recordings → MP4, HEIC → JPEG off). Each enabled rule watches **one** directory, never recursively: macOS screenshots use `com.apple.screencapture location` (and skip the watch when `target` is clipboard); recordings use the recording location or Desktop. Tests must set `Automation.folder` or `CONVT_SCREENSHOT_DIR` / `CONVT_RECORDING_DIR` — the poller never opens the real Desktop under `cfg(test)`. Drive it with `AppState::poll_automations` (twice after writing a file, so the size can settle). A checkbox per rule copies the result; the tray drop bar still copies independently.

Pack tests use `TestPacks`, a scripted `pack::Backend` (`Fixture::with_packs`), never the real one: it counts installs and removes, holds a download mid-way (`hold`), fails as told (`fail_next`), and its registry adds `TestOffice`, a fake document engine, once "installed", so results don't depend on this machine's LibreOffice. Assert `packs.installs() == 0` before every click on `pack-download`; `only_the_download_button_reaches_the_installer` checks in the source that nothing else reaches `install_documents`. Ids: `pack-card` (its label is the title), `pack-body`, `pack-detail`, `pack-download`, `pack-cancel`, `pack-progress`, `pack-done`, and in Settings `pack-status`, `pack-remove`, `pack-remove-confirm`, `pack-remove-keep`. `open_quick` records the newest Quick convert window, so `window_of::<QuickView>` finds what the app opened.

Not testable in the headless tests: the system pickers ("Choose a folder…", Change, Add files' picker) and "Open presets folder"; the real tray icon and its menu (under Xvfb, a private bus has no StatusNotifierWatcher, so the app logs "no tray icon" and quits with its last window; check the icon on a desktop with a tray host, such as KDE, or GNOME with the AppIndicator extension); notifications on a real desktop; how the bundled fonts render (the test platform has no real text shaping). A launched app under Xvfb covers the pickers (through `scripts/fake-portal.py`), fonts and drag-and-drop; see below.

## Sign-in and renewal

Packaged builds start with sign-in (`src/account.rs`, `src/ui/first_run.rs`, `src/ui/account.rs`, the shared client in `crates/convt-license/src/account.rs`). Continue with Google or Email in onboarding, and "Sign in with convt.app" in Settings, License, open `<site>/device?state=&challenge=&name=&os=&version=` once, with `&provider=google` or `&provider=email` from onboarding's buttons, and wait. With `google` the page goes straight to Google and back; with `email` it shows its email step, focused. The site answers with `convt://auth?state=...&code=...` (or `&error=...`), which counts only while the app waits with that state; a link the app didn't start, a replay, a cancelled flow's link or one older than 15 minutes is dropped with no network call and a notice. The app trades the code and verifier for a device token, stores `{email, token}` as `account.json` next to `license.key` (or in the credential store), and asks `/api/device/license`, whose `access` says what the account allows: `pro` (with the key), `trial` (`ends_on`, `ends_at`), `can_start_trial` (`checkout_url`, the site's `/checkout/pro`) or `lapsed`.

Start free trial opens that checkout (`/checkout/pro?from=app`, so the site's success page sends the buyer back to the app) and polls `/api/device/license` every 2 s for the first minute, every 10 s to two minutes, then every 60 s, 120 s after a 429, and stops after 15 minutes, on sign-out, or once the answer is a trial or Pro (`trial_poll_delay`; about 50 asks, under the site's 90 an hour per device). Onboarding coming to the front while the checkout is open asks at once (`check_trial_on_focus`, at most every 2 s). `the_trial_checkout_is_polled_fast_and_rechecked_on_focus` covers both. A trial is never a key: the app keeps its exact end as `[trial_cache]` `ends_at` and `fetched_on` in `settings.toml` and honors it at launch, offline included, only while signed in, with `fetched_on` not after today and `ends_at` no more than 8 days after `fetched_on` (`cached_trial_is_valid`); a forged far-future cache is ignored and conversions need sign-in again. Sign out, a revoked device (401 on refresh) and any non-trial answer clear the cache. While signed in, the app asks once a UTC day at launch (`license_checked`), on Refresh license, and from onboarding when the launch check already ran today. Together with the update check below, these are the only network calls the app makes without a click. Ids in Settings: `sign-in`, `sign-in-reopen`, `sign-in-cancel`, `account-status`, `account-notice`, `refresh-license`, `refresh-status`, `sign-out`, `refresh-note`, and in General `network-updates`, `network-refresh`, `network-other`.

Tests never reach the network: the `Fixture` gives the app a scripted `TestApi` (`f.api`) that counts exchanges, renewals and sign-outs, so assert `f.api.calls()` to prove a link or launch called nothing. `Fixture::signed_in(cx, key, email)` starts signed in. `AppState::age_sign_in` makes a waiting flow look old. Sign Pro keys with `pro_key`; read the opened page's state with `query(&cx.opened_url().unwrap(), "state")` and answer with `auth_link`. Set a trial with `s.licensing.set_account_trial_exact(...)` and `s.license = s.licensing.state()`.

In a real run, launch with `CONVT_LICENSE_ENFORCE=1`, `CONVT_LICENSE_PUBKEY` from `.convt-dev/license.pub`, a fresh `CONVT_CONFIG_DIR` and `CONVT_DATA_DIR` for a first run, and `CONVT_ACCOUNT_URL` at the dev server from `test-convt-web` (plain HTTP only to loopback). Put a fake `xdg-open` first in `PATH` that appends its argument to a file: GPUI opens URLs with `xdg-open`, so that file holds the device page and checkout URLs. Open them in `agent-browser` with the real `HOME` and in a shell that hasn't sourced the app's private `HOME` (agent-browser looks for Chrome under `HOME`). For a new account through Google, append `&identity=google-gmail` and an `&email=` address at gmail.com to the mock's authorize URL; other domains aren't verified and go through /sign-in/verify-email. Stub `HTMLAnchorElement.prototype.click` to record the `convt://auth` link, click Approve, and deliver the link as the desktop file's `%U` handler would: `./target/debug/convt-app '<link>'` with the same environment. A new account then shows "Start your 7-day free trial"; Start free trial opens `/checkout/pro`, which redirects to the billing mock's card form, and its Start trial button turns into "Your free trial is on" within about 10 s. For the email path read the code from Mailpit. To see the documents question on a machine with LibreOffice, use a pack-pinned build (below) with `PATH` limited to `ffmpeg`, `ffprobe` and the fake `xdg-open`; copy `account.json` into another fresh config to skip signing in again. Offline is a relaunch with `CONVT_ACCOUNT_URL` at a closed port and an older `license_checked`. Check `$CONVT_CONFIG_DIR/account.json`, `license.key`, `settings.toml` and the `devices` row (`bash scripts/db.sh psql owner`); revocation is "Sign out <name>" under Macs on the dashboard, then Refresh license. Never `xdotool windowkill` a GPUI window: it kills the whole app.

## Update check

`src/update.rs` and `src/ui/update.rs`. While automatic update checks are on (Settings, General, `update-checks`; on by default) the app fetches the signed manifest at every launch and every 5 hours while running (`CHECK_INTERVAL`; the schedule looks every `SCHEDULE_TICK`, so tests drive it with `advance_clock` and an older `update_attempted`). Check now (`check-updates`, "Checking…" while it runs) and the macOS menu's Check for Updates… fetch whether or not automatic checks are on. The app verifies the manifest with `convt-update`, keeps the highest accepted `update_sequence` and the time of the last answered check (`update_checked_at`) in `settings.toml`, and selects the newest build the license covers. It shows `update-card` in the main window's sidebar ("Update available" with `update-download`, or "New version" with `update-renew`). Settings shows the Updates card: `update-version`, `update-last-checked`, `check-updates`, then `update-status` (with `update-notes` and `update-download` or `update-renew` when a newer build is out), then the switch. Failed checks appear only in `update-status`; a covered update on an install that replaces itself downloads in the background, below.

The menu bar (`src/ui/menus.rs`) is installed only on macOS; its actions are registered everywhere, so tests call `menus::init`, dispatch actions such as `menus::CheckForUpdates` with `cx.dispatch_action`, and read the structure with `cx.set_menus(menus::menus())` and `cx.get_menus()`. Handlers run deferred, because the key window is busy while it dispatches. `menus::init` binds Quit and Close Window (`secondary-q`, `secondary-w`: ⌘ on macOS, Ctrl elsewhere) on every platform; for the Mac-only shortcuts also `cx.bind_keys(menus::mac_key_bindings())`. Drive them with `cx.simulate_keystrokes`. GPUI's test platform makes `cx.quit()` a no-op, so to see that ⌘Q quits, register a second `cx.on_action::<menus::Quit>` after `menus::init` that counts and calls `cx.propagate()`, as `quit_and_close_window_shortcuts_work_on_every_platform` does. The test platform also keeps reporting a removed window as active, so dispatch window-less actions before opening any window. About convt is `AboutView` (`about-version`, `about-site`, `about-notes`, `about-source`). Linux has no app menu bar, so the menus themselves can't be seen without a Mac.

When the install can replace itself (`UpdateConfig.install`: the disk image on macOS, the MSI on Windows, an AppImage with `$APPIMAGE` on Linux; `src/update/install.rs`), a covered update downloads in the background into `<data dir>/updates/<version>` and must match the manifest's size and SHA-256 (`src/update/download.rs`). The card goes "Downloading convt X… N%", then "convt X is ready" with `update-restart` (Restart to update), which installs, starts the new version once the app has quit, and quits. A failed download or install shows `update-retry` (Try again) and `update-download` (Download instead). Restart waits while conversions run or document support downloads, installs or is removed, and pack work is refused while it installs. A verified download keeps its manifest as `.manifest.json`, so a relaunch shows Restart to update again after checking both offline; on Windows the helper writes `install-result.json` in the updates folder and the next launch shows a failed msiexec run as InstallFailed. Tests simulate a relaunch with `forget_updates` and `start_update_checks`. Other installs show "Update available" with `update-download`, which opens the download page; an uncovered build shows "New version" with `update-renew`.

Tests give the app a scripted `TestReleases` (`f.releases`) that counts fetches; sign manifests with `manifest(sequence, &[(version, date)], &update_key())`, or `manifest_for(..., bytes)` to name an installer's size and hash. `update_key()` must stay different from `test_key()`. `f.self_installing(cx)` turns on self-update with a scripted host (`TestDownloads`: counts requests, `hold` stops a download half-way) and a `TestInstaller` that records what it would install, so tests never download or replace anything real. `update::tests::live_appimage_downloads_and_verifies` is `#[ignore]`d: it fetches the live manifest and downloads and checks the real Linux AppImage (pass the production key as `CONVT_LIVE_UPDATE_PUBKEY` in a build without one).

In a real run, a build from source takes `CONVT_UPDATE_URL` (plain HTTP only to loopback) and `CONVT_UPDATE_PUBKEY`. Serve a manifest from a temp directory with `python3 -m http.server --bind 127.0.0.1 0`, signed with a throwaway Ed25519 key outside the repo (payload is base64url JSON; the signature covers `convt-update-v1\n` followed by the payload text). Manifest build dates may not be later than its `issued_at` day, and the running build's date is the day it was built, so to show a newer covered build, build a separate binary with `CONVT_BUILD_DATE` set a few days back in its own `CARGO_TARGET_DIR`. Use your own license key pair too and pass its public half as `CONVT_LICENSE_PUBKEY`, so you never need `.convt-dev/license.key`. To check again, relaunch or click Check now.

## Headless tests first

`crates/convt-app/src/ui/tests.rs` opens real windows on GPUI's test platform, clicks elements by id and runs real conversions through `default_registry()`. They need no display and no permission, so run them for every app change:

```sh
cargo test -p convt-app
```

When adding a test, follow the existing `Fixture`:

- It points settings, history and presets at a `TempDir`, so tests never touch the user's config, and calls `theme::init`.
- `Fixture::new` is a build from source with no license check. `Fixture::licensed(cx, trial_start, key)` turns the check on with a test signing key, keeps the trial file and the key file in the `TempDir`, and never reaches the OS keyring. Sign test keys with `license_key`.
- It calls `cx.executor().allow_parking()` before anything else. Conversions run on real job threads; without it the test scheduler panics with "Detected activity on thread ... not deterministic".
- Controls are plain elements built by `ui/theme.rs` (`clickable`, `primary_button`, `text_button`, `switch`, `checkbox`, `select`, `segmented`). Each registers its `id` for test queries; find it with the helpers (`click`, `label`, `toggled`, `shown`). A switch's label is its setting's name and its state is `toggled` (`Some(true)` for on); a dropdown's options have the ids `{select id}-{option}` and only exist while it is open, so click the select first.
- `f.main(cx)`, `f.quick(request, cx)` and `f.settings(tab, cx)` open a window at a roomy test size. To test what the app itself opens, call `super::route(request, cx)` (or `show_main`, `show_settings`, `open_popover`) and get the window with `window_of::<View>(cx)`. Those open at their real sizes; `windows_fit_their_content_at_their_opening_sizes` checks that the main buttons are not cut off, so extend it when a window grows.
- Silent conversions open no window, so wait on app state with `wait_until`. Wait on a window's label with `wait_for_label`. Both poll with a timeout, not a fixed sleep.
- `theme::set_dark(true, cx)` switches to the dark palette; `every_window_renders_in_both_themes` opens each window in both.
- Text fields go through `theme::field` and `theme::small_field`, never a bare `Input`: the component sizes its height in rems but pads in pixels, so at convt's 13px rem a bare field cuts off descenders. `text_fields_have_room_for_a_whole_line` checks the heights. Note that `Input::h` is the multi-line height; use `Styled::h`.
- Icons (checkmarks, chevrons) come from `ui::assets()`, which `main.rs` registers with `with_assets`. The test platform draws nothing either way, so `the_icons_the_windows_draw_are_bundled` checks the asset source instead.
- `cx.opened_url()` returns the last URL a button opened, `cx.read_from_clipboard()` what was copied, and `cx.shown_system_notifications()` the notifications posted (the fixture sets the app identity they need). GPUI's test platform can't reveal files, so in tests `AppState::revealed` records what would have been revealed.

Request parsing (`request.rs`) and the single-instance handoff (`instance.rs`) have plain unit tests in the same crate.

## Permission

Launching the app, even under a private Xvfb display, needs the user's explicit permission in the current request. Asking to test, verify or fix the app does not count. Without permission, build it, run `cargo test` and clippy, and report the visual check as pending with the exact launch steps below. With permission, prefer Xvfb; driving a real desktop session or another person's machine needs that named specifically.

## Build

convt-app is excluded from `default-members`, so `cargo build` alone skips it.

```sh
cargo locate-project --workspace --message-format plain   # must be this checkout
cargo build -p convt-app
```

On Linux the build needs the GPUI system libraries (xkbcommon, fontconfig, wayland, X11/xcb, vulkan). `bash scripts/setup-linux.sh` installs them and needs sudo. Without sudo, two gaps are common and can be bridged outside the repo when the runtime libraries are installed:

```sh
# fontconfig.pc missing: load fontconfig at runtime instead of linking it.
export RUST_FONTCONFIG_DLOPEN=1
# "unable to find library -lxkbcommon-x11": only the unversioned .so link is missing.
mkdir -p ~/.cache/convt-linklibs
ln -sf /usr/lib/x86_64-linux-gnu/libxkbcommon-x11.so.0 ~/.cache/convt-linklibs/libxkbcommon-x11.so
export LIBRARY_PATH=~/.cache/convt-linklibs
```

For any other missing library, stop and report it; do not vendor system headers into the repo. macOS needs Xcode with the Metal toolchain (`scripts/setup-macos.sh`); Windows needs the MSVC build tools (`scripts/setup-windows.ps1`).

Rebuild after any change under `crates/`. A Rust-only edit elsewhere in the workspace that convt-app does not depend on can reuse the existing binary.

## Launch headless on Linux

GPUI picks X11 when `DISPLAY` is set and `WAYLAND_DISPLAY` is empty. Under Xvfb it renders through Vulkan on llvmpipe (the log says `Selected GPU adapter: "llvmpipe"`), which is fine for layout and fonts. Inter renders, not a fallback. The log always has `Found no xinput mouse pointers` and `Could not open device /dev/dri/renderD128`; both are harmless. For the real AMD GPU, use the headless sway recipe in the user's memory (`gpuix-linux-gpu-testing`) through `sg render`; it is not needed for layout checks.

The theme follows the `color-scheme` key of the XDG desktop portal's Settings interface (`org.freedesktop.appearance`): 1 is dark, anything else is light. GPUI reads it once at startup and then follows `SettingChanged`. Bare Xvfb has no portal, so the app shows light. Run a private D-Bus session bus with `scripts/fake-portal.py`, which answers the color scheme from a file and flips it live, and also answers the file chooser, so Add files and "Choose a folder…" work. The script refuses to start without `--private-bus`, on the user's own bus (`/run/user/<uid>/bus`), on a bus whose address and daemon PID don't match the `bus` and `buspid` files the recipe writes, or when something already owns the portal name. Keep those checks; the recipe below also stops if the private bus didn't come up, so nothing ever reaches the user's real portal.

Let Xvfb choose a free display, and track every PID you start:

```sh
work=$(mktemp -d /tmp/convt-app.XXXXXX); mkdir -m 700 "$work/run"
skill=.agents/skills/test-convt-desktop
Xvfb -displayfd 3 -screen 0 1280x900x24 3>"$work/display" >/dev/null 2>&1 &
echo $! >> "$work/pids"
for _ in $(seq 50); do [ -s "$work/display" ] && break; sleep 0.1; done
# Services the bus starts on demand (dconf, tracker, gvfs) inherit the daemon's
# environment, so make HOME and the XDG dirs private before starting it.
# Build first: cargo and rustup need the real HOME.
mkdir -p "$work/home" "$work/xdg-config" "$work/xdg-data" "$work/xdg-cache"
export HOME="$work/home" XDG_CONFIG_HOME="$work/xdg-config" XDG_DATA_HOME="$work/xdg-data" XDG_CACHE_HOME="$work/xdg-cache"
dbus-daemon --session --fork --print-address=3 --print-pid=4 3>"$work/bus" 4>"$work/buspid"
cat "$work/buspid" >> "$work/pids"
# Stop here unless the private bus is up; never fall back to the user's bus.
case "$(cat "$work/bus")" in
  unix:path=/tmp/*) ;;
  *) echo "no private bus; stopping" >&2; kill $(cat "$work/pids"); exit 1 ;;
esac
export DISPLAY=:$(cat "$work/display") WAYLAND_DISPLAY= DBUS_SESSION_BUS_ADDRESS=$(cat "$work/bus")
echo 2 > "$work/scheme"    # 1 dark, 2 light
python3 "$skill/scripts/fake-portal.py" --private-bus "$work" >"$work/portal.log" 2>&1 &
echo $! > "$work/portal.pid"; echo $! >> "$work/pids"
sleep 1; kill -0 "$(cat "$work/portal.pid")" || { cat "$work/portal.log"; kill $(cat "$work/pids"); exit 1; }
# Keep the app's state out of the user's config and away from a running instance.
export CONVT_RUNTIME_DIR="$work/run" CONVT_CONFIG_DIR="$work/config" CONVT_DATA_DIR="$work/data"
export XDG_RUNTIME_DIR="$work/run" CONVT_LICENSE_STORE=file
./target/debug/convt-app >"$work/app.log" 2>&1 &
echo $! > "$work/app.pid"; echo $! >> "$work/pids"
```

Switch the theme of the running app with `echo 1 > "$work/scheme"; kill -USR1 "$(cat "$work/portal.pid")"`. To pick files, write their paths to `$work/pick`, one per line, then click Add files; `$work/pick.log` records each request, including whether it asked for folders.

Without `CONVT_RUNTIME_DIR` a launch can find another agent's or the user's running convt-app and hand its request to that process instead of starting. With the same variables, `./target/debug/convt-app open -- <file>` opens Quick convert in the running app, `open --to webp -- <file>` converts in place with no window, and `"convt://convert?file=<path>&to=mp3"` opens Quick convert with MP3 picked. Use synthetic inputs from `.agents/skills/test-convt-cli/scripts/fixtures.sh`. A slow job for Stop needs a large input, such as a 40-second 1080p `testsrc2` clip converted to WebM.

To see onboarding, the trial card and the License tab, also set `CONVT_LICENSE_ENFORCE=1` and `CONVT_LICENSE_PUBKEY`, with their own config, data and runtime directories. The `dev-keys` example (see the Licensing section of `test-convt-cli`) writes `.convt-dev/` in the workspace root, which other agents may share: copy the public key and the keys you need into `$work`, then delete `.convt-dev` only if it is the one you made. Onboarding shows on the first plain launch with a fresh config directory; without a reachable `CONVT_ACCOUNT_URL` it stops at sign-in, so run the dev server for anything past that (see "Sign-in and renewal"). A key whose `updates_until` is before the build date is saved but shows the "newer than your license covers" status and Renew.

If you wrap the launch in a shell function, make it `exec` the app, or `$!` is the subshell's PID and the app outlives your `kill`.

Reveal ("Reveal it in the file manager", Show) runs `xdg-open`, which starts Nautilus on your Xvfb display. Turn Reveal off in Settings first, or kill those processes by PID afterwards (find them by `DISPLAY` in `/proc/<pid>/environ`).

Readiness: the process is still alive after a few seconds (`kill -0 "$(cat "$work/app.pid")"`) and `app.log` has no panic.

"Jobs at once" shows Auto with the number Rust's `available_parallelism` returns, which honors the cgroup CPU quota. On this server that is 28 of 32 cores, so "Auto (28)" is correct here.

### Document pack in a real run

A source build has no pinned pack. To see the whole flow, build a separate binary pinned to a local test pack, in its own target directory so `target/debug` stays a normal build for everyone else (a full GPUI build takes a few minutes):

```sh
# A pack like the engines' tests make: a soffice launcher, here handing off
# to the system LibreOffice, plus padding so the download lasts long enough
# to watch.
python3 - "$work" <<'PY'
import io, os, sys, tarfile
work = sys.argv[1]
launcher = b'#!/bin/sh\nPATH=/usr/bin:/bin exec /usr/bin/soffice "$@"\n'
with tarfile.open(f"{work}/documents.tar.gz", "w:gz", compresslevel=1) as tar:
    info = tarfile.TarInfo("soffice"); info.mode = 0o755; info.size = len(launcher)
    tar.addfile(info, io.BytesIO(launcher))
    pad = os.urandom(700 << 20)
    info = tarfile.TarInfo("share/padding.bin"); info.size = len(pad)
    tar.addfile(info, io.BytesIO(pad))
PY
pack="$work/documents.tar.gz"; chmod 600 "$pack"
CARGO_TARGET_DIR="$work/target" CONVT_DOCUMENT_PACK_URL="file://$pack" \
  CONVT_DOCUMENT_PACK_SHA256=$(sha256sum "$pack" | cut -d' ' -f1) \
  CONVT_DOCUMENT_PACK_SIZE=$(stat -c %s "$pack") CONVT_DOCUMENT_PACK_VERSION=test \
  cargo build -p convt-app
```

Use `file://` only: loopback HTTP works in the engines' unit tests alone, and never point a build at a real host. Launch `$work/target/debug/convt-app` as below, but with `PATH` limited to a folder holding `ffmpeg` and `ffprobe` symlinks, so the system LibreOffice in `/usr/bin` doesn't make documents work already. `CONVT_DATA_DIR` keeps the installed pack in `$work`. Then `open -- report.docx` shows the card. A local file downloads in about a second, so to capture the download in progress, `kill -STOP` the app a moment after clicking Download, capture, and `kill -CONT`; the X framebuffer keeps the last frame. Moving `$pack` away before clicking makes the download fail like an offline one; `chmod 500` on the pack folder gives the permission error; changing the archive's bytes gives the checksum error; `chmod g+w` on the installed pack directory makes discovery reject it. For a full disk without root, preload a small shim that fails `write` with `ENOSPC` on `*.partial` files (`LD_PRELOAD` on the app only). Keep `CONVT_DATA_DIR` and its parents private (0700): the installer refuses group-writable ancestors, and a `mkdir -p` under umask 002 makes them 0775.

### Video thumbnails

Video rows and the Quick convert header show a frame from about a second in, grabbed by `src/thumbs.rs` with the FFmpeg the engines find (`convt_engines::ffmpeg::ffmpeg_path`), on one worker thread, and cached in memory, re-checked for changes on the worker. Rendering only reads the cache: metadata, the mount check and FFmpeg all run on the worker. The cache and its queue are bounded, and FFmpeg is killed when the app quits (and, on Linux, by the kernel if the app dies). Network shares (NFS, SMB, sshfs, GVfs; on Windows UNC paths and mapped network drives) keep the extension badge, as do videos FFmpeg can't read and any video until its frame is ready; local disks and USB drives get frames. The unit tests in `thumbs.rs` make a fade-in clip and check that the frame isn't black. The Windows and macOS mount checks are unverified.

### Title bar under GNOME on Wayland

Under bare Xvfb GPUI uses X11 and server-side decorations, and with no window manager there is no title bar at all; that shows nothing about the drawn one. GNOME's compositor has no server-side decorations, so on Wayland GPUI falls back to client-side ones and `src/ui/chrome.rs` draws the title bar. To see it, run GNOME Shell nested on your Xvfb display and the app as its Wayland client, after the Xvfb and private-bus steps above:

```sh
MUTTER_DEBUG_DUMMY_MODE_SPECS=1360x960 gnome-shell --nested --wayland --no-x11 --wayland-display convt-wl >"$work/shell.log" 2>&1 &
echo $! >> "$work/pids"; sleep 6
xdotool mousemove 700 500 click 1; xdotool key Escape   # leave the overview
WAYLAND_DISPLAY=convt-wl DISPLAY= ./target/debug/convt-app >"$work/app.log" 2>&1 &
echo $! > "$work/app.pid"; echo $! >> "$work/pids"
```

Capture the whole nested screen with `x11grab` at `$DISPLAY+0,0`; xdotool input reaches the nested windows. The shell starts the real `xdg-desktop-portal` on the private bus, and it has no Settings interface there, so the app stays light: kill that portal (match its `DBUS_SESSION_BUS_ADDRESS` to your bus first), start `fake-portal.py`, and relaunch the app for dark. GNOME won't raise a running window for a second launch (it shows "convt is ready" instead), so relaunch rather than un-minimize. Check dragging the bar, double-clicking it, the right-click window menu, the buttons and edge resizing. A compositor with server-side decorations (sway) must show its own bar and none from convt.

### Drive it

Use `xdotool`. Find windows with `xdotool search --name ''` and `getwindowname`, and their position with `getwindowgeometry --shell`; GPUI windows open at fixed spots (main window at 122,130, Settings at 332,150, Quick convert at 342,90, onboarding at 162,113 on a 1280x900 screen). `xdotool type` fills text fields. Scroll with button 5. Move the pointer to a corner before a screenshot so no hover state shows.

Drag-and-drop works with a real drag source: start Thunar on the same display (with a private `HOME`), then `mousedown`, move in small steps onto the convt window, and `mouseup`. xdotool alone has no XDND source.

### File-manager menus

`integrations/linux/install.py` writes to `$HOME/.local/share` and `$HOME/.config`. Run it with a private `HOME`, `XDG_CONFIG_HOME`, `XDG_DATA_HOME` and a `PATH` whose first entry holds `convt` and `convt-app` symlinks to `target/debug`, and pass `--thunar --nemo --dolphin --nautilus` explicitly. Thunar is installed here: start it with the same private variables on your Xvfb display, right-click a file, and pick "Convert with convt", or "Open With Other Application" and convt. `convt-app.desktop` claims no file types (only `x-scheme-handler/convt`) and writes nothing to `mimeapps.list`, so convt never becomes a double-click default; `app_entry` in `install.py` says why. Nemo, Dolphin and nautilus-python are not installed system-wide; run the real ones from a private prefix with the "Real file managers without root" recipe in `test-convt-cli`. Include file names with spaces, quotes, `%` and `$`.

## Evidence

Capture one window by its geometry:

```sh
eval "$(xdotool getwindowgeometry --shell "$wid")"
ffmpeg -v error -y -f x11grab -video_size "${WIDTH}x${HEIGHT}" -i "$DISPLAY+$X,$Y" -frames:v 1 "$work/app.png"
```

Open `app.png` with the Read tool and compare it with the Paper artboard for that window (04 for the main window, 03 Quick convert, 06 Settings, 05 onboarding), in both themes. Zoom in on text fields with `ffmpeg -vf crop=...,scale=...:flags=neighbor` to check that descenders (g, y, p) are not cut off. Check the change you made, plus the baseline: the main window shows the sidebar and Add files, and the format cards Quick convert offers for a file match `./target/debug/convt targets <file>`. For a silent `open --to` conversion, check that the output file exists next to the input and is the requested format (`file <output>`). When a UI change affects layout, also resize the window (`xdotool windowsize`) and capture again; report sizes you did not check. Quick convert and Settings scroll when their content is taller than the window.

## Cleanup

Kill only what you started, by PID, then remove the work directory unless the screenshots are still needed for review:

```sh
kill $(cat "$work/pids") 2>/dev/null
rm -rf "$work"
```

Check that nothing you started is left: no process should still have your `DISPLAY` or `DBUS_SESSION_BUS_ADDRESS` in its environment. Processes started on the private bus (gvfsd, dconf-service, tracker, xfconfd and others from Thunar or Nautilus) are yours too. Never `pkill` by name; another agent may own a convt-app or Xvfb process.

## macOS and Windows

Native launch is `cargo run -p convt-app`. Taking a screenshot of a real session drives the user's desktop, so it needs explicit permission. The Finder Sync extension in `integrations/macos` is not built by Cargo and has no runnable path on Linux; see its README for the Xcode steps.
