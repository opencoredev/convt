param([switch]$VerificationOnly)
$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest
$ProgressPreference = 'SilentlyContinue'
$Repo = (Resolve-Path "$PSScriptRoot/../..").Path
Set-Location $Repo
$Lock = Get-Content "$PSScriptRoot/inputs.lock.json" -Raw | ConvertFrom-Json
if (-not $Lock.distribution_ready -and -not $VerificationOnly) {
    throw "Public Windows release blocked: $($Lock.blockers -join '; '). Use -VerificationOnly for unsigned local testing."
}
if (-not $env:SOURCE_DATE_EPOCH) { throw 'SOURCE_DATE_EPOCH required' }
$Date = [DateTimeOffset]::FromUnixTimeSeconds([long]$env:SOURCE_DATE_EPOCH).UtcDateTime.ToString('yyyy-MM-dd')
if ($env:CONVT_BUILD_DATE -and $env:CONVT_BUILD_DATE -ne $Date) { throw 'Build date disagrees with epoch' }
$env:CONVT_BUILD_DATE = $Date
$Out = Join-Path $Repo 'packaging/out/windows/payload'
if (Test-Path $Out) { throw "Output exists: $Out" }
$Cache = Join-Path $Repo 'packaging/cache/windows'
$Work = Join-Path $Repo 'packaging/out/windows/work'
New-Item -ItemType Directory -Force $Cache,$Work,"$Out/licenses" | Out-Null
foreach ($File in $Lock.files) {
    $Dest = Join-Path $Cache $File.name
    if (!(Test-Path $Dest)) { Invoke-WebRequest -Uri $File.url -OutFile $Dest }
    if ((Get-FileHash $Dest -Algorithm SHA256).Hash.ToLowerInvariant() -ne $File.sha256) {
        throw "Hash mismatch: $($File.name); remove the cached file before retrying"
    }
}
Expand-Archive (Join-Path $Cache 'ffmpeg.zip') (Join-Path $Work 'ffmpeg') -Force
$FFmpeg = Get-ChildItem "$Work/ffmpeg" -Recurse -Filter ffmpeg.exe | Select-Object -First 1
Copy-Item "$($FFmpeg.DirectoryName)/ffmpeg.exe","$($FFmpeg.DirectoryName)/ffprobe.exe" $Out
$ErrorActionPreference = 'Continue'
$BuildConfig = & "$Out/ffmpeg.exe" -buildconf 2>&1
$BuildConfigExit = $LASTEXITCODE
$ErrorActionPreference = 'Stop'
$BuildConfig | Out-File "$Out/licenses/ffmpeg-buildconf.txt" -Encoding utf8
if ($BuildConfigExit -ne 0) { throw 'FFmpeg configuration probe failed' }
$Config = Get-Content "$Out/licenses/ffmpeg-buildconf.txt" -Raw
if ($Config -notmatch '--enable-gpl' -or $Config -notmatch '--enable-libx264' -or $Config -match '--enable-nonfree') { throw 'FFmpeg must have GPL libx264 without nonfree components' }
Copy-Item "$($FFmpeg.DirectoryName)/../LICENSE" "$Out/licenses/ffmpeg-LICENSE.txt"
New-Item -ItemType Directory -Force "$Work/pdfium" | Out-Null
tar -xf "$Cache/pdfium.tgz" -C "$Work/pdfium"
if ($LASTEXITCODE -ne 0) { throw 'PDFium extraction failed' }
Copy-Item "$Work/pdfium/bin/pdfium.dll" $Out
Copy-Item "$Work/pdfium/LICENSE" "$Out/licenses/pdfium-LICENSE.txt"
if (Test-Path "$Work/pdfium/licenses") { Copy-Item "$Work/pdfium/licenses" "$Out/licenses/pdfium" -Recurse }
& "$PSScriptRoot/build-native.ps1" -Cache $Cache -Work "$Work/codecs" -Out $Out -Lock $Lock
# lessmsi needs native backslashes in its MSI path, including cabinet lookup.
# Windows PowerShell 5.1 needs a doubled trailing slash in native arguments.
Expand-Archive "$Cache/lessmsi.zip" "$Work/lessmsi" -Force
& "$Work/lessmsi/lessmsi.exe" x (Join-Path $Cache 'libreoffice.msi') "$Work\office\\" | Out-File "$Work/office-extraction.log" -Encoding utf8
if ($LASTEXITCODE -ne 0) { throw 'LibreOffice MSI extraction failed' }
$Office = Get-ChildItem "$Work/office" -Recurse -Filter soffice.com | Select-Object -First 1
if (-not $Office) { throw 'LibreOffice executable missing' }
# The document launcher excludes convt's directory from PATH. Office needs its
# own runtime closure even on a PC without Visual Studio/VC Redistributable.
$RuntimeDlls = Get-ChildItem "$Out/*" -Include 'msvcp*.dll','vcruntime*.dll','concrt*.dll','vccorlib*.dll'
if (-not $RuntimeDlls) { throw 'Document pack CRT closure missing' }
$RuntimeDlls | Copy-Item -Destination $Office.Directory.FullName
rustc --edition=2024 -C opt-level=2 -C target-feature=+crt-static -C link-arg=/Brepro "$PSScriptRoot/document-launcher.rs" -o "$Work/soffice.exe"
if ($LASTEXITCODE -ne 0) { throw 'Document launcher build failed' }
$Hash = python "$PSScriptRoot/build-document-pack.py" $Office.Directory.Parent.FullName "$Work/soffice.exe" "$Out/documents.tar.gz"
if ($LASTEXITCODE -ne 0 -or $Hash -notmatch '^[a-f0-9]{64}$') { throw 'Document archive creation failed' }
$env:CONVT_DOCUMENT_PACK_SHA256 = $Hash
$env:CONVT_DOCUMENT_PACK_URL = 'bundle:documents.tar.gz'
$env:CONVT_DOCUMENT_PACK_VERSION = 'LibreOffice 25.8.7 Windows x64'
$env:CONVT_DOCUMENT_PACK_SIZE = (Get-Item "$Out/documents.tar.gz").Length.ToString()
$env:CONVT_DOCUMENT_PACK_INSTALLED_SIZE = ((Get-ChildItem $Office.Directory.Parent.FullName -Recurse -File | Measure-Object Length -Sum).Sum).ToString()
cargo build --locked --release --target x86_64-pc-windows-msvc -p convt-cli -p convt-app
if ($LASTEXITCODE -ne 0) { throw 'cargo build failed' }
Copy-Item target/x86_64-pc-windows-msvc/release/convt.exe,target/x86_64-pc-windows-msvc/release/convt-app.exe $Out
Copy-Item LICENSE "$Out/LICENSE.txt"
Copy-Item "$PSScriptRoot/inputs.lock.json","$PSScriptRoot/build-native.ps1","$PSScriptRoot/build-document-pack.py","$PSScriptRoot/document-launcher.rs","$Repo/packaging/linux/libheif-explicit-init.patch" "$Out/licenses/"
@{ verification_only = [bool]$VerificationOnly; document_pack_sha256 = $Hash; source_date_epoch = $env:SOURCE_DATE_EPOCH } | ConvertTo-Json | Set-Content "$Out/build-receipt.json" -Encoding utf8
