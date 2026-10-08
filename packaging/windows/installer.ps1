$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest
$Repo = (Resolve-Path "$PSScriptRoot/../..").Path
Set-Location $Repo
$Version = ((Get-Content Cargo.toml | Select-String '^version = "([0-9.]+)"$' | Select-Object -First 1).Matches[0].Groups[1].Value)
$Payload = Join-Path $Repo 'packaging/out/windows/payload'
if (!(Test-Path "$Payload/build-receipt.json") -or !(Test-Path "$Payload/convt-app.exe")) { throw 'Build verified payload first' }
$WixVersion = '6.0.2'
$Tools = Join-Path $Repo 'packaging/cache/windows/wix'
if (!(Test-Path "$Tools/wix.exe")) {
    dotnet tool install --tool-path $Tools wix --version $WixVersion --add-source https://api.nuget.org/v3/index.json
    if ($LASTEXITCODE -ne 0) { throw 'WiX installation failed' }
}
$CachedVersion = (& "$Tools/wix.exe" --version | Out-String).Trim()
if ($LASTEXITCODE -ne 0 -or ($CachedVersion -split '\+')[0] -ne $WixVersion) {
    throw "Cached WiX version '$CachedVersion' does not match pin $WixVersion. Remove $Tools and rerun."
}
python "$PSScriptRoot/gen_explorer_verbs.py"
if ($LASTEXITCODE -ne 0) { throw 'Explorer verb generation failed' }
& "$Tools/wix.exe" build -arch x64 -d "Version=$Version" -d "Payload=$Payload" "$PSScriptRoot/convt.wxs" "$PSScriptRoot/explorer-verbs.wxs" -o "packaging/out/windows/convt-$Version-windows-x86_64.msi"
if ($LASTEXITCODE -ne 0) { throw 'WiX installer build failed' }
