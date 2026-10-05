# Linux bundle verification

Verified on 2026-10-04 with SOURCE_DATE_EPOCH=1791072000 (build date 2026-10-04). The x86_64 CLI, GUI and codec closure build in the digest-pinned manylinux_2_28 / AlmaLinux 8 image in `build-image.json` and target glibc 2.28. Cargo builds offline with Rust 1.95.0 and Cargo.lock; binary inputs, source packages, security patches and compiler runtimes are hash-locked.

After the payload is complete, `container-build.sh` runs `elf-audit.py` on it. The audit checks every ELF file under the payload, including the static ffmpeg and ffprobe, and fails the build if any file needs a GLIBC symbol version above 2.28, has an unresolved dependency, or resolves a library from outside the payload other than the host glibc ABI (libc, libm, libpthread, libdl, librt, libresolv, libutil, libmvec and the loader). It reports every violation, not only the first. It also audits the validation tools against their own library directories. Before either audit, `elf-audit.py --self-test` builds four small ELF cases and fails the build unless the audit accepts a clean pair of libraries and rejects a GLIBC_2.99 version need, a dependency resolved through an RPATH outside the root, and an unresolved library. A fifth case links a program against the builder's own glibc: it must pass in the 2.28 builder and fail on a newer host. On this host (glibc 2.39) the self-test rejected it as needing GLIBC_2.34. The audited build passed 20 payload ELF files and 110 validation-tool ELF files.

The GCC 15.2 runtime targets glibc 2.17 and exports GLIBCXX_3.4.34. It supplies newer C++ symbols for desktop drivers while preserving the bundle baseline. Its exact package recipes, metadata and GCC licence texts are retained.

## Build date and reproducibility

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

## Results

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

## Security and HEIC regressions

Every requested finding has a failing-before and passing-after regression. Pack discovery requires the compiled digest, matching pointer and receipt, current-user ownership, protected permissions and a regular executable launcher. It rejects symlinks and externally mutable ancestors. Unix sticky ancestors such as /tmp are allowed when the next component is protected; the pack itself has no group/world write permissions. Installation always rebuilds from a digest-verified archive, including when a matching receipt exists. Status explains why an installation is rejected. Removal validates roots, locks and candidates, and removes safe staging orphans. HTTP redirects remain HTTPS-only; loopback HTTP exists only in unit-test builds and cannot redirect.

Full tests also reproduced a lock-descriptor inheritance race. PackLock now explicitly unlocks on operation completion, so a transient child descriptor cannot block an immediate reinstall. The duplicated-descriptor regression failed before the fix and passes afterward. Independent final reviews reported no remaining concrete findings in the reviewed changes.

The bundled libheif patch exposes secure initialization without automatic plugin loading. Convt then loads only absolute executable-relative bundle directories and an absolute CONVT_LIBHEIF_PLUGIN_DIR. Relative overrides and inherited LIBHEIF_PLUGIN_PATH are ignored. Unpatched system libraries are unavailable rather than using their implicit loader. Constructor-sentinel tests cover absolute and relative inherited paths, the working directory and a relative explicit override. No process environment mutation is needed.

HEIC tests cover quality, alpha, size, orientation, PNG colour-metadata precedence, NCLX transfer/primaries, custom-primary conversion to sRGB and white-point adaptation, including zero-Y primaries. Linear-gamma PNG retains a linear NCLX tag. Untagged input is tagged sRGB. Sixteen-bit input attempts 10-bit encoding. The shipped x265 build reports unsupported depth, and the tested HEIC fallback is 8-bit. A separately built local multilib x265 verified actual 10-bit HEIC; the old implementation failed that regression. AVIF preserves 10-bit output. The final shipped-codec HEIC suite passed 15 tests with one test-only worker entry ignored. See [HEIC notes](../../docs/heic.md).

x265 is built with ENABLE_LIBNUMA=OFF, and convt sets only x265:frame-threads=1, since a still image has one frame. With libnuma, x265 binds every pool worker with set_mempolicy whenever a pool exists, and no encoder parameter avoids that. A pool-enabled encode with the earlier libnuma build printed 152 "set_mempolicy: Operation not permitted" lines in a default-seccomp container, which is why the previous build disabled the pool. Without libnuma the pool stays on. A 6000 by 4000 PNG to HEIC encode on a 32-thread host took 37.2 to 37.8 seconds with the previous build and 4.0 seconds (Ubuntu 22.04) and 5.4 seconds (Debian 12) with the current one, offline and with Docker's default seccomp profile, with no set_mempolicy lines. The bitstream differs because rows now encode in parallel; output is deterministic across runs and distributions, decoded PSNR against the source is unchanged (21.9167 dB both), and the two decodes are 63 dB apart. Clap always prints convt, including from convt.bin.

## Layout and sizes

The tarball and AppImage use the same payload: convt and convt-app launchers with their .bin executables, static ffmpeg and ffprobe, lib/ with PDFium and the native closure, lib/libheif/plugins/, share/ desktop metadata, and licenses/ notices and provenance. SHA-256 sidecars verify both archives. Future deb and rpm installers can reuse the payload.

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

## Desktop requirements and release limits

The host supplies glibc and its loader, including libmvec when needed. X11 linkage libraries are bundled. Rendering requires an X11 display or Wayland compositor and XKB data; libvulkan.so.1 and a vendor ICD/driver/device, or libEGL.so.1 and an OpenGL implementation; libwayland-client.so.0 and libwayland-egl.so.1 for Wayland; and libfontconfig.so.1 with its font stack, configuration and fonts. Exact linkage and runtime requirements are in [INVENTORY.md](INVENTORY.md). An ldd pass establishes library resolution, not rendering.

[Document-pack design](../../docs/document-pack.md) defines the separately owned app dialog, explicit download action, progress, completion, retry, updates and removal. Real hosting and pinned release packs remain gates. LibreOffice redistribution requires MPL-2.0 and its third-party obligations; the local proof is not a release pack built from pinned source packages.

[Licence inventory](INVENTORY.md) records AGPL convt, GPL FFmpeg and x265, LGPL HEIF/de265, BSD PDFium/aom, the GCC runtime exception, MIT AppImage runtime and MPL LibreOffice. Complete corresponding source, patches, third-party notices and Rust dependency inventory remain public-distribution gates. Patent review is separate. Native macOS HEIC and macOS/Windows bundles remain unverified; Windows pack ownership checks fail closed pending native support. Deb/rpm installers remain future work.

Detailed evidence is in packaging/out/verification/, including reproducible-build/, x265-threading/, matrix-ubuntu-22.04/ and host-checks-2026-10-04/. Earlier failed regressions, the host-check setup collision with a rebuilding native directory, the reproduced lock race, and the optional Alma registry timeout remain recorded. The successful host rerun used an immutable native payload. No real HEIC samples are bundled. Owned containers and image aliases were removed.
