# Classic Explorer menu smoke. Source checks run anywhere Python is
# available. The install, verb, conversion and uninstall path needs a built
# MSI on Windows. PR #96 owns the PE subsystem helper; this script calls it
# when those files are present.
param(
    [string]$AppExe,
    [string]$CliExe,
    [string]$Msi
)
$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest
$Repo = (Resolve-Path "$PSScriptRoot/../..").Path
Set-Location $Repo

function Test-Windows {
    try {
        return [System.Environment]::OSVersion.Platform -eq 'Win32NT'
    } catch {
        return $false
    }
}

function Invoke-Python {
    param([string[]]$Arguments)
    $Runner = if (Get-Command python -ErrorAction SilentlyContinue) { 'python' } else { 'python3' }
    & $Runner @Arguments
    if ($LASTEXITCODE -ne 0) { throw "python $($Arguments -join ' ') failed" }
}

function Get-VerbCommand {
    param([string]$Extension, [string]$Target)
    $Root = "HKCU:\Software\Classes\SystemFileAssociations\.$Extension\shell\ConvertWithConvt"
    if (-not (Test-Path $Root)) {
        throw "missing Explorer verb at $Root"
    }
    $Parent = Get-ItemProperty $Root
    if ($Parent.MUIVerb -ne 'Convert with Convt') {
        throw "unexpected cascade label '$($Parent.MUIVerb)'"
    }
    if ($Parent.MultiSelectModel -ne 'Player') {
        throw "cascade MultiSelectModel should be Player"
    }
    $Match = Get-ChildItem "$Root\shell" | Where-Object {
        $_.PSChildName -like "*_$Target" -or $_.PSChildName -eq $Target
    } | Select-Object -First 1
    if (-not $Match) {
        throw "no $Target sub-verb under $Root"
    }
    $Command = (Get-ItemProperty "$($Match.PSPath)\command").'(default)'
    if ($Command -notmatch 'convt-app\.exe' -or $Command -match '(?<!convt-app)convt\.exe') {
        throw "verb must launch convt-app.exe: $Command"
    }
    if ($Command -notmatch [regex]::Escape("open --to $Target")) {
        throw "verb must pass open --to ${Target}: $Command"
    }
    return $Command
}

function Assert-ExplorerVerbsInstalled {
    $null = Get-VerbCommand -Extension 'png' -Target 'jpeg'
    $null = Get-VerbCommand -Extension 'png' -Target 'webp'
    foreach ($Extension in @('jpg', 'mp4', 'mp3', 'pdf', 'docx')) {
        $Key = "HKCU:\Software\Classes\SystemFileAssociations\.$Extension\shell\ConvertWithConvt"
        if (-not (Test-Path $Key)) { throw "missing cascade for .$Extension" }
    }
    $SendTo = Join-Path $env:APPDATA 'Microsoft\Windows\SendTo\Convt.lnk'
    if (-not (Test-Path $SendTo)) { throw "missing Send To shortcut $SendTo" }
}

function Assert-ExplorerVerbsRemoved {
    foreach ($Extension in @('png', 'jpg', 'mp4', 'mp3', 'pdf', 'docx')) {
        $Key = "HKCU:\Software\Classes\SystemFileAssociations\.$Extension\shell\ConvertWithConvt"
        if (Test-Path $Key) { throw "uninstall left $Key" }
    }
    $SendTo = Join-Path $env:APPDATA 'Microsoft\Windows\SendTo\Convt.lnk'
    if (Test-Path $SendTo) { throw "uninstall left $SendTo" }
}

function Write-TestPng {
    param([string]$Path)
    [IO.File]::WriteAllBytes($Path, [Convert]::FromBase64String(
        'iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mP8z8BQDwAEhQGAhKmMIQAAAABJRU5ErkJggg=='
    ))
}

function Invoke-VerbCommand {
    param([string]$Command, [string]$File, [string]$Target)
    $Rendered = $Command.Replace('%1', $File)
    if ($Rendered -notmatch '^"([^"]+)"\s+(.+)$') {
        throw "verb command must quote the executable: $Rendered"
    }
    $Exe = $Matches[1]
    if (-not (Test-Path $Exe)) { throw "verb executable missing: $Exe" }
    $Work = Split-Path $File -Parent
    $Output = Join-Path $Work (([IO.Path]::GetFileNameWithoutExtension($File)) + '.jpg')
    $Env:CONVT_LICENSE_STORE = 'file'
    $Env:CONVT_CONFIG_DIR = Join-Path $Work 'cfg'
    $Env:CONVT_DATA_DIR = Join-Path $Work 'data'
    $Env:CONVT_RUNTIME_DIR = Join-Path $Work 'run'
    New-Item -ItemType Directory -Force $Env:CONVT_CONFIG_DIR, $Env:CONVT_DATA_DIR, $Env:CONVT_RUNTIME_DIR | Out-Null
    # Hidden window: the GUI exe (PR #96) owns the subsystem; helpers would
    # spawn it with CREATE_NO_WINDOW. Explorer itself uses this command line.
    $Process = Start-Process -FilePath $Exe -ArgumentList @('open', '--to', $Target, '--', $File) -WorkingDirectory (Split-Path $Exe -Parent) -PassThru -WindowStyle Hidden
    $Deadline = (Get-Date).AddMinutes(2)
    while (-not $Process.HasExited -and (Get-Date) -lt $Deadline) {
        if (Test-Path $Output) { break }
        Start-Sleep -Milliseconds 200
    }
    if (-not $Process.HasExited) {
        Stop-Process -Id $Process.Id -Force
    }
    if (-not (Test-Path $Output) -or (Get-Item $Output).Length -le 0) {
        throw "verb command did not write $Output"
    }
    Write-Host "converted $File -> $Output ($((Get-Item $Output).Length) bytes)"
}

Invoke-Python -Arguments @("$PSScriptRoot/test_explorer_verbs.py")
if (Test-Path "$PSScriptRoot/test_pe_subsystem.py") {
    Invoke-Python -Arguments @("$PSScriptRoot/test_pe_subsystem.py")
}
$Wxs = Get-Content "$PSScriptRoot/convt.wxs" -Raw
if ($Wxs -notmatch 'Target="\[INSTALLFOLDER\]convt-app\.exe"') {
    throw 'convt.wxs Start menu shortcut must target convt-app.exe'
}
if ($Wxs -notmatch 'ComponentGroupRef Id="ExplorerVerbs"') {
    throw 'convt.wxs must include the Explorer verb component group'
}

if (-not $AppExe) {
    $Candidate = Join-Path $Repo 'packaging/out/windows/payload/convt-app.exe'
    if (Test-Path $Candidate) { $AppExe = $Candidate }
}
if (-not $CliExe) {
    $Candidate = Join-Path $Repo 'packaging/out/windows/payload/convt.exe'
    if (Test-Path $Candidate) { $CliExe = $Candidate }
}
if ($AppExe -and (Test-Path "$PSScriptRoot/pe_subsystem.py")) {
    Invoke-Python -Arguments @("$PSScriptRoot/pe_subsystem.py", '--require-gui', $AppExe)
}
if ($CliExe -and (Test-Path "$PSScriptRoot/pe_subsystem.py")) {
    Invoke-Python -Arguments @("$PSScriptRoot/pe_subsystem.py", '--require-console', $CliExe)
}

if (-not $Msi) {
    $Found = Get-ChildItem -ErrorAction SilentlyContinue (Join-Path $Repo 'packaging/out/windows/convt-*-windows-x86_64.msi')
    if ($Found) { $Msi = $Found[0].FullName }
}

if (-not $Msi -or -not (Test-Windows)) {
    Write-Host 'Explorer verb source checks passed (no MSI install on this machine).'
    exit 0
}

$Log = Join-Path ([IO.Path]::GetTempPath()) ("convt-explorer-smoke-{0}.log" -f [guid]::NewGuid())
Write-Host "installing $Msi"
$Install = Start-Process msiexec.exe -ArgumentList @('/i', $Msi, '/qn', '/norestart', "/l*v", $Log) -Wait -PassThru
if ($Install.ExitCode -ne 0) {
    Get-Content $Log -Tail 80
    throw "msiexec install failed: $($Install.ExitCode)"
}
$Work = Join-Path ([IO.Path]::GetTempPath()) ("convt-explorer-smoke-{0}" -f [guid]::NewGuid())
New-Item -ItemType Directory -Force $Work | Out-Null
try {
    Assert-ExplorerVerbsInstalled
    $Png = Join-Path $Work 'sample.png'
    Write-TestPng $Png
    $Command = Get-VerbCommand -Extension 'png' -Target 'jpeg'
    Invoke-VerbCommand -Command $Command -File $Png -Target 'jpeg'
} finally {
    Write-Host "uninstalling $Msi"
    $Remove = Start-Process msiexec.exe -ArgumentList @('/x', $Msi, '/qn', '/norestart') -Wait -PassThru
    if ($Remove.ExitCode -ne 0) {
        throw "msiexec uninstall failed: $($Remove.ExitCode)"
    }
    Remove-Item -Recurse -Force $Work -ErrorAction SilentlyContinue
}
Assert-ExplorerVerbsRemoved
Write-Host 'Windows Explorer menu smoke passed'
