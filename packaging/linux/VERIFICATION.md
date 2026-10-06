# Linux packaging verification

## Review fixes, 2026-10-04

The current sources were copied into an isolated checkout, rebuilt in the pinned AlmaLinux 8 builder with `SOURCE_DATE_EPOCH=1791072000`, and swapped into `packaging/out` after success. The previous verification directory was preserved. The product passed the glibc 2.28 audit for all 20 ELF files; the separate validator passed for 110. Native packages reuse these binaries under `/opt/convt`, with `/usr/bin` symlinks. Neither package depends on distro FFmpeg, PDFium or libheif. Host dependencies are listed in [INVENTORY.md](INVENTORY.md). Tarball and AppImage copies were refreshed too.

The launcher clears inherited `LD_LIBRARY_PATH` before its absolute `/usr/bin` helpers run. The CLI and app use fixed, executable-relative `$ORIGIN/lib` DT_RPATH for their private closure. This preserves relocation and transitive native loading while leaving system LibreOffice children free of the private loader path. The launcher does not inject a HEIF plugin path into children. A non-root relocated run with a hostile PATH and working directory passed executable-identity and transitive-dlopen checks. Loader tracing rejected a working-directory `libm.so.6` despite inherited `LD_LIBRARY_PATH=.`. Bundled FFmpeg produced an independently probed MP3. A clean system Office child produced PDF, then bundled PDFium rendered PNG. Relative PDFium, libheif and plugin overrides now warn and are ignored; isolated constructor-sentinel tests cover rejection and absolute positive controls.

The package recommends Nautilus Python and suggests LibreOffice. Normal apt/dnf installation must leave Office absent. Documents use an explicitly installed pack or an existing system Office. Desktop metadata still claims only `x-scheme-handler/convt`, and package installation sets no file-type defaults.

Package post-install scripts only print guidance. Existing per-user menus are retired by the installing user with:

```sh
python3 /usr/share/convt/integrations/install.py --uninstall
```

This explicit step keeps package hooks out of home directories. Cleanup recognizes the new marker and complete legacy generator output, including commands pointing to an old checkout. It preserves unrelated files, symlinks and Thunar actions. Thunar users can then run the same installer with `--thunar`; it has no system custom-action directory. Nautilus offers More options for every recognized input even when targets are empty, and both caches expire after 30 seconds. Static menus retain recognized inputs with no targets too. All 35 integration tests pass. A real isolated cleanup removed 79 legacy files and 39 Thunar actions while retaining a custom menu. No file-manager window or GUI was launched.

Pinned upstream notices now cover libxcb, libxcb-xkb and libXau, plus the existing XKB closure. Collection rejects unknown components and absent or corrupt notices, including reused origin receipts. Four isolated negative/positive tests pass. Package metadata includes a component inventory, aggregate SPDX expression, DEP-5 copyright and RPM summary, with notice paths and holders. Complete upstream texts stay under `/opt/convt/licenses`; standard Debian terms are referenced instead of duplicated in the DEP-5 text. This earlier verification used the downloaded static FFmpeg build and had 30 missing Rust notices. The P11 source-closure changes below supersede those inputs and gaps. The source lock, payload receipt and notice graph must still agree; `--release` refuses missing notices.

Packaging uses offline, hash-locked dpkg-deb 1.22.6 and rpmbuild 6.0.2 in pinned containers. Both final artifacts compare byte for byte across separate output trees and umasks 022 and 077. A reproduced Debian difference came solely from filesystem-dependent `du` directory allocation in Installed-Size; both data archives were identical. The header now uses logical file/link bytes. The deb is 62,344,996 bytes, SHA-256 `6885cc75c9331f5161cf0da437a0866d30db7f9ed983d429a5a64d2d0ec30c36`. The rpm is 62,143,009 bytes, SHA-256 `0fb030ee512e6a05d9157547f593022187f150516f2b580de52ff8bda89466a8`. Packages are unsigned; P11 signing remains a documented hook on copies.

The final five-distro results are recorded in `packaging/out/verification/review-fixes/packages`. The harness installs an older 0.1.0-0 package, upgrades through apt/dnf to 0.1.0-1 with normal recommendations, validates metadata and GUI ldd, then runs the installed CLI offline. Office is explicitly installed for document tests and removed for the third matrix. Reinstall, remove and Debian purge check every owned file and retain a user-data sentinel. One matrix worker avoids concurrent Office profile startup races. AlmaLinux has no AppStream validator or rpmlint in its base repositories: its exact installed AppStream file is validated offline with the host tool, and the identical RPM is linted in Fedora.

All five exact final artifacts passed normal package-manager installation without Office, upgrade, reinstall, metadata validation, menu presence, GUI library resolution and removal. Debian-family purge also passed, with user-data sentinels preserved. The installed CLI matrices ran with `--network none` and produced 6,385 successful conversions in total.

| Distribution   | Without Office | With Office | After Office removal | Lifecycle and metadata | Lint gate                      |
| -------------- | -------------: | ----------: | -------------------: | ---------------------- | ------------------------------ |
| Ubuntu 22.04   |            349 |         579 |                  349 | pass, including purge  | lintian, scoped exceptions     |
| Ubuntu 24.04   |            349 |         579 |                  349 | pass, including purge  | lintian, scoped exceptions     |
| Debian 12      |            349 |         579 |                  349 | pass, including purge  | lintian, scoped exceptions     |
| Fedora 44      |            349 |         579 |                  349 | pass                   | rpmlint, scoped exceptions     |
| AlmaLinux 8.10 |            349 |         579 |                  349 | pass                   | identical RPM linted in Fedora |

Earlier failing lint attempts and the Debian size-header runs remain archived. The final Debian-family matrix and lifecycle runs were repeated against the corrected header artifact, rather than relying on the previous package's identical data archive.

Linter output is retained. Scoped exceptions cover the private `/opt` layout, static FFmpeg, embedded codecs, the fixed main-executable RPATH and private GCC RUNPATH, PDFium's private SONAME, complete duplicated notices/provenance and required Dolphin executable data. Two new narrow RPM exceptions retain the hash-locked wayland-protocols-plasma 0.3.12 notice's historical FSF address, and allow the shared informative-only post-install helper. Standard-license duplication in DEP-5 was fixed rather than overridden. The gate rejects unparsed failures and unexpected exception paths.

Host `cargo test -p convt-engines -p convt-cli`, `bun run rs:check`, and `bun run test:matrix` pass. The host matrix reports 579 cases with no failures. The owned packaging documents pass oxfmt. The global `bun run check` passes oxlint but fails only formatting in `.agents/skills/test-convt-web/SKILL.md`, outside the allowed ownership. The independent GPT-6.1 Sol review was queued with Standard and medium settings but never started after more than 30 minutes; cancellation was requested. No review result is claimed. The queued review is interrupted.

Evidence, including failed lint and reproducibility attempts, is retained under `packaging/out/verification/review-fixes/`. All owned Docker containers, image aliases and tracked test snapshots were removed. Windows runtime verification and signing remain blocked; no commits or publishing occurred.

## Earlier bundled-payload verification

These records describe the preceding bundle and locally pinned proof-pack builds. The native installer section above describes the current artifacts. Earlier evidence remains available under `packaging/out/verification`.

Verified on 2026-10-04 with SOURCE_DATE_EPOCH=1791072000 (build date 2026-10-04). The x86_64 CLI, GUI and codec closure build in the digest-pinned manylinux_2_28 / AlmaLinux 8 image in `build-image.json` and target glibc 2.28. Cargo builds offline with Rust 1.95.0 and Cargo.lock; binary inputs, source packages, security patches and compiler runtimes are hash-locked.

After the payload is complete, `container-build.sh` runs `elf-audit.py` on it. The audit checks every ELF file under the payload, including the static ffmpeg and ffprobe, and fails the build if any file needs a GLIBC symbol version above 2.28, has an unresolved dependency, or resolves a library from outside the payload other than the host glibc ABI (libc, libm, libpthread, libdl, librt, libresolv, libutil, libmvec and the loader). It reports every violation, not only the first. It also audits the validation tools against their own library directories. Before either audit, `elf-audit.py --self-test` builds four small ELF cases and fails the build unless the audit accepts a clean pair of libraries and rejects a GLIBC_2.99 version need, a dependency resolved through an RPATH outside the root, and an unresolved library. A fifth case links a program against the builder's own glibc: it must pass in the 2.28 builder and fail on a newer host. On this host (glibc 2.39) the self-test rejected it as needing GLIBC_2.34. The audited build passed 20 payload ELF files and 110 validation-tool ELF files.

The GCC 15.2 runtime targets glibc 2.17 and exports GLIBCXX_3.4.34. It supplies newer C++ symbols for desktop drivers while preserving the bundle baseline. Its exact package recipes, metadata and GCC licence texts are retained.

### Build date and reproducibility

`build.sh` resolves one build date. It takes SOURCE_DATE_EPOCH, or CONVT_BUILD_DATE as UTC midnight, and refuses both when they name different days. A release build (CONVT_RELEASE=1 or CONVT_LICENSE_ENFORCE=1) fails if neither is set. A dev build defaults to today at UTC midnight. The script passes both values into the container, which refuses to build without them. They feed `crates/convt-license/build.rs` (the licence update cutoff), every timestamp in the build, the outer tarball mtimes and, through the `source-date-epoch` file in the output directory, the AppImage's squashfs timestamps. `build.rs` itself panics if CONVT_LICENSE_ENFORCE=1 has no date, if SOURCE_DATE_EPOCH is not a number, or if the two values disagree. Each of those cases was checked by running it.

Two complete builds from fresh work directories used the same frozen source snapshot and SOURCE_DATE_EPOCH. One ran with the container clock on 2026-10-04 and the other on 2026-10-07, using the test-only `clock-offset.c` preload that `CONVT_BUILD_CLOCK_OFFSET_DAYS` enables. All 168 payload files, all 3286 validation-tool files (excluding .pyc) and the tarball were byte-for-byte identical. The AppImage built from each payload was identical too.

| Artifact                    | SHA-256                                                          |
| --------------------------- | ---------------------------------------------------------------- |
| convt.bin                   | 32f63f102822b8b436aff49f4fdccf66494f478b2c2b3ff35e2fd0e928efc350 |
| convt-app.bin               | 39c01dde4b499195fd423d94c4c320fd29bcb9e35bab892a86ebd7060778242e |
| lib/libx265.so.199          | 0b69cb7b6992b627575838aa689633b513e982786ab44821dd7a0a9f148539f9 |
| convt-linux-x86_64.tar.gz   | ca5b73f1e6b8de0ab9bd21947107030df54f1c12252c65e8fa4ce4d80799210d |
| convt-linux-x86_64.AppImage | b93741c44c5638c8528473458b789fed4034a0e71e90f33150217110a663d036 |

The first pair of builds differed in two files. convt-app.bin had 96 different bytes of hash constants, because gpui pulls in ahash with `compile-time-rng` and const-random seeds from getrandom unless CONST_RANDOM_SEED is set. The container now sets it from SOURCE_DATE_EPOCH. licenses/native-ldd.txt held ldd load addresses, which ASLR changes on every run; the audit now omits them. Container paths (/repo, /work, /cargo, /rust) are fixed, so the remaining embedded paths do not vary, and GNU ld derives the build ID from content. CARGO_INCREMENTAL=0 is set. The reproducibility claim covers builds from the same source, cache inputs and builder image on x86_64; a different host kernel or Docker version was not tested.

### Results

The current build passed the full offline matrix in Ubuntu 22.04: 349 passed, 0 failed, 13 document inputs skipped, with no document pack. The log has no set_mempolicy lines. Offline smoke tests in Ubuntu 22.04 and Debian 12 used only the bundled binaries: ffmpeg generated a PNG and a MOV, then convt converted PNG to HEIC, HEIC to WebP and MOV to MP4. ffprobe read a 640 by 480 WebP and an MP4 with 320 by 240 H.264 and AAC, both 2 seconds long. The table below records the previous build (x265 with libnuma and pools=none), which was not rerun across every distribution after this change.

| Offline container, previous build | Without document pack                            | After pinned CLI pack installation |
| --------------------------------- | ------------------------------------------------ | ---------------------------------- |
| Ubuntu 22.04                      | 349 passed, 0 failed, 13 document inputs skipped | 579 passed, 0 failed, 0 skipped    |
| Debian 12                         | 349 passed, 0 failed, 13 document inputs skipped | 579 passed, 0 failed, 0 skipped    |
| Debian stable-slim, Debian 13     | 349 passed, 0 failed, 13 document inputs skipped | 579 passed, 0 failed, 0 skipped    |
| Ubuntu 24.04                      | 349 passed, 0 failed, 13 document inputs skipped | 579 passed, 0 failed, 0 skipped    |
| Fedora 44                         | 349 passed, 0 failed, 13 document inputs skipped | 579 passed, 0 failed, 0 skipped    |

With the previous build, AlmaLinux 8 also passed 349 cases with no failures and 13 document inputs skipped. The registry refresh for that extra test timed out; the successful run used its already cached image and recorded its digest. The document proof pack has a separate glibc 2.35 baseline and was not tested on AlmaLinux 8.

All matrix containers ran with `--network none`. Mounts contain the bundle, isolated baseline validation tools, reports and, for document runs, the pinned archive. No host library directories or sockets enter them. The CLI wrapper clears native overrides, so conversions use bundle discovery. The independent test runner and Python interpreter build at the same baseline and are separate from the product. Engine lists, targets, library-loader traces, GUI ldd, image digests and case JSON are retained. Document targets are absent without a pack. Installation verifies the local archive, publishes it in the container's private data directory and discovers it without CONVT_SOFFICE. All ten requested runs of the previous build passed on their first attempt.

Host checks for the current change: `cargo test` passes. `bun run rs:check` fails only on rustfmt differences in crates/convt-app/src/pack.rs, thumbs.rs and ui/quick.rs, which another change was editing at the time; `cargo clippy --all-targets -- -D warnings` passes and no file in this change has a formatting difference. `bun run test:matrix` used a host-built CLI with the new bundled codecs and FFmpeg: 579 passed, 0 failed, 0 skipped. The HEIC suite with CONVT_TEST_REQUIRE_HEIF=1 and the shipped codecs passed 15 tests with one test-only worker entry ignored. File-backed licence storage avoids the real keyring. Raster to SVG remains unsupported.

The current AppImage was extracted and tested offline in Ubuntu 22.04: the same PNG, HEIC, WebP and MP4 smoke test passed, AppRun printed the app help, and stderr had no set_mempolicy lines. GUI ldd resolved in the Ubuntu 22.04 matrix container. No GUI was launched. Desktop preparation from the earlier verification records package versions and the SHA-256 of archives verified against Ubuntu's signed package indexes.

### Security and HEIC regressions

Every requested finding has a failing-before and passing-after regression. Pack discovery requires the compiled digest, matching pointer and receipt, current-user ownership, protected permissions and a regular executable launcher. It rejects symlinks and externally mutable ancestors. Unix sticky ancestors such as /tmp are allowed when the next component is protected; the pack itself has no group/world write permissions. Installation always rebuilds from a digest-verified archive, including when a matching receipt exists. Status explains why an installation is rejected. Removal validates roots, locks and candidates, and removes safe staging orphans. HTTP redirects remain HTTPS-only; loopback HTTP exists only in unit-test builds and cannot redirect.

Full tests also reproduced a lock-descriptor inheritance race. PackLock now explicitly unlocks on operation completion, so a transient child descriptor cannot block an immediate reinstall. The duplicated-descriptor regression failed before the fix and passes afterward. Independent final reviews reported no remaining concrete findings in the reviewed changes.

The bundled libheif patch exposes secure initialization without automatic plugin loading. Convt then loads only absolute executable-relative bundle directories and an absolute CONVT_LIBHEIF_PLUGIN_DIR. Relative overrides and inherited LIBHEIF_PLUGIN_PATH are ignored. Unpatched system libraries are unavailable rather than using their implicit loader. Constructor-sentinel tests cover absolute and relative inherited paths, the working directory and a relative explicit override. No process environment mutation is needed.

HEIC tests cover quality, alpha, size, orientation, PNG colour-metadata precedence, NCLX transfer/primaries, custom-primary conversion to sRGB and white-point adaptation, including zero-Y primaries. Linear-gamma PNG retains a linear NCLX tag. Untagged input is tagged sRGB. Sixteen-bit input attempts 10-bit encoding. The shipped x265 build reports unsupported depth, and the tested HEIC fallback is 8-bit. A separately built local multilib x265 verified actual 10-bit HEIC; the old implementation failed that regression. AVIF preserves 10-bit output. The final shipped-codec HEIC suite passed 15 tests with one test-only worker entry ignored. See [HEIC notes](../../docs/heic.md).

x265 is built with ENABLE_LIBNUMA=OFF, and convt sets only x265:frame-threads=1, since a still image has one frame. With libnuma, x265 binds every pool worker with set_mempolicy whenever a pool exists, and no encoder parameter avoids that. A pool-enabled encode with the earlier libnuma build printed 152 "set_mempolicy: Operation not permitted" lines in a default-seccomp container, which is why the previous build disabled the pool. Without libnuma the pool stays on. A 6000 by 4000 PNG to HEIC encode on a 32-thread host took 37.2 to 37.8 seconds with the previous build and 4.0 seconds (Ubuntu 22.04) and 5.4 seconds (Debian 12) with the current one, offline and with Docker's default seccomp profile, with no set_mempolicy lines. The bitstream differs because rows now encode in parallel; output is deterministic across runs and distributions, decoded PSNR against the source is unchanged (21.9167 dB both), and the two decodes are 63 dB apart. Clap always prints convt, including from convt.bin.

### Layout and sizes

The tarball and AppImage use the same payload: convt and convt-app launchers with their .bin executables, static ffmpeg and ffprobe, lib/ with PDFium and the native closure, lib/libheif/plugins/, share/ desktop metadata, and licenses/ notices and provenance. SHA-256 sidecars verify both archives. The later native installers reuse this payload layout.

| Component               |    MiB |
| ----------------------- | -----: |
| CLI                     |  14.44 |
| GUI                     |  46.93 |
| FFmpeg                  |  76.13 |
| ffprobe                 |  75.98 |
| PDFium                  |   6.38 |
| HEIF codecs and plugins |  14.96 |
| C++ and X11 runtime     |   3.22 |
| Notices                 |   0.51 |
| Unpacked payload        | 238.54 |
| Tarball                 |  90.22 |
| AppImage                |  83.73 |

Exact bytes are in components.json; sizes-mib.json groups components. The LibreOffice 7.3.7 proof pack is about 150 MiB compressed and 407 MiB installed. Its SHA-256 is 6da865572b9110ff44d8e1620a0890f48c8e7217fe60d88a637230d360e6a6fd, embedded in this verification bundle. The default placeholder hosting URL is rejected before any request; explicit installation from the matching local archive works. A release build must embed its own platform/version digest and real URL. A source override cannot change the trust pin.

### Desktop requirements and release limits

The host supplies glibc and its loader, including libmvec when needed. X11 linkage libraries are bundled. Rendering requires an X11 display or Wayland compositor and XKB data; libvulkan.so.1 and a vendor ICD/driver/device, or libEGL.so.1 and an OpenGL implementation; libwayland-client.so.0 and libwayland-egl.so.1 for Wayland; and libfontconfig.so.1 with its font stack, configuration and fonts. Exact linkage and runtime requirements are in [INVENTORY.md](INVENTORY.md). An ldd pass establishes library resolution, not rendering.

[Document-pack design](../../docs/document-pack.md) defines the separately owned app dialog, explicit download action, progress, completion, retry, updates and removal. Real hosting and pinned release packs remain gates. LibreOffice redistribution requires MPL-2.0 and its third-party obligations; the local proof is not a release pack built from pinned source packages.

[Licence inventory](INVENTORY.md) records AGPL convt, GPL FFmpeg and x265, LGPL HEIF/de265, BSD PDFium/aom, the GCC runtime exception, MIT AppImage runtime and MPL LibreOffice. Complete corresponding source, patches, third-party notices and Rust dependency inventory remain public-distribution gates. Patent review is separate. Native macOS HEIC and macOS/Windows bundles remain unverified; Windows pack ownership checks fail closed pending native support. Native installer verification is recorded separately below.

Detailed evidence is in packaging/out/verification/, including reproducible-build/, x265-threading/, matrix-ubuntu-22.04/ and host-checks-2026-10-04/. Earlier failed regressions, the host-check setup collision with a rebuilding native directory, the reproduced lock race, and the optional Alma registry timeout remain recorded. The successful host rerun used an immutable native payload. No real HEIC samples are bundled. Owned containers and image aliases were removed.

### P11 release reproducibility

On 2026-10-05, two complete verification release runs used frozen tree `b2fc4f6ce49aab741e4591b3214e98237670582325f796c91e5389722b1c6e29`, separate build directories and `SOURCE_DATE_EPOCH=1791331200` (2026-10-07 UTC). Outputs are in `packaging/out/p11-resumed-a/0.1.0` and `packaging/out/p11-resumed-b/0.1.0`. `scripts/release/compare.py` passed for the payload, AppImage, deb, rpm, source archive, website manifest, signed update envelope and source audit. AppImage filesystem creation now forces every inode timestamp, including files created by appimagetool. Debian control timestamps are normalized after generation.

The AppImage SHA-256 is `6203e1da9b7262a583ee2d5c566d07999e659328fb883817da6ece2b76a47637`; the source archive SHA-256 is `070fcc01182637c937d968c4ba75dc062572d727328966da669ffa1e9285c522`. Both release runs completed with upload in dry-run mode. Their audit lists 69 source or notice gaps and their manifests set `distribution_ready=false`. These are private verification artifacts. This reproducibility result does not authorize public distribution.

Update unit tests, release-tool regressions, `cargo test`, `bun run rs:check` and actionlint 1.7.12 pass. Independent GPT-6.1 Sol review findings about historical builds, immutable uploads, frozen permissions and AppImage timestamps were fixed. Evidence is retained outside the checkout in `~/.agents/verify-notes/convt/evidence/2026-10-05-p11/`.

The source archive also rebuilt the CLI offline in a clean, labeled container without a host Cargo cache or target directory. The rebuilt CLI listed formats and engines, selected PNG targets and converted a real SVG into a PNG with the expected file signature. The container and its temporary extracted source were removed after the check. This proves the CLI rebuild path; the 69 binary-redistribution gaps remain.

## P11 source closure, 2026-10-05

Two production-mode local dry runs used the same frozen working tree, Wednesday build date 2026-10-07 and `SOURCE_DATE_EPOCH=1791331200`. Separate external temporary license and update keys were used for verification. No upload, package signing or hosted workflow ran. Both `packaging/out/p11-closure-verified-{a,b}/0.1.0` manifests and source audits report `distribution_ready=true`, with zero Linux publication gaps. Production publication still needs a rebuild with Leo's real public keys.

All eight compared files matched: payload tarball, AppImage, deb, rpm, source archive, source audit, website manifest and signed update envelope. The source archive SHA-256 is `c81a4de589926870fac4b2ba36b18dc3820a1f13a02d99b8b79f3e95886cfc7f`. Both finished tarballs and AppImages retain all 829 audited payload notice files. All 20 payload ELF files passed the glibc baseline audit.

The source gate validated pinned FFmpeg and codec inputs, exact PDFium DEPS and recipes, the actual stripped PDFium ELF, native source RPM associations, GCC recipes and the AppImage's actual source-built runtime. The Linux Rust archive retains 638 sources with complete notices. Its 902 compilation units preserve the original package pins, features, profiles and edges. Each derived CLI/app release build passed in a clean network-disabled container with empty Cargo and target caches. Each actual source tarball then rebuilt and ran the CLI offline and converted SVG to a valid PNG.

The first archive attempt exposed an overly broad tar exclusion that removed a crate's real `src/target` modules. The exclusion was removed, and the actual archive rebuild now runs before manifest generation. The superseded `p11-closure-final-*` pair is not a release candidate.

Host `cargo test`, `bun run check` and `bun run rs:check` passed. Update signature, tampering, wrong-key, rollback and coverage tests passed; a real Bun-signed manifest passed the Rust consumer. Source, notice, metadata derivation and platform coverage tests passed. Independent GPT-6.1 Sol reviews at medium effort on Standard identified and led to fixes for notice ordering, original-lock validation, PDFium binary association, clean build enforcement and explicit cache/epoch propagation. Evidence is outside the checkout at `~/.agents/verify-notes/convt/evidence/2026-10-05-p11-source-closure/`.

The original Mac FFmpeg binaries still need the pinned source-build replacement integrated and checked on Mac. SDK-derived Mac Rust sources retain their separate redistribution gate and are excluded from the Linux archive. Windows signing, live R2 uploads and hosted workflows are NOT CHECKED. `actionlint` is not installed. All owned containers and external temporary signing seeds were removed after verification.

## P11 AV1 follow-up, 2026-10-05

The full host matrix passed 651 cases, including 72 generated AV1 MP4, WebM and MKV cases, with zero failures or skips. All six engines were available. Linux FFmpeg already has a source-pinned libaom software decoder; each AV1 container also decoded with hardware acceleration disabled. No Linux native dependency was added. The Mac helper now builds pinned dav1d 1.5.3 and retains its COPYING notice. Its actual Mac rebuild remains P4's check; `packaging/release/macos-source-notes.md` contains the fresh-output commands and fixture-generator override.

A new production-mode local dry run used frozen tree `dc3642ee72674de0b1623b6a53df553b353a8397340b83e38296e3d94f3bdc16`, `SOURCE_DATE_EPOCH=1791331200` and separate external temporary verification keys. Outputs are in `packaging/out/p11-av1-release/0.1.0`: payload, AppImage, deb, rpm, source archive, website/update manifests and repository metadata. Both the source audit and website manifest report `distribution_ready=true`; the Linux publication gaps are empty. The source archive SHA-256 is `d2d2e875d9b15d490d13ae3ab96ae7b47c6a0a58b9818208bba5cdafeee7dd2f`. The actual tarball and AppImage retain all 829 audited notice files. The derived CLI/app build passed with empty caches and networking disabled. The delivered source tarball then rebuilt offline in another clean container and its CLI converted SVG to a valid PNG.

`cargo test`, `bun run check` and `bun run rs:check` passed. Independent GPT-6.1 Sol review at medium effort on Standard found no concrete issues and exercised decoder failure and semantic AV1 checks. Evidence is retained in `~/.agents/verify-notes/convt/evidence/2026-10-05-p11-av1/`. No commits, uploads or hosted workflows ran. The artifacts need a rebuild with production public keys before publication. Mac execution and assembly optimization are NOT CHECKED; assembly remains deferred for architecture-specific verification.
