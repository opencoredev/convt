# macOS FFmpeg source replacement

The existing Martin Riedl FFmpeg 9.0.2 archives match the pins, but their complete corresponding source remains blocked. Both architectures embed x265 `4.0+1-6318f22`; the public 9.0.2 recipe specifies x265 4.2. Its x264 input also moves with `master`. `macos-source-ffmpeg.lock.json` records the binary hashes, configuration strings, candidate recipe and exact evidence gaps.

The replacement helper builds FFmpeg 9.0.2 with nine pinned source libraries: x264, x265, libvpx, opus, LAME, libogg, libvorbis, zlib and dav1d. It preserves the software encoders selected by convt, PNG and explicit VideoToolbox support. It omits the vendor's additional AV1 encoders, subtitle renderers, bluray, OpenSSL and other extras.

On the Mac builder, install the declared Xcode/SDK, Python 3.12+, CMake, make, pkg-config, Meson >=0.49 and Ninja prerequisites. Set the cache to the retained source downloads. Verify/fetch inputs before disabling network access for the build:

```sh
export CONVT_BUNDLE_CACHE=/absolute/path/to/convt/packaging/.cache
python3 packaging/release/pdfium-source-verify.py --fetch packaging/release/macos-source-ffmpeg.lock.json
SOURCE_DATE_EPOCH=1791331200 MACOSX_DEPLOYMENT_TARGET=11.0 \
  bash packaging/release/macos-source-ffmpeg-build.sh --arch arm64
SOURCE_DATE_EPOCH=1791331200 MACOSX_DEPLOYMENT_TARGET=11.0 \
  bash packaging/release/macos-source-ffmpeg-build.sh --arch x86_64
```

Each build refuses an existing output directory. Use `--output` with a fresh directory inside the cache for another build. The helper does not download sources. It uses private static libraries and disables FFmpeg dependency autodetection, rejects undeclared dynamic libraries, installs component notices and retains source/configuration/build logs.

Outputs are `macos-source-built-<arch>/bin/ffmpeg`, `bin/ffprobe`, `licenses/` and `provenance.json` inside the cache. The receipt contains actual output paths, SHA-256 hashes, source pins, the helper hash, toolchain identity, dynamic dependencies and conversion results. It attempts AVC, HEVC, VP9, MP3, Opus, Vorbis, PNG and ffprobe smoke checks. VideoToolbox checks remain explicit when the builder cannot supply the required encoder. A target architecture that cannot execute on that Mac is reported as NOT CHECKED. Cross builds use portable codec implementations to avoid an undeclared assembler prerequisite; x265's replacement configuration is 8-bit.

Actual macOS compilation is NOT CHECKED from this Linux session. The macOS owner must run the helper, inspect both receipts, perform the platform checks and replace the bundle's input pins and notice installation under their ownership. These new source inputs cannot establish corresponding source for the historical Riedl binaries. Include both the retained research evidence and the new matching source/recipe/notice closure in release source delivery, with separate platform status.

## arm64 result

The arm64 build ran on macOS 27.0 with Xcode 27.0 (SDK 27.0), Apple clang 21, CMake 4.4.3 and Homebrew Python 3.14. The system Python 3.9 can't run the helper: its `tarfile` has no `filter='data'`. Two helper changes were needed, so the lock's recipe hash changed with them. libvorbis 1.3.7's configure passes `-force_cpusubtype_ALL`, which the current linker rejects, and x265 3.5 sets CMake policies CMP0025 and CMP0054 to OLD, which CMake 4 refuses. The helper now removes that flag and those two policies, and keeps AppleClang on x265's Clang path. Every receipt check passed, including `h264_videotoolbox` and `hevc_videotoolbox`; the binaries link only `/usr/lib` and system frameworks. `packaging/macos` uses this build for arm64. x86_64 remains NOT CHECKED.

The configuration string embedded in the binaries records the builder's absolute paths (`--prefix` and the `-I`/`-L` flags), so `ffmpeg -version` shows them.

## AV1 decoder rebuild

The first arm64 replacement passed 579 matrix cases, but those fixtures had no AV1 video. The new recipe adds dav1d 1.5.3, pinned by source archive SHA-256 `732010aa5ef461fa93355ed2c6c5fedb48ddc4b74e697eaabe8907eaeb943011`, and enables `--enable-libdav1d`. Its exact COPYING notice is retained with the source. Meson builds a static PIC decoder for the explicitly selected Apple target; optional assembly, tools and tests are disabled. The helper retains Meson and Ninja versions, verifies the decoder is present and rejects non-system dynamic libraries. The arm64 libvorbis and CMake 4 fixes above remain in place.

P4 must fetch and verify the updated lock, install the declared Meson and Ninja prerequisites, and build into fresh output directories using `--output`. The prior arm64 output is refused deliberately. Re-run the full conversion matrix with the generated AV1 MP4, WebM and MKV input cases, including video outputs, GIF and PNG thumbnails. Set `CONVT_MATRIX_AV1_FFMPEG` to an absolute path to a separate trusted FFmpeg with the libaom AV1 encoder when the shipped build has no AV1 encoder. The new bundle only needs dav1d to decode those inputs. Update Mac binary pins and retained notices after the new receipts and matrix pass, then repeat for x86_64. Actual Mac compilation of this decoder change is NOT CHECKED here.

For the arm64 rebuild, use a fresh cache output and verify the installed decoder before running the matrix against the rebuilt bundle:

```sh
SOURCE_DATE_EPOCH=1791331200 MACOSX_DEPLOYMENT_TARGET=11.0 \
  bash packaging/release/macos-source-ffmpeg-build.sh --arch arm64 \
  --output "$CONVT_BUNDLE_CACHE/macos-source-built-arm64-av1"
"$CONVT_BUNDLE_CACHE/macos-source-built-arm64-av1/bin/ffmpeg" -hide_banner -decoders
```

The decoder list must contain `libdav1d`. After P4 updates and rebuilds the Mac bundle, run `CONVT_MATRIX_AV1_FFMPEG=/absolute/path/to/trusted/ffmpeg bun run test:matrix` with `CONVT_FFMPEG`, `CONVT_FFPROBE` and the native library overrides pointing to that new bundle. Repeat with a fresh x86_64 output on a builder that can execute that target. Retain both provenance receipts and matrix reports.

Linux already ships a pinned libaom software AV1 decoder. Generated AV1 MP4, WebM and MKV inputs decoded successfully with hardware acceleration disabled, so its native FFmpeg source closure needs no new dependency.

The portable Mac build still disables codec assembly. P4 measured x264, x265 and VP9 encodes at 1.6 to 2.5 times the earlier build's duration. Adding nasm alone would not restore the arm64 assembler paths and would broaden the toolchain change. Assembly optimization is deferred until separate architecture-specific build and performance verification can cover it.
