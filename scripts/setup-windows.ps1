# Dev setup for Windows. Run in PowerShell from the repo root.
$ErrorActionPreference = "Stop"

winget install --id Microsoft.VisualStudio.2022.BuildTools --override "--quiet --wait --add Microsoft.VisualStudio.Workload.VCTools --includeRecommended" --accept-package-agreements
winget install --id Rustlang.Rustup --accept-package-agreements
winget install --id Oven-sh.Bun --accept-package-agreements
winget install --id Gyan.FFmpeg --accept-package-agreements
if ($env:WITH_LIBREOFFICE -ne "0") { winget install --id TheDocumentFoundation.LibreOffice --accept-package-agreements }

$env:Path = [Environment]::GetEnvironmentVariable("Path", "Machine") + ";" + [Environment]::GetEnvironmentVariable("Path", "User")
rustup show active-toolchain
bun install

# PDFium
$dest = "vendor\pdfium"
if (Test-Path $dest) { Remove-Item -Recurse -Force $dest }
New-Item -ItemType Directory -Force $dest | Out-Null
$tgz = Join-Path $env:TEMP "pdfium-win-x64.tgz"
Invoke-WebRequest "https://github.com/bblanchon/pdfium-binaries/releases/latest/download/pdfium-win-x64.tgz" -OutFile $tgz
tar -xzf $tgz -C $dest
# Windows ships the DLL in bin/, the engine looks in lib/
Copy-Item "$dest\bin\pdfium.dll" "$dest\lib\" -Force
Write-Host "Done. Try: cargo run -p convt-cli -- formats"
