$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest
$Repo = (Resolve-Path "$PSScriptRoot/../..").Path
Set-Location $Repo
$Version = ((Get-Content Cargo.toml | Select-String '^version = "([0-9.]+)"$' | Select-Object -First 1).Matches[0].Groups[1].Value)
$Payload = Join-Path $Repo 'packaging/out/windows/payload'
if (-not (Test-Path "$Payload/convt-app.exe")) { throw 'Build audited payload first' }
$Tools = Join-Path $env:RUNNER_TEMP 'convt-wix'
dotnet tool install --tool-path $Tools wix --version 6.0.2
if ($LASTEXITCODE -ne 0) { throw 'WiX installation failed' }
& "$Tools/wix.exe" build -arch x64 -d "Version=$Version" -d "Payload=$Payload" "$PSScriptRoot/convt.wxs" -o "packaging/out/windows/convt-$Version-windows-x86_64.msi"
if ($LASTEXITCODE -ne 0) { throw 'WiX installer build failed' }
