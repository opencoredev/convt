# Extract the per-user MSI and convert one image and one video with the
# bundled CLI. The GUI is started only long enough to prove the process lives;
# a headless runner may not keep a GPUI window up, so that check is best-effort.
$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest
$Repo = (Resolve-Path "$PSScriptRoot/../..").Path
Set-Location $Repo
$Msi = Get-Item "$Repo/packaging/out/windows/convt-*-windows-x86_64.msi" | Select-Object -First 1
if (-not $Msi) { throw 'MSI is missing; run installer.ps1 first' }
$Limit = 120MB
if ($Msi.Length -ge $Limit) {
    throw ("{0} is {1:N0} bytes ({2:N1} MiB); the runtime MSI must stay under 120 MiB. v0.2.0 was 537 MiB because it harvested documents.tar.gz and codec sources." -f $Msi.Name, $Msi.Length, ($Msi.Length / 1MB))
}
Write-Host ("MSI {0}: {1:N0} bytes ({2:N1} MiB)" -f $Msi.Name, $Msi.Length, ($Msi.Length / 1MB))
$LessMsi = Get-ChildItem "$Repo/packaging/out/windows/work/lessmsi" -Recurse -Filter lessmsi.exe | Select-Object -First 1
if (-not $LessMsi) { throw 'lessmsi.exe missing from the Windows build work directory' }
$Extract = Join-Path $Repo 'packaging/out/windows/msi-extract'
if (Test-Path $Extract) { throw "Output exists: $Extract" }
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
$Work = Join-Path $Repo 'packaging/out/windows/smoke'
New-Item -ItemType Directory -Force "$Work/in","$Work/out" | Out-Null
$env:CONVT_LICENSE_STORE = 'file'
$env:CONVT_CONFIG_DIR = Join-Path $Work 'cfg'
$env:CONVT_DATA_DIR = Join-Path $Work 'data'
$env:CONVT_PDFIUM_DIR = $Bin
& "$Bin/convt.exe" engines
if ($LASTEXITCODE -ne 0) { throw 'convt engines failed' }
& "$Bin/ffmpeg.exe" -v error -nostdin -y -f lavfi -i color=c=red:s=32x32:d=0.1 "$Work/in/sample.png"
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
$App = Join-Path $Bin 'convt-app.exe'
$Gui = $null
try {
    $Gui = Start-Process -FilePath $App -PassThru -WindowStyle Hidden
    Start-Sleep -Seconds 5
    if ($Gui.HasExited) {
        Write-Host "convt-app.exe exited $($Gui.ExitCode) on this runner; CLI conversions still passed."
    } else {
        Write-Host "convt-app.exe stayed running (pid $($Gui.Id))"
        Stop-Process -Id $Gui.Id
    }
} catch {
    Write-Host "convt-app.exe launch skipped: $_"
} finally {
    if ($Gui -and -not $Gui.HasExited) { Stop-Process -Id $Gui.Id -ErrorAction SilentlyContinue }
}
Write-Host 'Windows MSI smoke test passed'
