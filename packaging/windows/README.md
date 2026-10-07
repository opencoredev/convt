# Windows verification installer

Run in a Windows x64 checkout with Rust 1.95.0, Visual Studio 2019, 2022 or 2026 C++ build tools (including CMake), Git for Windows, Python 3 and the .NET SDK. The scripts use only user-owned build directories. Git supplies patch and Perl for the codec builds; Strawberry Perl's `patch.exe` is refused (it asserts on the x265 debian series).

```powershell
$env:SOURCE_DATE_EPOCH = '1791244800'
./packaging/windows/build.ps1 -VerificationOnly
./packaging/windows/installer.ps1
```

`build.ps1` verifies every downloaded or cached archive against `inputs.lock.json`, builds the patched codecs, repackages the official LibreOffice MSI, and compiles convt with that document archive's digest. It writes `packaging/out/windows/payload` (runtime files only) and `packaging/out/windows/documents.tar.gz` beside it. An existing payload is refused; move the previous owned `packaging/out/windows` directory aside before rebuilding. `installer.ps1` stages the runtime harvest, installs WiX 6.0.2 into the checkout's cache, and produces a per-user MSI under `packaging/out/windows`. The harvest refuses `documents.tar.gz`, codec source trees, PDBs, `.lib` files and headers. `python packaging/windows/stage_payload.py` is the allowlist; `packaging/windows/smoke.ps1` extracts the MSI and converts a generated PNG and MP4.

The MSI installs into `%LOCALAPPDATA%\Programs\convt` and adds a Start menu shortcut. It needs no machine-wide LibreOffice or codec installation. Images, SVG, PDF, video and audio engines find their dependencies beside the installed executable. The optional LibreOffice archive is not inside the MSI (same as Mac and Linux). After a hosted pack URL exists, `convt.exe pack install documents` downloads it; until then a local proof uses `convt pack install documents --source file:///absolute/path/documents.tar.gz --sha256 <compiled-digest>`. Pack removal leaves a system LibreOffice untouched. Uninstalling the MSI removes its payload and shortcut and preserves user data.

The artifact is unsigned. Public GitHub Release still builds with `-VerificationOnly` and may attach that unsigned per-user MSI for /download; `inputs.lock.json` keeps `distribution_ready=false` until FFmpeg/PDFium corresponding-source is collected. Do not treat `-VerificationOnly` as publication clearance. See [the licence inventory](../../docs/licence-inventory.md). Signing remains P11, and Explorer integration is CNV-11.
