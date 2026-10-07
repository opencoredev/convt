# Windows verification installer

Run in a Windows x64 checkout with Rust 1.95.0, Visual Studio 2019 or 2022 C++ build tools (including CMake), Git for Windows, Python 3 and the .NET SDK. The scripts use only user-owned build directories. Git supplies patch and Perl for the codec builds.

FFmpeg is cross-built from pinned sources on Linux first. `build-ffmpeg.sh` needs Docker; it verifies every tarball in `ffmpeg-source.lock.json` and builds FFmpeg with x264, x265, libvpx, opus, LAME, Vorbis, dav1d and zlib (GPL, no nonfree) using the lock's pinned Ubuntu image and mingw-w64 packages. Copy its output to `packaging/cache/windows/ffmpeg-build` on the Windows machine, or point `CONVT_WINDOWS_FFMPEG` at it.

```sh
bash packaging/windows/build-ffmpeg.sh "$(mktemp -d)/cache" /tmp/convt-ffmpeg-build
```

```powershell
$env:SOURCE_DATE_EPOCH = '1791244800'
./packaging/windows/build.ps1 -VerificationOnly
./packaging/windows/installer.ps1
```

`build.ps1` accepts the FFmpeg only if its receipt matches the lock and the binaries' hashes, checks its build configuration and every encoder convt uses, verifies every downloaded or cached archive against `inputs.lock.json`, builds the patched codecs, repackages the official LibreOffice MSI, and compiles convt with that document archive's digest. It writes `packaging/out/windows/payload`. An existing payload is refused; move the previous owned `packaging/out/windows` directory aside before rebuilding. `installer.ps1` installs WiX 6.0.2 into the checkout's cache and produces a per-user MSI under `packaging/out/windows`.

The MSI installs into `%LOCALAPPDATA%\Programs\convt` and adds a Start menu shortcut. It needs no machine-wide LibreOffice or codec installation. Images, SVG, PDF, video and audio engines find their dependencies beside the installed executable. Run `convt.exe pack install documents` to explicitly install the bundled, pinned document archive into the user's convt data directory. Pack removal leaves a system LibreOffice untouched. Uninstalling the MSI removes its payload and shortcut and preserves user data.

`source-archive.py` writes `convt-VERSION-windows-source.tar.gz`: every pinned third-party source tarball built into the MSI, the build recipes and the FFmpeg build record. The release workflow publishes it beside the MSI, and `scripts/release/assemble.py` refuses the MSI unless that archive holds every source in both locks. PDFium (BSD) and LibreOffice (MPL-2.0, repackaged unmodified) ship with their notices and source links in `licenses/SOURCES.txt`.

The MSI is unsigned, so Windows shows a SmartScreen warning; that is accepted for release. `release-status.json` records Windows readiness for the source audit. Explorer integration is CNV-11.
