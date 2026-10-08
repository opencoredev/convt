# Extract the per-user MSI, convert an image and a video, install the
# document pack from the sibling release archive, convert a Word file to
# PDF, and prove convt-app.exe stays running. With -AppExe / -CliExe,
# only the PE subsystem and Start-menu shortcut are checked (CI desktop).
param(
    [string]$AppExe,
    [string]$CliExe
)
$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest
$Repo = (Resolve-Path "$PSScriptRoot/../..").Path
Set-Location $Repo
$PeOnly = $PSBoundParameters.ContainsKey('AppExe') -or $PSBoundParameters.ContainsKey('CliExe')

python "$PSScriptRoot/test_pe_subsystem.py"
if ($LASTEXITCODE -ne 0) { throw 'packaging/windows unit tests failed' }
$Wxs = Get-Content "$PSScriptRoot/convt.wxs" -Raw
if ($Wxs -notmatch 'Target="\[INSTALLFOLDER\]convt-app\.exe"') {
    throw 'convt.wxs Start menu shortcut must target convt-app.exe'
}

function Test-PeSubsystems {
    param([string]$Gui, [string]$Cli)
    if ($Gui) {
        python "$PSScriptRoot/pe_subsystem.py" --require-gui $Gui
        if ($LASTEXITCODE -ne 0) { throw 'convt-app.exe must use the WINDOWS_GUI subsystem' }
    }
    if ($Cli) {
        python "$PSScriptRoot/pe_subsystem.py" --require-console $Cli
        if ($LASTEXITCODE -ne 0) { throw 'convt.exe must stay a console binary' }
    }
}

if (-not $AppExe) {
    $Candidate = Join-Path $Repo 'packaging/out/windows/payload/convt-app.exe'
    if (Test-Path $Candidate) { $AppExe = $Candidate }
}
if (-not $CliExe) {
    $Candidate = Join-Path $Repo 'packaging/out/windows/payload/convt.exe'
    if (Test-Path $Candidate) { $CliExe = $Candidate }
}
if ($AppExe -or $CliExe) {
    Test-PeSubsystems -Gui $AppExe -Cli $CliExe
} elseif ($PeOnly) {
    Write-Host 'No convt-app.exe to inspect; source checks passed.'
}

if ($PeOnly) {
    Write-Host 'Windows GUI subsystem smoke passed'
    return
}

function Remove-OwnedDir {
    param([string]$Path, [string]$Parent, [string]$Prefix)
    if (-not $Path) { return }
    $Full = [System.IO.Path]::GetFullPath($Path)
    $OwnedRoot = [System.IO.Path]::GetFullPath($Parent)
    $Name = [System.IO.Path]::GetFileName($Full)
    if (-not $Name.StartsWith($Prefix)) { return }
    if ([System.IO.Path]::GetDirectoryName($Full) -ne $OwnedRoot) { return }
    $Item = Get-Item -LiteralPath $Full -ErrorAction SilentlyContinue
    if (-not $Item) { return }
    if ($Item.Attributes -band [IO.FileAttributes]::ReparsePoint) { return }
    Remove-Item -LiteralPath $Full -Recurse -Force
}

$Msi = Get-Item "$Repo/packaging/out/windows/convt-*-windows-x86_64.msi" | Select-Object -First 1
if (-not $Msi) { throw 'MSI is missing; run installer.ps1 first' }
$Limit = 120MB
if ($Msi.Length -ge $Limit) {
    throw ("{0} is {1:N0} bytes ({2:N1} MiB); the runtime MSI must stay under 120 MiB. v0.2.0 was 537 MiB because it harvested documents.tar.gz and codec sources." -f $Msi.Name, $Msi.Length, ($Msi.Length / 1MB))
}
Write-Host ("MSI {0}: {1:N0} bytes ({2:N1} MiB)" -f $Msi.Name, $Msi.Length, ($Msi.Length / 1MB))
$LessMsi = Get-ChildItem "$Repo/packaging/out/windows/work/lessmsi" -Recurse -Filter lessmsi.exe | Select-Object -First 1
if (-not $LessMsi) { throw 'lessmsi.exe missing from the Windows build work directory' }
$RunId = [guid]::NewGuid().ToString('N')
$WindowsOut = Join-Path $Repo 'packaging/out/windows'
$Extract = Join-Path $WindowsOut "msi-extract-$RunId"
$Work = Join-Path $WindowsOut "smoke-$RunId"
$SmokeHome = $null
$Gui = $null
if (Test-Path -LiteralPath $Extract) { throw "Output exists: $Extract" }
try {
New-Item -ItemType Directory -Force $Extract | Out-Null
& $LessMsi.FullName x $Msi.FullName "$Extract\\"
if ($LASTEXITCODE -ne 0) { throw 'lessmsi extraction failed' }
$Forbidden = Get-ChildItem $Extract -Recurse -File | Where-Object {
    $_.Name -eq 'documents.tar.gz' -or $_.Name -like '*-documents.tar.gz' -or $_.Extension -in '.pdb','.lib','.h','.hpp' -or $_.FullName -match '[\\/]native-source[\\/]'
}
if ($Forbidden) {
    throw ("Extracted MSI still contains non-runtime files:`n{0}" -f (($Forbidden | ForEach-Object { $_.FullName.Substring($Extract.Length + 1) }) -join "`n"))
}
$Convt = Get-ChildItem $Extract -Recurse -Filter convt.exe | Select-Object -First 1
if (-not $Convt) { throw 'Extracted MSI has no convt.exe' }
$Bin = $Convt.Directory.FullName
$ExtractedApp = Join-Path $Bin 'convt-app.exe'
if (-not (Test-Path -LiteralPath $ExtractedApp)) { throw 'Extracted MSI has no convt-app.exe' }
Test-PeSubsystems -Gui $ExtractedApp -Cli $Convt.FullName
New-Item -ItemType Directory -Force "$Work/in","$Work/out" | Out-Null
# GitHub's workspace is on D:\, whose volume root is not a trusted pack
# ancestor. Install into a unique per-run profile directory.
if (-not $env:LOCALAPPDATA) { throw 'LOCALAPPDATA is required to install the document pack' }
$SmokeHome = Join-Path $env:LOCALAPPDATA "convt-smoke-$RunId"
if (Test-Path -LiteralPath $SmokeHome) { throw "Output exists: $SmokeHome" }
New-Item -ItemType Directory -Force $SmokeHome | Out-Null
$env:CONVT_LICENSE_STORE = 'file'
$env:CONVT_CONFIG_DIR = Join-Path $SmokeHome 'cfg'
$env:CONVT_DATA_DIR = Join-Path $SmokeHome 'data'
$env:CONVT_PDFIUM_DIR = $Bin
& "$Bin/convt.exe" engines
if ($LASTEXITCODE -ne 0) { throw 'convt engines failed' }
# color is 25 fps; without -frames:v 1 the image2 muxer refuses a second
# frame for an unnumbered PNG and the required smoke step fails.
& "$Bin/ffmpeg.exe" -v error -nostdin -y -f lavfi -i color=c=red:s=32x32:d=0.1 -frames:v 1 "$Work/in/sample.png"
if ($LASTEXITCODE -ne 0) { throw 'ffmpeg image fixture failed' }
& "$Bin/ffmpeg.exe" -v error -nostdin -y -f lavfi -i testsrc=size=32x32:rate=10:duration=1 -f lavfi -i sine=frequency=440:duration=1 -c:v libx264 -c:a aac "$Work/in/sample.mp4"
if ($LASTEXITCODE -ne 0) { throw 'ffmpeg video fixture failed' }
& "$Bin/convt.exe" "$Work/in/sample.png" --to webp -o "$Work/out"
if ($LASTEXITCODE -ne 0 -or -not (Test-Path "$Work/out/sample.webp")) { throw 'image conversion failed' }
& "$Bin/convt.exe" "$Work/in/sample.mp4" --to webm -o "$Work/out"
if ($LASTEXITCODE -ne 0 -or -not (Test-Path "$Work/out/sample.webm")) { throw 'video conversion failed' }
Get-Item "$Work/out/sample.webp","$Work/out/sample.webm" | ForEach-Object {
    if ($_.Length -lt 32) { throw "$($_.Name) is too small" }
    Write-Host ("{0}: {1} bytes" -f $_.Name, $_.Length)
}
$Pack = Get-Item "$Repo/packaging/out/windows/convt-*-windows-x86_64-documents.tar.gz" | Select-Object -First 1
$Checksum = Get-Item "$Repo/packaging/out/windows/convt-*-windows-x86_64-documents.tar.gz.sha256" | Select-Object -First 1
if (-not $Pack -or -not $Checksum) { throw 'Windows document pack release assets are missing' }
$PackHash = (Get-FileHash $Pack.FullName -Algorithm SHA256).Hash.ToLowerInvariant()
$Recorded = (($Checksum | Get-Content -TotalCount 1) -split '\s+')[0].ToLowerInvariant()
if ($PackHash -ne $Recorded) { throw "Document pack checksum mismatch: $PackHash vs $Recorded" }
$Receipt = Get-Content "$Bin/build-receipt.json" -Raw | ConvertFrom-Json
if ($Receipt.document_pack_sha256 -ne $PackHash) {
    throw "Compiled document pack digest $($Receipt.document_pack_sha256) does not match $($Pack.Name)"
}
if ($Receipt.document_pack_url -notmatch '^https://github.com/.+/releases/download/v[^/]+/convt-[^/]+-windows-x86_64-documents\.tar\.gz$') {
    throw "Compiled document pack URL is not the GitHub release asset: $($Receipt.document_pack_url)"
}
$Status = & "$Bin/convt.exe" pack status documents
if ($LASTEXITCODE -ne 0) { throw 'convt pack status failed' }
if ($Status -notmatch 'pack install documents') {
    throw "Document pack is not compiled as a downloadable add-on:`n$Status"
}
Write-Host $Status
Write-Host ("document pack {0}: {1:N0} bytes, sha256 {2}" -f $Pack.Name, $Pack.Length, $PackHash)
# The GitHub asset is not on the release yet during a dry-run. Install the
# same archive the release will publish, through the pinned CLI source.
$PackUri = 'file://' + $Pack.FullName
& "$Bin/convt.exe" pack install documents --source $PackUri --sha256 $PackHash
if ($LASTEXITCODE -ne 0) { throw 'document pack install failed' }
$Installed = & "$Bin/convt.exe" pack status documents
if ($LASTEXITCODE -ne 0) { throw 'convt pack status failed after install' }
if ($Installed -notmatch 'documents: installed') {
    throw "Document pack did not install:`n$Installed"
}
Write-Host $Installed
$Docx = Join-Path "$Work/in" 'sample.docx'
python -c @"
import zipfile
from pathlib import Path
path = Path(r'$Docx')
with zipfile.ZipFile(path, 'w') as z:
    z.writestr('[Content_Types].xml', '''<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types">
  <Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/>
  <Default Extension="xml" ContentType="application/xml"/>
  <Override PartName="/word/document.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml"/>
</Types>''')
    z.writestr('_rels/.rels', '''<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">
  <Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="word/document.xml"/>
</Relationships>''')
    z.writestr('word/document.xml', '''<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">
  <w:body><w:p><w:r><w:t>Known document text 12345</w:t></w:r></w:p></w:body>
</w:document>''')
    z.writestr('word/_rels/document.xml.rels', '''<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"/>''')
"@
if ($LASTEXITCODE -ne 0 -or -not (Test-Path $Docx)) { throw 'Word fixture failed' }
& "$Bin/convt.exe" $Docx --to pdf -o "$Work/out"
if ($LASTEXITCODE -ne 0 -or -not (Test-Path "$Work/out/sample.pdf")) { throw 'document conversion failed' }
$Pdf = Get-Item "$Work/out/sample.pdf"
$PdfHead = [System.IO.File]::ReadAllBytes($Pdf.FullName)[0..4]
if ($Pdf.Length -lt 32 -or [System.Text.Encoding]::ASCII.GetString($PdfHead) -ne '%PDF-') {
    throw ("{0} is not a PDF ({1} bytes)" -f $Pdf.Name, $Pdf.Length)
}
Write-Host ("{0}: {1} bytes" -f $Pdf.Name, $Pdf.Length)
$Gui = Start-Process -FilePath $ExtractedApp -PassThru -WindowStyle Hidden
if (-not $Gui) { throw 'convt-app.exe did not start' }
    Start-Sleep -Seconds 5
    if ($Gui.HasExited) {
        throw "convt-app.exe exited $($Gui.ExitCode) after launch"
    }
    Write-Host "convt-app.exe stayed running (pid $($Gui.Id))"
    Write-Host 'Windows MSI smoke test passed'
} finally {
    if ($Gui -and -not $Gui.HasExited) { Stop-Process -Id $Gui.Id -ErrorAction SilentlyContinue }
    if ($SmokeHome) { Remove-OwnedDir -Path $SmokeHome -Parent $env:LOCALAPPDATA -Prefix 'convt-smoke-' }
    Remove-OwnedDir -Path $Extract -Parent $WindowsOut -Prefix 'msi-extract-'
    Remove-OwnedDir -Path $Work -Parent $WindowsOut -Prefix 'smoke-'
}
