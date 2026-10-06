---
name: test-convt-cli
description: Test convt conversions end to end through the `convt` CLI, the engines in convt-engines, routing in convt-core, and the Linux file-manager menus in integrations/linux. Use when changing an engine, a format, the registry, the CLI, or install.py, or when asked to reproduce a conversion bug or prove a conversion works.
---

# Test convt conversions

The CLI is the fastest way to prove engine and routing behavior. The desktop app, the Finder extension and the Linux menus all call the same `convt_engines::default_registry()`, so a CLI result is the ground truth for what a conversion produces.

## Doctor

Run from the repository root before trusting a result. Read only.

```sh
cargo locate-project --workspace --message-format plain   # must be this checkout's Cargo.toml
cargo build -p convt-cli && ./target/debug/convt engines
```

`convt engines` prints each ready engine (`image`, `svg`, `libheif`, `ffmpeg`, `pdfium`, `libreoffice`) and each unavailable one with its reason. A conversion that fails with "no route" usually means an engine is missing here, not a routing bug. Fix the environment first:

- `pdfium` unavailable: `bash scripts/fetch-pdfium.sh` (writes `vendor/pdfium`, gitignored).
- `ffmpeg` or `libreoffice` unavailable: install the package, or point `CONVT_FFMPEG` / `CONVT_SOFFICE` at a binary. See `crates/convt-engines/src/lib.rs` for discovery order.

Always run `./target/debug/convt` from this checkout. A `convt` on `PATH` may be an older install.

## Fixtures

Use synthetic inputs in a fresh temporary directory, never files from a user's home.

```sh
work=$(mktemp -d /tmp/convt-test.XXXXXX)
bash .agents/skills/test-convt-cli/scripts/fixtures.sh "$work/in"
```

This writes `sample.png`, `sample.mp4` (3 s, video and audio), `sample.wav`, `sample.svg`, `sample.txt`, and `sample.pdf` when LibreOffice is present. Add new fixtures to that script rather than committing binaries.

## Drive

Write outputs to a separate directory with `-o` so a same-format or failed run cannot touch the inputs.

```sh
c=./target/debug/convt; mkdir -p "$work/out"
$c targets "$work/in/sample.mp4"                    # what the menus would offer
$c "$work/in/sample.mp4" --to webm -o "$work/out"
$c "$work/in/sample.png" --to avif -o "$work/out"
$c "$work/in/sample.svg" --to jpeg -o "$work/out"
$c "$work/in/sample.pdf" --to png  -o "$work/out"   # one file per page: sample.png, sample-2.png, ...
```

Options, presets and batches:

```sh
$c "$work/in/sample.png" --to jpeg -q 60 --max-size 64 -o "$work/out"
# Transparent areas: white by default for JPEG and PPM, or --background black,
# "#ff8800" or transparent (refused for formats that can't store it).
$c "$work/in/sample.png" --to jpeg --background black -o "$work/out"
$c "$work/in/sample.pdf" --to webp --pages 2- --dpi 72 -o "$work/out"   # named by page: sample-2.webp, ...
$c "$work/in/sample.mp4" --to mp4 --video-height 120 -o "$work/out"
export CONVT_CONFIG_DIR="$work/cfg"                  # never the user's real presets
mkdir -p "$CONVT_CONFIG_DIR/presets"
printf 'to = "jpeg"\nquality = 60\n' > "$CONVT_CONFIG_DIR/presets/thumb.toml"
$c presets && $c "$work/in/sample.png" --preset thumb -o "$work/out"
$c "$work/in" -r --to webp -j 2 --json -o "$work/batch"   # subfolders mirrored under -o
```

`--json` prints one event per line on stdout (`started`, `progress`, `done`, `failed` with a `kind`, then `summary`). Folder files that can't reach the target, or already are it, are skipped quietly; files named directly report their errors. Exit codes: 0 all done, 1 any failure, 130 cancelled.

To test cancellation, start a conversion of a long video in the background, `kill -INT` that PID (never `pkill`), and check that it exits 130, that its FFmpeg child is gone (`ps -o pid= --ppid <pid>` before the kill), and that the output directory holds no `.convt-*` staging folder or partial file.

HEIC and AVIF decode through the patched libheif (`libheif.so.1`, or `CONVT_LIBHEIF_DIR`). Unpatched system libraries are unavailable because they cannot disable implicit plugin loading. For full host verification, select the built bundle with absolute `CONVT_LIBHEIF_DIR` and `CONVT_LIBHEIF_PLUGIN_DIR`, and prepend its `lib` directory to `LD_LIBRARY_PATH`. Set `CONVT_FFMPEG` and `CONVT_FFPROBE` to the bundle tools so the independent AVIF validator has container support. The Python fixture helper separately needs a trusted absolute `LIBHEIF_PLUGIN_PATH`; the engine ignores that variable. The matrix generates its own HEIC with alpha. It first tries a libheif HEVC encoder, then uses FFmpeg's libx265 to encode the color and alpha items in a small HEIF container. It generates AVIF with libheif's AV1 encoder, independently of convt's AVIF encoder. When no HEIC encoder is available, `CONVT_TEST_HEIC=/path/to/sample.heic` opts into a local sample, copied into the temporary fixture directory and compared against separately decoded source pixels. No real sample belongs in the repository.

Pick the paths your change touches. For a routing change, also check that `targets` does not offer something it should not. A still image must never list a video or audio format through a multi-hop route; `crates/convt-core/src/registry.rs` has tests for this.

## Licensing

Builds from source skip the license check, so `convert` works without a key. To try the trial and activation flow, make a development key pair and opt in at run time. Always set `CONVT_LICENSE_STORE=file`: without it a key goes to the user's real Secret Service keyring.

```sh
cargo run -p convt-license --example dev-keys -- keygen       # writes .convt-dev/ (gitignored)
key=$(cargo run -q -p convt-license --example dev-keys -- issue you@example.com 2027-10-02)
export CONVT_CONFIG_DIR="$work/cfg" CONVT_DATA_DIR="$work/data" CONVT_LICENSE_STORE=file
export CONVT_LICENSE_ENFORCE=1 CONVT_LICENSE_PUBKEY=$(cat .convt-dev/license.pub)
$c license                                          # Free trial: 7 days, starting with ...
$c "$work/in/sample.png" --to jpeg -o "$work/out"   # starts the trial: $work/data/trial
echo 2026-01-01 > "$work/data/trial"; $c "$work/in/sample.png" --to jpeg -o "$work/out"   # trial ended, exit 1
echo "$key" | $c license activate && $c license     # key in $work/cfg/license.key, mode 0600
$c license remove
```

`formats`, `targets`, `engines` and `presets` never need a license; only conversions do. A key issued with an `updates_until` before today's build date shows the "newer than your license covers" message. Delete `.convt-dev` when done unless you need the key again, and never commit it.

The Secret Service path has one ignored test. Run it only on a private bus with its own runtime and data directories, so it can't reach the user's keyring:

```sh
T=$(mktemp -d); mkdir -m 700 "$T/run"; mkdir "$T/data"
env -u DBUS_SESSION_BUS_ADDRESS XDG_RUNTIME_DIR="$T/run" XDG_DATA_HOME="$T/data" dbus-run-session -- sh -c \
  'echo -n test | gnome-keyring-daemon --unlock --components=secrets >/dev/null; cargo test -p convt-license --features client -- --ignored secret_service'
rm -rf "$T"
```

The Keychain on macOS and Credential Manager on Windows have no test path here; report them as unverified.

## Evidence

A zero exit code and a `-> path` line are not enough. Inspect the output:

```sh
ffprobe -v error -show_entries stream=codec_name,width,height:format=duration -of compact "$work/out/sample.webm"
file "$work/out/"*
```

Check codec, dimensions, duration, and page count against what the change should produce. For images, open the file with the Read tool to look at it. Report which engines were unavailable, because those routes were not exercised.

## Linux file-manager menus

Format entries run `convt-app open --to <format> -- <files>`, so the app shows progress and errors. Every supported Linux menu also has “More options…” which runs `convt-app open -- <files>` to open Quick convert without choosing a target. `install.py` reads `convt formats --json` and `convt targets` to build them, and also installs `convt-app.desktop` (Open with, and the `convt://` handler). It needs both binaries on `PATH`.

Start with the unit tests. They run each generated command the way the file manager would: Dolphin entries and `convt-app.desktop` through `gio launch`, Thunar through `sh -c`, Nemo through GLib's argv parser, and the Nautilus extension against stubbed GObject classes. A fake app in a folder with spaces and quotes records its arguments, and the file names include quotes, `$HOME`, a backtick, `100%` and a leading dash.

```sh
python3 -m unittest discover -s integrations/linux
```

Tests that need GLib, `gio` or `desktop-file-validate` skip without them; say which ran. Then check the real output against a throwaway home, since `install.py` writes into `$HOME` and runs `xdg-mime`:

```sh
cargo build -p convt-cli && cargo build -p convt-app   # see test-convt-desktop for the app's build needs
h="$work/home"
HOME="$h" XDG_DATA_HOME="$h/.local/share" XDG_CONFIG_HOME="$h/.config" PATH="$PWD/target/debug:$PATH" \
  python3 integrations/linux/install.py --dolphin --nemo --thunar --nautilus
find "$h" -type f | head
desktop-file-validate "$h/.local/share/applications/convt-app.desktop"
```

Check that each `Exec`/`command` points at `target/debug/convt-app`, that Dolphin `.desktop` files are mode 0755, and that `uca.xml` parses (`python3 -c 'import xml.dom.minidom,sys;xml.dom.minidom.parse(sys.argv[1])' "$h/.config/Thunar/uca.xml"`). The unit tests emulate Nemo and Thunar from their source and Dolphin through GIO; none of them runs inside the real file manager. Seeing the menu in Dolphin, Nemo, Thunar or Nautilus means launching the file manager, which needs the user's explicit permission in the current request. Without it, report the menus as verified by emulation only. With it, use the recipe below on a private display, never the user's desktop session.

What each menu looks like in the real app, so you can tell a regression from the expected shape:

- Nautilus 46 (nautilus-python 4.0): a "Convert with convt" submenu with the targets in `convt targets` order, then "More options…". A video's 23 targets run past a 900 pixel screen; the popover scrolls.
- Nemo 6.0: no submenus. Each target is a top-level "Convert to X" item, in action file name order, with "More options…" last because its file is `convt-zz-more-options.nemo_action`. Nemo compares file names byte by byte. Nemo 6.0 has no action layout file, so a submenu is not possible there.
- Dolphin 23.08 (KIO 5.115): a "Convert with convt" submenu. KIO sorts a submenu by action ID, not label, so the IDs are `convert_<target>` and `options`, which puts "More options…" last.

### Real file managers without root

Nautilus 46 and Thunar are installed system-wide here; Nemo, Dolphin and nautilus-python are not, and there is no passwordless sudo. Install them into a throwaway prefix instead. `apt-get -s install` works without root and lists exactly the packages this machine is missing:

```sh
REPO=$PWD W=$(mktemp -d /tmp/convt-fm.XXXXXX); cd "$W"; mkdir debs root bin home
for p in nemo dolphin python3-nautilus; do apt-get -s install $p | awk '/^Inst /{print $2}'; done | sort -u > all.txt
(cd debs && apt-get download $(cat ../all.txt))      # about 250 packages, 57 MB
for d in debs/*.deb; do dpkg -x "$d" root; done      # about 270 MB
ln -s "$REPO/target/debug/convt" bin/convt; ln -s "$REPO/target/debug/convt-app" bin/convt-app
```

Package postinst scripts never run, so do their work by hand. Nemo needs its schemas compiled together with the system ones: copy `/usr/share/glib-2.0/schemas/*.xml` and `*.override` plus `root/usr/share/glib-2.0/schemas/*` into `$W/schemas` and run `glib-compile-schemas $W/schemas`. Dolphin needs `kbuildsycoca5` run once with its environment set (below).

Nautilus loads extensions only from the directory compiled into the binary, `/usr/lib/x86_64-linux-gnu/nautilus/extensions-4`; no environment variable changes it. Copy `/usr/bin/nautilus` to `$W/bin/nautilus` and overwrite that string with a shorter private path padded with NUL bytes, then put `libnautilus-python.so` from `root/usr/lib/x86_64-linux-gnu/nautilus/extensions-4` in that directory:

```sh
mkdir "$W/nx"; cp root/usr/lib/x86_64-linux-gnu/nautilus/extensions-4/libnautilus-python.so "$W/nx/"
python3 - "$W" <<'EOF'
import sys
old = b"/usr/lib/x86_64-linux-gnu/nautilus/extensions-4\0"; new = sys.argv[1].encode() + b"/nx"
data = open("/usr/bin/nautilus", "rb").read()
assert data.count(old) == 1 and len(new) < len(old)
open(sys.argv[1] + "/bin/nautilus", "wb").write(data.replace(old, new + b"\0" * (len(old) - len(new))))
EOF
chmod +x "$W/bin/nautilus"
```

nautilus-python reads Python extensions from `$XDG_DATA_HOME/nautilus-python/extensions`, which `install.py --nautilus` fills in the private home.

Export the private home first, then start Xvfb, a private D-Bus bus and `fake-portal.py` as in `test-convt-desktop` (Launch headless on Linux), with `$W` as the work directory. The order matters: services the bus starts on demand (dconf-service, gvfsd-metadata, tracker-miner-fs, goa-daemon) inherit the bus daemon's environment, not the file manager's. A bus started with the real `HOME` writes Nemo and Nautilus settings into the user's real `~/.config/dconf/user` and rewrites `~/.cache/tracker3`, even though every file manager has a private home. Then run `install.py --nautilus --nemo --dolphin` in that environment:

```sh
export HOME=$W/home XDG_DATA_HOME=$W/home/.local/share XDG_CONFIG_HOME=$W/home/.config XDG_CACHE_HOME=$W/home/.cache
export XDG_RUNTIME_DIR=$W/run CONVT_RUNTIME_DIR=$W/run CONVT_CONFIG_DIR=$W/cfg CONVT_DATA_DIR=$W/data CONVT_LICENSE_STORE=file
export PATH=$W/bin:/usr/bin:/bin NO_AT_BRIDGE=1
# Now start Xvfb, dbus-daemon and fake-portal.py, then:
python3 "$REPO/integrations/linux/install.py" --nautilus --nemo --dolphin
R=$W/root L=$R/usr/lib/x86_64-linux-gnu
GI_TYPELIB_PATH=$L/girepository-1.0 nautilus "$HOME/Samples" &
LD_LIBRARY_PATH=$L XDG_DATA_DIRS=$R/usr/share:/usr/share GSETTINGS_SCHEMA_DIR=$W/schemas $R/usr/bin/nemo "$HOME/Samples" &
LD_LIBRARY_PATH=$L QT_PLUGIN_PATH=$L/qt5/plugins QML2_IMPORT_PATH=$L/qt5/qml QT_QPA_PLATFORM=xcb \
  XDG_DATA_DIRS=$R/usr/share:/usr/share XDG_CONFIG_DIRS=$R/etc/xdg:/etc/xdg PATH=$W/bin:$R/usr/bin:/usr/bin:/bin \
  sh -c 'kbuildsycoca5 >/dev/null 2>&1; exec dolphin "$HOME/Samples"' &
```

Run one file manager at a time so their windows don't overlap. Nautilus forks, so `$!` is not its PID, and each one starts helpers on the private bus (gvfsd, dconf-service, tracker-miner-fs, goa-daemon and others). Find all of them by their environment and kill only those:

```sh
bus=$DBUS_SESSION_BUS_ADDRESS
for d in /proc/[0-9]*; do { tr '\0' '\n' < $d/environ; } 2>/dev/null | grep -qxF "DBUS_SESSION_BUS_ADDRESS=$bus" && echo ${d#/proc/}; done
```

Dolphin shows blank file icons because no Breeze icon theme is installed; thumbnails and menus work. Drive the menus with `xdotool` (right-click, then click "Convert with convt"; Qt submenus open on click more reliably than on hover) and capture with `ffmpeg -f x11grab`. To keep icon positions stable between conversions, move each output out of the folder after checking it. A target click converts silently next to the original; "More options…" opens Quick convert, which stays hidden but alive after Cancel, so later requests reuse the same convt-app process.

Before and after, checksum the real home's `.local/share/{nemo,kio,nautilus-python,applications,mime,gvfs-metadata}`, `.config/{Thunar,nemo,dconf,mimeapps.list}` and `.cache/tracker3`, and compare. A changed `.config/dconf/user` means the bus was started with the real home. When done, kill the PIDs found above, then `rm -rf "$W"`. The work directory ends up about 350 MB.

## Automated tests

Run both tiers from the repository root with the file-backed license store. Plain Cargo tests include a real declared conversion for every available engine, an encoder regression for RGB ICO and floating-point EXR inputs, a 16-bit precision check, routing checks, and CLI scenarios. The CLI tests cover recursive folders, parallel jobs, JSON progress, presets, output directories, collisions, selected PDF pages, corrupt and unsupported inputs, and a read-only destination. Engines or validator tools that are missing are reported as skips. Running as root cannot prove the Unix permission check, so that test reports the limitation.

```sh
export PATH=$HOME/.cargo/bin:$HOME/.bun/bin:$PATH CONVT_LICENSE_STORE=file
cargo test
# Release binaries use only explicit, executable-adjacent and system library paths.
export CONVT_PDFIUM_DIR="$PWD/vendor/pdfium/lib"
cargo test --release -p convt-engines
bun run rs:check
python3 -m unittest discover -s integrations/linux -v
```

PDFium's repository vendor directory is discovered automatically only in debug builds. Release tests must set `CONVT_PDFIUM_DIR` before launching the test process, as shown above, or use `bash scripts/test-engines.sh --release`. The matrix package command uses that launcher, which supplies the repository library path unless an explicit override is already set. It does not change shipped discovery.

The full tier is explicit. It generates one tiny fixture per format ID, reads every offered target from the registry, converts each pair, and validates the result. Fixtures are shared across cases. The default is eight concurrent workers; each LibreOffice process gets a private profile. Tests use temporary directories and remove their outputs after the run.

```sh
bun run test:matrix
CONVT_MATRIX_INPUT=heic,avif bun run test:matrix
CONVT_MATRIX_TARGET=pdf,png bun run test:matrix
CONVT_MATRIX_JOBS=16 bun run test:matrix
bash scripts/test-engines.sh --release --test matrix full_matrix -- --ignored --nocapture
```

Input and target filters accept comma-separated format IDs. An empty selection fails instead of reporting success. `CONVT_MATRIX_REPORT=/tmp/convt-matrix.json` saves case results, timings and failure reasons as JSON. `CONVT_MATRIX_KEEP=1` retains the temporary directory and prints its path for diagnosis. Remove only that printed directory when finished.

The matrix checks image magic, dimensions, quadrant colors and alpha; media containers, codecs, streams, duration, changing first and last video frames and the 440 Hz audio tone; document marker text; presentation slide count and text; and spreadsheet cells A1, B1, A2 and B2. PDFium extracts PDF text. Office images are compared with an independently exported and rendered reference PDF. The two-page PDF fixture has distinct colored page markers and a large black text block. Its text region must contain the expected dark pixels; a regression test erases the text while preserving the colored markers and proves that validation fails. Output names and selected-page behavior are checked too. Video height is a maximum and never enlarges the source; scaled dimensions stay even. ICO's maximum size of 256 pixels, SVG's 96 DPI user units, binary GIF transparency, and bounded media encoder padding are accounted for explicitly.

The summary prints pass, failure and skip counts per input category, the slowest cases, every failure reason, and wall time. An explicit required-route table is checked independently of the offered-target loop. It covers every engine step family and the HTML, RGB ICO, AVIF and PDF regressions; removing a required route fails when its backend capability is available. Known intentional gaps, including raster-to-SVG, are reported separately. HEIC output is capability-gated and required when a HEVC encoder is available. The HEIC suite also checks linear-gamma PNG NCLX, custom primaries, white adaptation, plugin-path constructor sentinels, 10-bit capability and explicit 8-bit fallback. See `docs/heic.md` for format notes. Raster-to-HEIC validation skips when Python or the independent HEVC decoder is missing. Libheif advertises HEIC and AVIF input steps only when the matching HEVC or AV1 decoder plugin is available, and unavailable input decoders are skipped explicitly. The format table currently has 40 IDs; derive coverage from `FORMATS` rather than a fixed claimed count. Raster-to-SVG is unsupported. HEIC and AVIF encoding through libheif are offered only when the loaded library can instantiate the matching encoder. AVIF decoding requires a libheif build with an AV1 decoder.

The matrix also generates AV1 video in MP4, WebM and MKV and runs every supported target with the same motion, pattern, stream, duration and audio checks. AV1 labels are distinct in logs and JSON reports. A ready FFmpeg without a software AV1 decoder must fail these cases. `CONVT_MATRIX_AV1_FFMPEG=/absolute/path/to/trusted/ffmpeg` selects a separate libaom-enabled encoder for fixture generation when the shipped FFmpeg has a decoder only, as on Mac. Conversion still uses `CONVT_FFMPEG`. Missing fixture-generation tooling is an explicit prerequisite failure, not a decoder skip.

Runtime fixtures and Office validators live in `scripts/matrix-fixtures.py`, and Rust image/media checks live in `crates/convt-engines/tests/support/mod.rs`. Update those sources when adding formats. Never add fixture binaries. The older shell fixture script above remains useful for manual CLI checks.

On this Linux machine, all six engines and validation tools are present. A passing run without those tools proves less; always report the skips. Linux file-manager tests emulate their commands and do not prove that a real desktop menu is visible.

## Linux bundle and document pack

`bun run bundle:linux` builds the relocatable payload using hash-locked inputs. `bun run bundle:appimage` wraps it. Set `CONVT_BUNDLE_OUT` to a fresh directory for repeat builds and `CONVT_BUNDLE_CACHE` to reuse downloads; every cached input is verified again. The CLI, GUI, codec libraries and validation tools build in pinned manylinux/AlmaLinux 8 with a glibc 2.28 ceiling checked on every ELF. The harness uses these baseline validation tools, not a host-built runner. Runtime discovery searches explicit overrides, the executable's fixed package layout and system paths. Use an absolute `CONVT_LIBHEIF_PLUGIN_DIR` for codec tests outside the bundle. Relative plugin paths are ignored; `LIBHEIF_PLUGIN_PATH` is not a convt plugin override. It never searches arbitrary ancestors.

`bash packaging/linux/validate.sh /absolute/path/to/convt` runs the semantic matrix through the packaged CLI in Ubuntu 22.04, Debian 12, Debian stable-slim and Ubuntu 24.04 with networking disabled. `CONVT_TEST_FEDORA=1` adds Fedora. `CONVT_TEST_USE_LOCAL_IMAGES=1` uses cached test images without a registry request and still records their image IDs and digests. `CONVT_TEST_DISTROS="ubuntu:24.04 fedora:latest"` limits a diagnostic rerun to those images; a release proof still covers the full set. The containers receive only the bundle and an isolated test interpreter and matrix runner, never host library directories or sockets. `CONVT_BUNDLE_REPORT` selects the report directory. `CONVT_BIN` switches host matrix conversions to a specific CLI; targets must match the test registry, so point both at the same native bundle using its library/plugin paths. `CONVT_MATRIX_HELPER` overrides the Python helper's path for containers.

Without a document pack, documents must have no targets and the matrix reports those inputs skipped. `python3 packaging/linux/build-document-pack.py <fresh-output-directory>` produces a local proof pack from installed LibreOffice with a private native closure and relocated bootstrap. It makes no downloads and is not a pinned public-release pack. Set `CONVT_DOCUMENT_PACK` to its `documents` directory when running the container harness. To prove installation and per-user discovery too, use `CONVT_DOCUMENT_ARCHIVE` pointing to a directory containing `documents.tar.gz` and `documents.sha256`; each container installs and verifies that archive explicitly before running the matrix. Validate all document cases as well as non-document cases.

Use `convt pack status documents` or `convt pack status` for the same offline status check. Test install/status/removal with temporary `CONVT_DATA_DIR`, `CONVT_CONFIG_DIR` and `CONVT_LICENSE_STORE=file`. `convt pack install documents --source file:///absolute/path/documents.tar.gz --sha256 <digest>` is an explicit local test source. Discovery still requires that digest to equal `CONVT_DOCUMENT_PACK_SHA256` embedded at build time; a source override cannot establish a release trust pin. Build the verification bundle with the proof archive digest, `CONVT_DOCUMENT_PACK_VERSION`, and the placeholder URL. Default installation fails offline before any request until release CI embeds the real URL and digest. Pack tests cover resume, complete cached archives, checksum rejection, unsafe archive entries, pinned receipts, writable paths, symlinked launchers and removal roots, reinstall verification, inherited lock descriptors, staging-orphan cleanup and publication recovery. Status, discovery and conversions never download a pack. See `docs/document-pack.md` for the UI contract and remaining release gates.

## Cleanup

Remove only the directory you created: `rm -rf "$work"`. Keep it while review is pending if the outputs are your evidence, and give its path.

## Native Linux installers

Rebuild the current sources with `bun run bundle:linux` into a fresh `CONVT_BUNDLE_OUT`, then replace the old payload only after the build and ELF audit succeed. Preserve `packaging/out/verification`. Set `SOURCE_DATE_EPOCH` to the existing payload's epoch when comparing builds. `bun run package:deb`, `bun run package:rpm` and `bun run package:linux` wrap that exact payload using standard tools inside digest-pinned containers. They do not rebuild Rust binaries. `CONVT_PACKAGE_VERSION` and `CONVT_PACKAGE_RELEASE` select package metadata.

The packages put the payload in `/opt/convt`, symlink `/usr/bin/convt` and `/usr/bin/convt-app`, and install desktop metadata, icons, Nautilus Python, Nemo actions and Dolphin service menus under `/usr/share`. Static menus open Quick convert so targets reflect the installed engines. Thunar has no system custom-action location: run `python3 /usr/share/convt/integrations/install.py --thunar` as the user who wants those actions. An explicit per-user installer run retires that user's old entries when system integration is present. Package scripts never inspect user homes.

Run `bun run test:packages` after building both current packages and an older release under `packaging/out/verification/packages/older`. To prepare an older package, use the saved previous payload with `CONVT_BUNDLE_OUT` and `CONVT_PACKAGE_RELEASE=0`, then copy its packages into that directory. The runner installs the older build and upgrades with apt or dnf. Dependency installation can use the network. The three full conversion passes run in fresh containers with `--network none`, using the installed `/usr/bin/convt` through an environment-clearing wrapper: 349 non-document cases, 579 cases with distro LibreOffice, then 349 after LibreOffice removal. Distro Office runs use one matrix worker by default to avoid concurrent first-profile startup collisions; `CONVT_PACKAGE_MATRIX_JOBS` overrides that count. `CONVT_PACKAGE_LIFECYCLE_ONLY=1` reruns metadata, upgrade and removal checks without repeating already verified matrices. The validator uses separate baseline-built tools; its library overrides do not reach the CLI. Reports include per-case semantic results, engine lists, metadata validation, GUI ldd, lint and reinstall/removal checks. `CONVT_PACKAGE_DISTROS` can select names from `package-images.lock.json`.

Package verification must also compare two package builds byte for byte, inspect their dependency metadata and maintainer scripts, and prove user data survives removal. No GUI launch is needed for ldd. Lintian and rpmlint policy exceptions must name the actual finding and explain the bundled `/opt` layout rather than suppress unknown warnings. No package is signed; P11 release CI must sign copies after reproducibility checks. Document packs and corresponding-source publication remain separate release gates.

## Release verification (P11)

Read `docs/release.md` before building release artifacts. `cargo test -p convt-update` checks valid and tampered signatures, wrong keys, schema rejection, expiry, metadata rollback and license coverage. `crates/convt-update/examples/check-manifest.rs` is a tiny offline consumer for checking a production-ready Bun-signed fixture.

Freeze one source tree with `python3 scripts/release/source.py snapshot /tmp/convt-release-source` and use `CONVT_RELEASE_SOURCE_TREE` for both builds when agents share the checkout. With a fixed `SOURCE_DATE_EPOCH` and the same external `CONVT_UPDATE_SIGNING_KEY`, run `bun run release:linux --verification-only` into two fresh output roots. Compare all artifacts and manifests with `scripts/release/compare.py`. Run `scripts/release/rebuild-cli.sh` on the source archive; it builds offline in an owned, labeled clean container and performs a real SVG-to-PNG conversion. Also run `cargo test` and `bun run rs:check`.

Verification manifests have `distribution_ready=false`. Production `--dry-run` and real upload fail while any corresponding-source or notice gate remains. Linux release collection now checks source-built FFmpeg, the exact PDFium revision and DEPS sources, native RPM/GCC recipes and the source-built AppImage runtime. The Linux archive excludes unrelated SDK-derived Mac crate sources and preserves original manifests, pins and an identical compilation graph in its derivation receipt. A clean offline rebuild alone does not establish native source coverage. Use production `--dry-run` with separate external keys to test `distribution_ready=true`; adding uncovered Mac or Windows artifacts must make the manifest unavailable. Keep publication gaps explicit in the final PASS/FAIL/NOT CHECKED report. Never run live uploads or hosted workflows as part of a local release dry run.

## Local media security

Conversion, duration probing and desktop thumbnails share a local-only FFmpeg policy. It permits file and pipe protocols and self-contained media demuxers. HLS, DASH and concat playlists are refused, including playlists referencing other local files. Do not remove the demuxer allowlist merely because network protocols are blocked.

On the Linux verification host, run `CONVT_REQUIRE_MEDIA_TOOLS=1 cargo test -p convt-engines local_ -- --nocapture`. This requires both `/usr/bin` tools and the tools under `packaging/out/convt`. The DASH test opens an owned loopback listener and tests a disguised MP4 and a Linux memfd through duration, conversion and the shared thumbnail command. Both toolsets must produce zero HTTP requests. The playlist test verifies HLS, DASH and concat refusal. Run `bun run test:matrix` afterward and retain the full 579 cases when all engines are available.
