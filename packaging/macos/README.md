# macOS packaging

These scripts build `convt.app` and its disk image. The release workflow calls them; they also run by hand on a Mac with Xcode.

| Script       | Does                                                                                                                                                                                                                          |
| ------------ | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `bundle.sh`  | Builds the CLI and the GPUI app, takes FFmpeg and ffprobe from a checked source build, downloads the pinned PDFium, compiles the Finder Sync extension with `swiftc`, assembles `convt.app` and signs it from the inside out. |
| `dmg.sh`     | Packs a built app into `convt-<version>-<arch>.dmg` with an Applications link, and signs the image when there is a real identity.                                                                                             |
| `release.sh` | The Developer ID path: arm64 build, hardened runtime, notarize and staple the app, then the same for the image.                                                                                                               |

`inputs.sh` holds every third-party download with its SHA-256, and says where each architecture's FFmpeg comes from. A download that doesn't match is deleted and the build stops. `ffmpeg-input.py` checks a source-built FFmpeg before it goes in the bundle. `release-status.json` is what the release audit reads for the Mac: it stays not ready while it lists a gap.

## Bundle layout

```
convt.app/Contents/
  Info.plist              app.convt.desktop, convt:// scheme, Open With (Alternate rank), Services entry
  MacOS/convt-app         GPUI app (CFBundleExecutable)
  MacOS/convt             CLI; a symlink to it in PATH still finds the bundle's tools
  MacOS/ffmpeg, ffprobe   static FFmpeg built from pinned sources
  Frameworks/libpdfium.dylib
  PlugIns/FinderSync.appex  sandboxed Finder Sync extension
  Resources/convt.icns
  Resources/licenses/FFmpeg/   README.txt, sources.json, build-<arch>.json, components/<package>/ notices
  Resources/licenses/PDFium/   LICENSE.txt, components/ (every notice in the archive), runtime/ (libc++, LLVM)
```

Discovery (`crates/convt-engines`): tools are found through `CONVT_<TOOL>`, then next to the canonical executable (`Contents/MacOS`), then `PATH`; libraries also through `Contents/Frameworks`. LibreOffice comes from the installed document pack, else `/Applications/LibreOffice.app` or `~/Applications/LibreOffice.app`, else `PATH`. HEIC in and out, and AVIF in, go through the system's ImageIO with `/usr/bin/sips`; libheif is not bundled on macOS. AVIF out uses the `image` crate.

## Environment

| Variable                           | Meaning                                                                                                                                                                                                                       |
| ---------------------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `CONVT_MAC_ARCHS`                  | `arm64`, `x86_64` or `arm64 x86_64` (universal). Default: the host's. `release.sh` defaults to `arm64`; the release ships Apple silicon only.                                                                                 |
| `CONVT_MAC_OUT`, `CONVT_MAC_CACHE` | Output and download cache directories.                                                                                                                                                                                        |
| `CARGO_TARGET_DIR`                 | Cargo's target directory. A host-only build uses `release/`; any other arch uses `<triple>/release/`.                                                                                                                         |
| `CONVT_SKIP_CARGO=1`               | Reuse binaries already built.                                                                                                                                                                                                 |
| `CONVT_SIGN_IDENTITY`              | Codesign identity, used as given. Unset: the first Developer ID Application identity, else Apple Development, else ad-hoc. An identity that can't sign here (a locked keychain over SSH) falls back to ad-hoc with a warning. |
| `CONVT_TEAM_ID`                    | Team for the App Group `<team>.app.convt.desktop`. Read from the certificate's OU when unset.                                                                                                                                 |
| `CONVT_HARDENED`                   | Hardened runtime and secure timestamp. On by default for Developer ID.                                                                                                                                                        |
| `CONVT_BUNDLE_CACHE`               | Where `macos-source-ffmpeg-build.sh` wrote `macos-source-built-<arch>`, and where the PDFium source archives for the runtime notices are. Default `packaging/.cache`.                                                         |
| `CONVT_MAC_FFMPEG_DIR_<arch>`      | One source build's directory, overriding that.                                                                                                                                                                                |
| `CONVT_MAC_UNSOURCED_FFMPEG=1`     | Let an architecture with no source build use the prebuilt Riedl FFmpeg. The bundle gets `licenses/FFmpeg/NOT-READY` and `release.sh` refuses it. For local tests only.                                                        |
| `CONVT_PYTHON`                     | Python 3.11+ for the PDFium notice installer. Default `python3`.                                                                                                                                                              |
| `CONVT_BUILD`                      | `CFBundleVersion`. Default `SOURCE_DATE_EPOCH`, else the current Unix time, so it always increases and a release build is repeatable. `CFBundleShortVersionString` is the workspace version.                                  |

Build-time variables the Rust build reads (`CONVT_LICENSE_PUBKEY`, `CONVT_UPDATE_PUBKEY`, `CONVT_DOCUMENT_PACK_*`) pass straight through to cargo.

## Pinned inputs

**FFmpeg 9.0.2** is built from source by `packaging/release/macos-source-ffmpeg-build.sh`, from the nine archives pinned in `packaging/release/macos-source-ffmpeg.lock.json`: FFmpeg 9.0.2, x264 0.164.3108+git31e19f9, x265 3.5, libvpx 1.14.0, opus 1.4, LAME 3.99.5, libogg 1.3.5, libvorbis 1.3.7 and zlib 1.3.1. It links only system libraries and frameworks. `bundle.sh` refuses the build unless its receipt (`provenance.json`) names the right architecture, its binaries still match the receipt, its sources and helper hash match the lock, and every software codec check passed. The binary hashes aren't pinned, because a rebuild with another Xcode gives different bytes; the sources and recipe are.

| Architecture | FFmpeg                     | Status                                                                                        |
| ------------ | -------------------------- | --------------------------------------------------------------------------------------------- |
| arm64        | source build               | Built on macOS 27 / Xcode 27; all receipt checks pass, including VideoToolbox H.264 and HEVC. |
| x86_64       | source build (`inputs.sh`) | Not shipped. The release builds only the arm64 slice.                                         |

Compared with the Riedl build it replaces, the source build has no dav1d, libaom, rav1e, SVT-AV1 or vvenc (AV1 and VVC), no libass, freetype, fontconfig or harfbuzz (subtitles and text), no libwebp, openjpeg, openh264 or theora libraries, no zimg or libvmaf, and no OpenSSL, srt or bluray. convt encodes none of these. The one convt-visible loss is **AV1 input**: FFmpeg's own AV1 decoder needs a hardware accelerator, so AV1 video in MP4, WebM or MKV doesn't convert, and has no thumbnail. It works on M3 and later only with `-hwaccel videotoolbox`, which the engine doesn't pass. Adding pinned dav1d to the source build fixes it on every Mac.

The build also passes `--disable-asm` everywhere, so encoding is slower than an optimized build. On an M5 Pro, 5 seconds of 1080p took 2.1 times as long with x264, 1.6 times with x265 and 2.5 times with VP9 as Homebrew's FFmpeg.

**PDFium chromium/8076** comes from bblanchon/pdfium-binaries (`pdfium-mac-arm64.tgz` `0d6781fe…b97c`, `pdfium-mac-x64.tgz` `40865f34…cca9`, `pdfium-mac-univ.tgz` `3bdb93e2…bdc1`; full hashes in `inputs.sh`). Its source closure is in `packaging/release/pdfium-source.lock.json`. The bundle carries every notice in the archive and the compiler runtime notices from that lock.

## Release gaps

`release-status.json` lists them, and the release audit copies them into `platform_gaps["macos-arm64"]`, so the website never offers a Mac download while one is open. It lists none: the release is arm64 only, and `.github/workflows/release-macos.yml` fails a production run unless `stapler validate` and `spctl --assess` pass for the Developer ID signed, notarized app and disk image. The x86_64 slice is not shipped.

- Mac Rust inventories are generated for both Apple targets, including the SDK-derived objc2 crates. Their exact sources, upstream `LICENSE.md` declaration and Apple SDK caveat, authors/copyright lines, and canonical declared SPDX terms from the pinned SPDX source lock are retained.

Codec patents (H.264, HEVC and AAC through FFmpeg; VideoToolbox and ImageIO cover some of these) are a separate P12 decision.

## Signing and notarization

Local builds use ad-hoc or Apple Development signing and run only on the machine that built them. An ad-hoc build can't create the App Group container (macOS refuses it without a team signature), so its Finder menu only offers "Open in convt…". Over SSH the login keychain is locked, so `bundle.sh` falls back to ad-hoc there; run it from a local Terminal session to sign with Apple Development.

A release needs, from a paid Apple Developer Program membership:

1. A **Developer ID Application** certificate with its private key, exported as a `.p12` for CI.
2. **notarytool credentials**: an App Store Connect API key, stored once with `xcrun notarytool store-credentials convt-notary --key <api-key.p8> --key-id <KEY_ID> --issuer <ISSUER_ID>`.
3. The **Team ID**, which also names the App Group (`<TEAMID>.app.convt.desktop`). Team-prefixed groups need no provisioning profile on macOS.

Then:

```sh
CONVT_SIGN_IDENTITY="Developer ID Application: <Name> (<TEAMID>)" \
CONVT_NOTARY_PROFILE=convt-notary \
SOURCE_DATE_EPOCH=$(git log -1 --format=%ct) \
  packaging/macos/release.sh
```

`release.sh` refuses any identity that isn't Developer ID Application, checks that both notarizations come back Accepted, staples both, and runs `spctl` on the app and the image.
