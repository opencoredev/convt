$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest
$Repo = (Resolve-Path "$PSScriptRoot/../..").Path
Set-Location $Repo
$Lock = Get-Content "$PSScriptRoot/inputs.lock.json" -Raw | ConvertFrom-Json
# The manifest remains fail-closed until real Windows native inputs are locked.
if (-not $Lock.distribution_ready -or $Lock.files.Count -eq 0) {
    throw "Windows release blocked: $($Lock.blockers -join '; ')"
}
if (-not $env:SOURCE_DATE_EPOCH) { throw 'SOURCE_DATE_EPOCH required' }
$Date = [DateTimeOffset]::FromUnixTimeSeconds([long]$env:SOURCE_DATE_EPOCH).UtcDateTime.ToString('yyyy-MM-dd')
if ($env:CONVT_BUILD_DATE -and $env:CONVT_BUILD_DATE -ne $Date) { throw 'Build date disagrees with epoch' }
$env:CONVT_BUILD_DATE = $Date
$Out = Join-Path $Repo 'packaging/out/windows/payload'
if (Test-Path $Out) { throw "Output exists: $Out" }
New-Item -ItemType Directory -Path $Out | Out-Null
cargo build --locked --release --target x86_64-pc-windows-msvc -p convt-cli -p convt-app
if ($LASTEXITCODE -ne 0) { throw 'cargo build failed' }
Copy-Item target/x86_64-pc-windows-msvc/release/convt.exe,target/x86_64-pc-windows-msvc/release/convt-app.exe $Out
foreach ($File in $Lock.files) {
    if (-not $File.source_sha256 -or -not $File.recipe -or -not $File.notice) { throw "Incomplete sources/notices: $($File.name)" }
    $Dest = Join-Path $Out $File.name
    Invoke-WebRequest -Uri $File.url -OutFile $Dest
    if ((Get-FileHash $Dest -Algorithm SHA256).Hash.ToLower() -ne $File.sha256) { throw "Hash mismatch: $($File.name)" }
    Copy-Item (Join-Path $Repo $File.notice) (Join-Path $Out "$($File.name).LICENSE.txt")
}
Copy-Item LICENSE (Join-Path $Out 'LICENSE.txt')
