param([string]$Cache, [string]$Work, [string]$Out, $Lock)
$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest
$Repo = (Resolve-Path "$PSScriptRoot/../..").Path
$Prefix = Join-Path $Work 'native'
New-Item -ItemType Directory -Force $Prefix | Out-Null
$VSWhere = "${env:ProgramFiles(x86)}/Microsoft Visual Studio/Installer/vswhere.exe"
# Prefer VS 2022 when both are installed; windows-latest now ships VS 2026 (18).
$VisualStudio = & $VSWhere -version '[17.0,18.0)' -latest -products '*' -requires Microsoft.VisualStudio.Component.VC.Tools.x86.x64 -format json | ConvertFrom-Json | Select-Object -First 1
if (-not $VisualStudio) {
    $VisualStudio = & $VSWhere -latest -products '*' -requires Microsoft.VisualStudio.Component.VC.Tools.x86.x64 -format json | ConvertFrom-Json | Select-Object -First 1
}
if (-not $VisualStudio) { throw 'Visual Studio C++ build tools required' }
$Major = [int]($VisualStudio.installationVersion -split '\.')[0]
$Generator = switch ($Major) {
    16 { 'Visual Studio 16 2019' }
    17 { 'Visual Studio 17 2022' }
    18 { 'Visual Studio 18 2026' }
    default { throw "Unsupported Visual Studio version $Major" }
}
$CMake = Get-Command cmake.exe -ErrorAction SilentlyContinue | Select-Object -ExpandProperty Source
if (-not $CMake) { $CMake = Join-Path $VisualStudio.installationPath 'Common7/IDE/CommonExtensions/Microsoft/CMake/CMake/bin/cmake.exe' }
if (!(Test-Path $CMake)) { throw 'CMake required (Visual Studio C++ CMake tools or PATH)' }
# Prefer Git for Windows' GNU patch. windows-latest puts Strawberry Perl
# first on PATH; its patch 2.5.9 asserts `hunk` on the x265 debian series
# (0001-Fix-arm-flags.patch). That ARM CMake tweak is unused on this x64
# ENABLE_ASSEMBLY=OFF build, but the full Ubuntu series is still applied
# for the same corresponding-source closure as Linux.
$GitPatch = Join-Path $env:ProgramFiles 'Git/usr/bin/patch.exe'
if (Test-Path -LiteralPath $GitPatch) {
    $Patch = $GitPatch
} else {
    $Patch = (Get-Command patch.exe -ErrorAction SilentlyContinue | Select-Object -ExpandProperty Source)
}
if (-not $Patch -or -not (Test-Path -LiteralPath $Patch)) {
    throw 'patch.exe required (Git for Windows)'
}
if ($Patch -match '(?i)[\\/]Strawberry[\\/]') {
    throw "Unusable patch.exe from Strawberry Perl: $Patch. Install Git for Windows."
}
function Native($Name, $Subdir, $Options) {
    $Source = Join-Path $Work $Name
    New-Item -ItemType Directory -Force $Source | Out-Null
    $Original = $Lock.files | Where-Object { $_.package -eq $Name -and $_.kind -eq 'source' }
    $Patches = $Lock.files | Where-Object { $_.package -eq $Name -and $_.kind -eq 'patches' }
    tar -xf (Join-Path $Cache $Original.name) --strip-components=1 -C $Source
    if ($LASTEXITCODE -ne 0) { throw "Extract $Name" }
    tar -xf (Join-Path $Cache $Patches.name) -C $Source
    if ($LASTEXITCODE -ne 0) { throw "Extract $Name patches" }
    $Series = Join-Path $Source 'debian/patches/series'
    if (Test-Path $Series) {
        foreach ($Line in Get-Content $Series) {
            $Line = ($Line -split '#')[0].Trim()
            if ($Line) {
                & $Patch -d $Source -p1 --batch -i (Join-Path $Source "debian/patches/$(($Line -split '\s+')[0])")
                if ($LASTEXITCODE -ne 0) { throw "Patch $Name $Line" }
            }
        }
    }
    if ($Name -eq 'libheif') {
        & $Patch -d $Source -p1 --batch -i "$Repo/packaging/linux/libheif-explicit-init.patch"
        if ($LASTEXITCODE -ne 0) { throw 'Secure libheif patch failed' }
    }
    if ($Name -eq 'x265') {
        # x265 3.5 forces CMP0025/CMP0054 to OLD, which CMake 4 (VS 2026) rejects.
        $CMakeLists = Join-Path $Source 'source/CMakeLists.txt'
        $Text = [System.IO.File]::ReadAllText($CMakeLists)
        $Updated = $Text.Replace('SET CMP0025 OLD', 'SET CMP0025 NEW').Replace('SET CMP0054 OLD', 'SET CMP0054 NEW')
        if ($Updated -ne $Text) {
            $Utf8 = New-Object System.Text.UTF8Encoding $false
            [System.IO.File]::WriteAllText($CMakeLists, $Updated, $Utf8)
        }
    }
    $Build = Join-Path $Source 'build-convt'
    # Quote / array-splat every -D. pwsh splits an unquoted -DFOO=3.5 into
    # CMAKE_POLICY_VERSION_MINIMUM=3 and a stray path ".5" (CNV-37 follow-up).
    $CMakeArgs = @(
        '-S', (Join-Path $Source $Subdir)
        '-B', $Build
        '-G', $Generator
        '-A', 'x64'
        "-DCMAKE_INSTALL_PREFIX=$Prefix"
        "-DCMAKE_PREFIX_PATH=$Prefix"
        '-DCMAKE_INSTALL_LIBDIR=lib'
        '-DCMAKE_POLICY_VERSION_MINIMUM=3.5'
    ) + @($Options)
    & $CMake @CMakeArgs
    if ($LASTEXITCODE -ne 0) { throw "Configure $Name" }
    & $CMake --build $Build --config Release --parallel 4
    if ($LASTEXITCODE -ne 0) { throw "Build $Name" }
    & $CMake --install $Build --config Release
    if ($LASTEXITCODE -ne 0) { throw "Install $Name" }
}
Native 'x265' 'source' @('-DENABLE_SHARED=ON','-DENABLE_CLI=OFF','-DENABLE_ASSEMBLY=OFF','-DENABLE_LIBNUMA=OFF')
Native 'libde265' '.' @('-DBUILD_SHARED_LIBS=ON','-DENABLE_DEC265=OFF','-DENABLE_SDL=OFF')
Native 'aom' '.' @('-DBUILD_SHARED_LIBS=ON','-DENABLE_TESTS=OFF','-DENABLE_EXAMPLES=OFF','-DENABLE_TOOLS=OFF','-DENABLE_DOCS=OFF','-DCONFIG_LIBYUV=0','-DCONFIG_WEBM_IO=0','-DENABLE_NASM=OFF','-DAOM_TARGET_CPU=generic',"-DPERL_EXECUTABLE=$env:ProgramFiles/Git/usr/bin/perl.exe")
# libheif 1.17.6 (and this Ubuntu series) has no BUILD_DOCUMENTATION /
# WITH_DOXYGEN option. find_package(Doxygen) adds doc_doxygen ALL, which
# runs Graphviz `dot` (missing on windows-latest). Skip that lookup.
Native 'libheif' '.' @('-DBUILD_SHARED_LIBS=ON','-DBUILD_TESTING=OFF','-DWITH_EXAMPLES=OFF','-DENABLE_PLUGIN_LOADING=OFF','-DCMAKE_DISABLE_FIND_PACKAGE_Doxygen=ON','-DWITH_LIBDE265=ON','-DWITH_LIBDE265_PLUGIN=OFF','-DWITH_X265=ON','-DWITH_X265_PLUGIN=OFF','-DWITH_AOM_DECODER=ON','-DWITH_AOM_DECODER_PLUGIN=OFF','-DWITH_AOM_ENCODER=ON','-DWITH_AOM_ENCODER_PLUGIN=OFF','-DWITH_DAV1D=OFF','-DWITH_RAV1E=OFF','-DWITH_SvtEnc=OFF','-DWITH_LIBSHARPYUV=OFF')
$Dlls = @(Get-ChildItem $Prefix -Recurse -Filter '*.dll')
if ($Dlls.Count -lt 4) { throw 'Missing codec DLL closure' }
$Dlls | Copy-Item -Destination $Out
# App-local CRT avoids a machine-wide runtime installation/UAC prompt.
$CRT = Get-ChildItem (Join-Path $VisualStudio.installationPath 'VC/Redist/MSVC') -Directory | Where-Object { $_.Name -match '^\d' } | Sort-Object Name -Descending | Select-Object -First 1
$Runtime = Get-ChildItem (Join-Path $CRT.FullName 'x64') -Directory -Filter 'Microsoft.VC*.CRT' | Select-Object -First 1
if (-not $Runtime) { throw 'MSVC redistributable CRT missing' }
Copy-Item "$($Runtime.FullName)/*.dll" $Out
# Preserve corresponding codec sources, excluding generated build products.
foreach ($Name in @('x265','libde265','aom','libheif')) {
    $Dest = "$Out/licenses/native-source/$Name"
    New-Item -ItemType Directory -Force $Dest | Out-Null
    Get-ChildItem (Join-Path $Work $Name) | Where-Object { $_.Name -ne 'build-convt' } | Copy-Item -Destination $Dest -Recurse
}
