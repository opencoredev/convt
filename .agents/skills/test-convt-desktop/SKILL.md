---
name: test-convt-desktop
description: Run the headless window tests for the convt GPUI desktop app (crates/convt-app), and build, launch and screenshot it under Xvfb or natively on macOS and Windows. Use when changing convt-app, its window, views or styling, or when asked to show or verify the desktop UI.
---

# Test the convt desktop app

`crates/convt-app` is a GPUI app built on the `gpui-kit` crate. One process runs per user; a second launch forwards its request to the first over a socket in the runtime directory and exits. The look follows the Paper design (file "convt", page "Desktop app"): Geist and Geist Mono are bundled in `crates/convt-app/assets/fonts`, the light and dark palettes live in `src/ui/theme.rs`, and the theme follows the system appearance. Windows:

- The main window (1040x640, title "convt"): a sidebar with Activity, Automations and Settings, plus a trial card in licensed builds. Activity lists running jobs and history in one list, with a defaults bar ("Add files converts right away to Photos → JPEG, Images → PNG, …", Change) and Add files, which converts right away. Photos are HEIC, AVIF, WebP and JPEG; other stills (PNG, SVG, screenshots by name) become PNG. Dropping files on the window does the same. Stop cancels a running job; Retry runs a failed or cancelled one again with the options and output folder it had, which history keeps.
- Quick convert (600x560, title "Convert"): format cards, quality and size options, Codec (H.264 or HEVC, for MP4, MOV and MKV) and Keep audio for video, Background for images (Transparent where the format keeps it, White, Black, or a preset's color; ids `background` and `background-<id>`), Save to and File name. A preset sets the controls; Balanced and Original clear what it set. It opens for files sent without a target (`convt-app <files>`, `convt-app open -- <files>`, "More options…" in the file manager menus, "Open With Other Application"), for every `convt://convert` link, and when a request with a target can't run silently (a file that can't be converted, an unknown preset, or a license that stops conversions). Video files show a frame as their thumbnail, here and in Activity (see "Video thumbnails").
- The document pack (`src/pack.rs`, `src/ui/pack.rs`, design in `docs/document-pack.md`): documents (Word, Excel, PowerPoint, OpenDocument, RTF, text, HTML, CSV) need LibreOffice. When nothing can convert a document, Quick convert shows a "Document support isn't installed" card with the pinned size and one Download button instead of format cards; Add files, drops, the popover and a right-click `open --to` on a document all end up there, and a silent convert never downloads. The card follows the download (progress, Cancel), then the checksum and the install; on success the registry is rebuilt, document targets appear and the target a right-click asked for is picked, but converting still takes Convert. Failures read "Couldn't download document support" (Try again), "The download didn't check out" (checksum) or, for a pack that discovery rejects, "Document support needs reinstalling" with the reason in plain words and the engines' message under it. Settings → General → Documents shows the status, Download, and Remove (asks first, waits for document jobs). A build with no pin (any source build without the `CONVT_DOCUMENT_PACK_*` variables) says it has nothing to download and offers no button. Status checks are offline; the only network request is the Download click.
- Settings (620x600): General, Presets and License tabs. "Jobs at once" defaults to Auto, shown as "Auto (N)" with N from the CPU count.
- First run (420x420): the Finder step (macOS only), "Start 7-day trial" or "I have a key", and a last step. The last step says photos become JPEG and screenshots and other images become PNG. It shows until the last step is finished, and only in builds that check licenses. Closing mid-setup shows it again. After that, macOS Activity shows a recover card until the Finder extension is on. The Finder step's System Settings picture is a preview (id `finder-preview`), not a switch.
- The tray icon (`src/tray.rs`): the menu bar item on macOS (`tray-icon`, template image), the notification area icon on Windows (`tray-icon`) and a StatusNotifierItem on Linux (`ksni`, no GTK). Its menu: Open convt, Settings…, Quit convt; a left click on Windows and Linux opens convt; the tooltip reads "convt: converting N files" while jobs run. While "Keep running in the background" (Settings, General, id `menu-bar-icon`, key `menu_bar_icon`) is on and the icon is up, closing the last window keeps the app running (on macOS it also leaves the Dock). Without an icon (the setting off, or a Linux desktop with no tray host) the app quits with its last window, or once its jobs finish. Quit is ⌘Q on macOS and Ctrl+Q elsewhere, from any window (`src/menu.rs`, which also sets the macOS menu bar menus). Tests in `ui/tests/background.rs` give the app a fake icon through `tray::init` and count quits with `menu::Quits`, because the test platform's quit does nothing.
- The menu bar popover (340x520): drop bar (converts and copies the result to the clipboard), running jobs, automation switches, Open convt and Settings. The tray shows a native menu instead, so only tests open it (`ui::open_popover`).

Requests come from `convt-app [files...]`, `convt-app open [--to <fmt>] [--preset <name>] [--] <files...>`, `file://` URIs from file managers, `convt://convert?file=<path>&to=<fmt>` links, `convt://activate?key=<key>` links (open the License tab with the key filled in, never activate without a click) and `convt://auth?state=<state>&code=<code>` links (the browser's answer to a desktop sign-in; see "Sign-in and renewal"). A command-line request with `--to`, or a preset that names a format, converts in place with no window: the output goes next to the original whatever Settings says, no notification is shown and nothing is revealed, and the app quits when the batch is done if nothing else is open and no tray icon keeps it running. Links never convert without a click.

Licensing: a build from source needs no license and never shows first run or a trial card. With the check on, the sidebar shows the trial card, Quick convert shows why conversions stopped, and the License tab has the key field, Activate and Remove license. `convt-app --help` prints the usage without opening a window.

Example automation rules live in `src/placeholder.rs` (screenshots → PNG with copy, screen recordings → MP4, HEIC → JPEG off). Each enabled rule watches **one** directory, never recursively: macOS screenshots use `com.apple.screencapture location` (and skip the watch when `target` is clipboard); recordings use the recording location or Desktop. Tests must set `Automation.folder` or `CONVT_SCREENSHOT_DIR` / `CONVT_RECORDING_DIR` — the poller never opens the real Desktop under `cfg(test)`. Drive it with `AppState::poll_automations` (twice after writing a file, so the size can settle). A checkbox per rule copies the result; the tray drop bar still copies independently.

Pack tests use `TestPacks`, a scripted `pack::Backend` (`Fixture::with_packs`), never the real one: it counts installs and removes, holds a download mid-way (`hold`), fails as told (`fail_next`), and its registry adds `TestOffice`, a fake document engine, once "installed", so results don't depend on this machine's LibreOffice. Assert `packs.installs() == 0` before every click on `pack-download`; `only_the_download_button_reaches_the_installer` checks in the source that nothing else reaches `install_documents`. Ids: `pack-card` (its label is the title), `pack-body`, `pack-detail`, `pack-download`, `pack-cancel`, `pack-progress`, `pack-done`, and in Settings `pack-status`, `pack-remove`, `pack-remove-confirm`, `pack-remove-keep`. `open_quick` records the newest Quick convert window, so `window_of::<QuickView>` finds what the app opened.

Not testable in the headless tests: the system pickers ("Choose a folder…", Change, Add files' picker) and "Open presets folder"; the real tray icon and its menu (under Xvfb, a private bus has no StatusNotifierWatcher, so the app logs "no tray icon" and quits with its last window; check the icon on a desktop with a tray host, such as KDE, or GNOME with the AppIndicator extension); notifications on a real desktop; how the bundled fonts render (the test platform has no real text shaping). A launched app under Xvfb covers the pickers (through `scripts/fake-portal.py`), fonts and drag-and-drop; see below.

## Sign-in and renewal

Signing in to convt.app is optional and only for Pro (`src/account.rs`, `src/ui/account.rs`, the shared client in `crates/convt-license/src/account.rs`). "Sign in with convt.app" in Settings, License, or the first-run plan step opens `<site>/device?state=&challenge=&name=&os=&version=` and waits. The site answers with `convt://auth?state=...&code=...` (or `&error=...`), which counts only while the app waits with that state; a link the app didn't start, a replay, a cancelled flow's link or one older than 15 minutes is dropped with no network call and a notice. The app trades the code and verifier for a device token, stores `{email, token}` as `account.json` next to `license.key` (or in the credential store), and fetches the Pro key. While signed in, it asks `/api/device/license` once a UTC day at launch (`license_checked` in `settings.toml`) and on Refresh license. Together with the update check below, these are the only network calls the app makes without a click. Ids: `sign-in`, `sign-in-reopen`, `sign-in-cancel`, `account-status`, `account-notice`, `refresh-license`, `refresh-status`, `sign-out`, `refresh-note`, and in General `network-updates`, `network-refresh`, `network-other`.

Tests never reach the network: the `Fixture` gives the app a scripted `TestApi` (`f.api`) that counts exchanges, renewals and sign-outs, so assert `f.api.calls()` to prove a link or launch called nothing. `Fixture::signed_in(cx, key, email)` starts signed in. `AppState::age_sign_in` makes a waiting flow look old. Sign Pro keys with `pro_key`; read the opened page's state with `query(&cx.opened_url().unwrap(), "state")` and answer with `auth_link`.

In a real run, a build from source takes `CONVT_ACCOUNT_URL` (plain HTTP only to loopback), so point it at the dev server from `test-convt-web` (for example `http://localhost:3001`) and set `CONVT_LICENSE_PUBKEY` to `.convt-dev/license.pub` so seeded keys verify. Put a fake `xdg-open` first in `PATH` that appends its argument to a file: GPUI opens URLs with `xdg-open`, so that file is how you get the device page URL without a desktop browser. Open that URL in `agent-browser` (with the real `HOME`; the app's private `HOME` has no Chrome), sign in as a fixture, stub `HTMLAnchorElement.prototype.click` to record the `convt://auth` link, and click Approve or Cancel. Deliver the link as the desktop file's `%U` handler would: `./target/debug/convt-app '<link>'` with the same environment, which hands it to the running app. Check `$CONVT_CONFIG_DIR/account.json`, `license.key` and the `devices` row (`bash scripts/db.sh psql owner`). Offline renewal is a relaunch with `CONVT_ACCOUNT_URL` at a closed port and an older `license_checked`; revocation is "Sign out <name>" under Macs on the dashboard, then Refresh license. Never `xdotool windowkill` a GPUI window: it kills the whole app.

## Update check

`src/update.rs` and `src/ui/update.rs`. While update checks are on (Settings, General, `update-checks`; on by default) the app fetches the signed manifest at launch once a UTC day (`update_checked`) and on Check now (`check-updates`), verifies it with `convt-update`, keeps the highest accepted `update_sequence` in `settings.toml`, and selects the newest build the license covers. It shows `update-card` in the main window's sidebar ("Update available" with `update-download`, or "New version" with `update-renew`) and `update-status` in Settings. Failures appear only in `update-status`. Nothing downloads.

Tests give the app a scripted `TestReleases` (`f.releases`) that counts fetches; sign manifests with `manifest(sequence, &[(version, date)], &update_key())`. `update_key()` must stay different from `test_key()`.

In a real run, a build from source takes `CONVT_UPDATE_URL` (plain HTTP only to loopback) and `CONVT_UPDATE_PUBKEY`. Serve a manifest from a temp directory with `python3 -m http.server --bind 127.0.0.1 0`, signed with a throwaway Ed25519 key outside the repo (payload is base64url JSON; the signature covers `convt-update-v1\n` followed by the payload text). Manifest build dates may not be later than its `issued_at` day, and the running build's date is the day it was built, so to show a newer covered build, build a separate binary with `CONVT_BUILD_DATE` set a few days back in its own `CARGO_TARGET_DIR`. Use your own license key pair too and pass its public half as `CONVT_LICENSE_PUBKEY`, so you never need `.convt-dev/license.key`. To check again the same day, delete `update_checked` from `settings.toml` or click Check now.

## Headless tests first

`crates/convt-app/src/ui/tests.rs` opens real windows on GPUI's test platform, clicks elements by id and runs real conversions through `default_registry()`. They need no display and no permission, so run them for every app change:

```sh
cargo test -p convt-app
```

When adding a test, follow the existing `Fixture`:

- It points settings, history and presets at a `TempDir`, so tests never touch the user's config, and calls `theme::init`.
- `Fixture::new` is a build from source with no license check. `Fixture::licensed(cx, trial_start, key)` turns the check on with a test signing key, keeps the trial file and the key file in the `TempDir`, and never reaches the OS keyring. Sign test keys with `license_key`.
- It calls `cx.executor().allow_parking()` before anything else. Conversions run on real job threads; without it the test scheduler panics with "Detected activity on thread ... not deterministic".
- Controls are plain elements built by `ui/theme.rs` (`clickable`, `primary_button`, `text_button`, `switch`, `checkbox`, `select`, `segmented`). Each registers its `id` for test queries; find it with the helpers (`click`, `label`, `shown`). A switch's label is "On" or "Off"; a dropdown's options have the ids `{select id}-{option}` and only exist while it is open, so click the select first.
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

GPUI picks X11 when `DISPLAY` is set and `WAYLAND_DISPLAY` is empty. Under Xvfb it renders through Vulkan on llvmpipe (the log says `Selected GPU adapter: "llvmpipe"`), which is fine for layout and fonts. Geist renders, not a fallback. The log always has `Found no xinput mouse pointers` and `Could not open device /dev/dri/renderD128`; both are harmless. For the real AMD GPU, use the headless sway recipe in the user's memory (`gpuix-linux-gpu-testing`) through `sg render`; it is not needed for layout checks.

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

To see the trial card, the License tab and first run, also set `CONVT_LICENSE_ENFORCE=1` and `CONVT_LICENSE_PUBKEY`, with their own config, data and runtime directories. The `dev-keys` example (see the Licensing section of `test-convt-cli`) writes `.convt-dev/` in the workspace root, which other agents may share: copy the public key and the keys you need into `$work`, then delete `.convt-dev` only if it is the one you made. First run shows on the first plain launch with a fresh config directory; on Linux it has two steps, because the Finder step is macOS only. An expired trial is `echo 2026-01-01 > "$CONVT_DATA_DIR/trial"`. A key whose `updates_until` is before the build date is saved but shows the "newer than your license covers" status and Renew.

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

Use `xdotool`. Find windows with `xdotool search --name ''` and `getwindowname`, and their position with `getwindowgeometry --shell`; GPUI windows open at fixed spots (main window at 122,130, Settings at 332,150, Quick convert at 342,170, first run at 432,240 on a 1280x900 screen). `xdotool type` fills text fields. Scroll with button 5. Move the pointer to a corner before a screenshot so no hover state shows.

Drag-and-drop works with a real drag source: start Thunar on the same display (with a private `HOME`), then `mousedown`, move in small steps onto the convt window, and `mouseup`. xdotool alone has no XDND source.

### File-manager menus

`integrations/linux/install.py` writes to `$HOME/.local/share` and `$HOME/.config`. Run it with a private `HOME`, `XDG_CONFIG_HOME`, `XDG_DATA_HOME` and a `PATH` whose first entry holds `convt` and `convt-app` symlinks to `target/debug`, and pass `--thunar --nemo --dolphin --nautilus` explicitly. Thunar is installed here: start it with the same private variables on your Xvfb display, right-click a file, and pick "Convert with convt", or "Open With Other Application" and convt. `convt-app.desktop` claims no file types (only `x-scheme-handler/convt`) and writes nothing to `mimeapps.list`, so convt never becomes a double-click default; `app_entry` in `install.py` says why. Nemo, Dolphin and nautilus-python are not installed system-wide; run the real ones from a private prefix with the "Real file managers without root" recipe in `test-convt-cli`. Include file names with spaces, quotes, `%` and `$`.

## Evidence

Capture one window by its geometry:

```sh
eval "$(xdotool getwindowgeometry --shell "$wid")"
ffmpeg -v error -y -f x11grab -video_size "${WIDTH}x${HEIGHT}" -i "$DISPLAY+$X,$Y" -frames:v 1 "$work/app.png"
```

Open `app.png` with the Read tool and compare it with the Paper artboard for that window (04 for the main window, 03 Quick convert, 06 Settings, 05 first run), in both themes. Zoom in on text fields with `ffmpeg -vf crop=...,scale=...:flags=neighbor` to check that descenders (g, y, p) are not cut off. Check the change you made, plus the baseline: the main window shows the sidebar, the defaults bar and Add files, and the format cards Quick convert offers for a file match `./target/debug/convt targets <file>`. For a silent `open --to` conversion, check that the output file exists next to the input and is the requested format (`file <output>`). When a UI change affects layout, also resize the window (`xdotool windowsize`) and capture again; report sizes you did not check. Quick convert and Settings scroll when their content is taller than the window.

## Cleanup

Kill only what you started, by PID, then remove the work directory unless the screenshots are still needed for review:

```sh
kill $(cat "$work/pids") 2>/dev/null
rm -rf "$work"
```

Check that nothing you started is left: no process should still have your `DISPLAY` or `DBUS_SESSION_BUS_ADDRESS` in its environment. Processes started on the private bus (gvfsd, dconf-service, tracker, xfconfd and others from Thunar or Nautilus) are yours too. Never `pkill` by name; another agent may own a convt-app or Xvfb process.

## macOS and Windows

Native launch is `cargo run -p convt-app`. Taking a screenshot of a real session drives the user's desktop, so it needs explicit permission. The Finder Sync extension in `integrations/macos` is not built by Cargo and has no runnable path on Linux; see its README for the Xcode steps.
