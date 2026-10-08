# Check that the Windows GUI binary is WINDOWS_GUI, the CLI stays a console
# tool, and the Start-menu shortcut targets convt-app.exe.
param(
    [string]$AppExe,
    [string]$CliExe
)
$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest
$Repo = (Resolve-Path "$PSScriptRoot/../..").Path
Set-Location $Repo
python "$PSScriptRoot/test_pe_subsystem.py"
if ($LASTEXITCODE -ne 0) { throw 'packaging/windows unit tests failed' }
$Wxs = Get-Content "$PSScriptRoot/convt.wxs" -Raw
if ($Wxs -notmatch 'Target="\[INSTALLFOLDER\]convt-app\.exe"') {
    throw 'convt.wxs Start menu shortcut must target convt-app.exe'
}
if (-not $AppExe) {
    $Candidate = Join-Path $Repo 'packaging/out/windows/payload/convt-app.exe'
    if (Test-Path $Candidate) { $AppExe = $Candidate }
}
if (-not $CliExe) {
    $Candidate = Join-Path $Repo 'packaging/out/windows/payload/convt.exe'
    if (Test-Path $Candidate) { $CliExe = $Candidate }
}
if ($AppExe) {
    python "$PSScriptRoot/pe_subsystem.py" --require-gui $AppExe
    if ($LASTEXITCODE -ne 0) { throw 'convt-app.exe must use the WINDOWS_GUI subsystem' }
} else {
    Write-Host 'No convt-app.exe to inspect; source checks passed.'
}
if ($CliExe) {
    python "$PSScriptRoot/pe_subsystem.py" --require-console $CliExe
    if ($LASTEXITCODE -ne 0) { throw 'convt.exe must stay a console binary' }
}
Write-Host 'Windows GUI subsystem smoke passed'
