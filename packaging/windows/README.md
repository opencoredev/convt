# Windows verification installer

Run in a Windows x64 checkout with Rust 1.95.0, Visual Studio 2019 or 2022 C++ build tools (including CMake), Git for Windows, Python 3 and the .NET SDK. The scripts use only user-owned build directories. Git supplies patch and Perl for the codec builds.

```powershell
$env:SOURCE_DATE_EPOCH = '1791244800'
./packaging/windows/build.ps1 -VerificationOnly
./packaging/windows/installer.ps1
```

`build.ps1` verifies every downloaded or cached archive against `inputs.lock.json`, builds the patched codecs, repackages the official LibreOffice MSI, and compiles convt with that document archive's digest. It writes `packaging/out/windows/payload`. An existing payload is refused; move the previous owned `packaging/out/windows` directory aside before rebuilding. `installer.ps1` installs WiX 6.0.2 into the checkout's cache and produces a per-user MSI under `packaging/out/windows`.

The MSI installs into `%LOCALAPPDATA%\Programs\convt` and adds a Start menu shortcut. It needs no machine-wide LibreOffice or codec installation. Images, SVG, PDF, video and audio engines find their dependencies beside the installed executable. Run `convt.exe pack install documents` to explicitly install the bundled, pinned document archive into the user's convt data directory. Pack removal leaves a system LibreOffice untouched. Uninstalling the MSI removes its payload and shortcut and preserves user data.

The artifact is unsigned. Public-release builds remain blocked because the full corresponding sources for the prebuilt FFmpeg and PDFium archives have not been collected. Do not treat `-VerificationOnly` as publication clearance. See [the licence inventory](../../docs/licence-inventory.md). Signing remains P11, and Explorer integration is CNV-11.

## Public Release MSI

The Release workflow builds with `-VerificationOnly` and publishes an **unsigned**
per-user MSI when the Windows job succeeds. That MSI is attached to the GitHub
release and listed on /download without claiming code signing or a complete
FFmpeg/PDFium corresponding-source closure. `inputs.lock.json` keeps
`distribution_ready=false` until those source archives are collected; Mac and
Linux publication stays gated on their own audited source closures.

