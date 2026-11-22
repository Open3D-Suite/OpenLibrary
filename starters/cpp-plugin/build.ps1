# SPDX-License-Identifier: GPL-3.0-or-later
#
# Builds OpenExample's library and its tests with Visual Studio (MSVC, plus the CMake and Ninja that come
# with it), then runs the tests. With -Plugin it also builds the 3ds Max plug-in and puts the bundle 3ds Max
# loads together.
#
#   powershell -ExecutionPolicy Bypass -File build.ps1 [-Config Release|Debug] [-BuildDir <dir>] [-NoTests]
#                                                      [-Clean] [-Plugin] [-MaxSdk <maxsdk folder>]
#                                                      [-Bundle <folder>]
#
# The build goes to <repo>\build\<config> unless -BuildDir says otherwise. To build somewhere else every
# time (a local disk when the repo is on a network share, for instance) create build.local.ps1 next to
# this file with the line  $BuildRoot = "<a folder>"  - Git ignores that file.
#
# -Plugin needs the 3ds Max 2027 SDK and a Release build. The SDK's "maxsdk" folder comes from -MaxSdk,
# else from $MaxSdk in build.local.ps1, else from the ADSK_3DSMAX_SDK_2027 environment variable the SDK's
# installer sets. The bundle is put together in -Bundle, by default <build>\bundle\OpenExample.bundle,
# ready to be copied into an ApplicationPlugins folder; nothing is installed. It is refused when
# PackageContents.xml does not say the version of CMakeLists.txt, or when its codes are not GUIDs.
param(
    [ValidateSet("Release", "Debug")]
    [string]$Config = "Release",
    [string]$BuildDir = "",
    [switch]$NoTests,
    [switch]$Clean,
    [switch]$Plugin,
    [string]$MaxSdk = "",
    [string]$Bundle = ""
)
$ErrorActionPreference = "Stop"
$src = $PSScriptRoot

$BuildRoot = Join-Path $src "build"
$localSettings = Join-Path $src "build.local.ps1"
$MaxSdkArgument = $MaxSdk
if (Test-Path $localSettings) { . $localSettings }
if ($MaxSdkArgument) { $MaxSdk = $MaxSdkArgument }
if (-not $MaxSdk) { $MaxSdk = $env:ADSK_3DSMAX_SDK_2027 }
if (-not $BuildDir) { $BuildDir = Join-Path $BuildRoot $Config.ToLower() }
if (-not $Bundle) { $Bundle = Join-Path $BuildDir "bundle\OpenExample.bundle" }
elseif (-not [IO.Path]::IsPathRooted($Bundle)) { $Bundle = Join-Path $src $Bundle }

$package = Join-Path $src "bundle\OpenExample.bundle\PackageContents.xml"
if ($Plugin) {
    if ($Config -ne "Release") { throw "-Plugin needs -Config Release: 3ds Max only loads plug-ins built against the release C runtime." }
    if (-not $MaxSdk -or -not (Test-Path (Join-Path $MaxSdk "include\max.h"))) {
        throw "3ds Max SDK not found (looked in '$MaxSdk'). Pass -MaxSdk <path to the SDK's maxsdk folder>, or set `$MaxSdk in build.local.ps1."
    }
    # The version is written in three places (CMakeLists.txt, which also stamps the plug-in, and twice in
    # the package description), and the bundle's codes are made once for OpenExample: refuse to build a
    # bundle that is not right.
    $version = [regex]::Match((Get-Content (Join-Path $src "CMakeLists.txt") -Raw), 'project\(OpenExample VERSION ([0-9.]+)').Groups[1].Value
    $description = ([xml](Get-Content $package -Raw)).ApplicationPackage
    $entry = $description.Components.ComponentEntry
    if ($description.AppVersion -ne $version -or $entry.Version -ne $version) {
        throw "Version mismatch: CMakeLists.txt says $version, PackageContents.xml says $($description.AppVersion) and $($entry.Version)"
    }
    foreach ($name in @("ProductCode", "UpgradeCode")) {
        if ($description.$name -notmatch '^\{[0-9A-Fa-f]{8}(-[0-9A-Fa-f]{4}){3}-[0-9A-Fa-f]{12}\}$') {
            throw "PackageContents.xml: $name is $($description.$name), not a GUID in braces made for OpenExample. Make one with  [guid]::NewGuid()"
        }
    }
    if ($description.ProductCode -eq $description.UpgradeCode) { throw "PackageContents.xml: ProductCode and UpgradeCode must differ" }
}

# Visual Studio (any edition with the C++ tools): ask vswhere, else the usual place of the 2022 Build Tools.
$vs = $null
$vswhere = Join-Path ${env:ProgramFiles(x86)} "Microsoft Visual Studio\Installer\vswhere.exe"
if (Test-Path $vswhere) {
    $vs = & $vswhere -latest -products * -requires Microsoft.VisualStudio.Component.VC.Tools.x86.x64 `
        -property installationPath | Select-Object -First 1
}
if (-not $vs) { $vs = Join-Path ${env:ProgramFiles(x86)} "Microsoft Visual Studio\2022\BuildTools" }
$vcvars = Join-Path $vs "VC\Auxiliary\Build\vcvars64.bat"
if (-not (Test-Path $vcvars)) { throw "Visual Studio C++ tools not found (looked for $vcvars)" }
# vcvars64.bat calls vswhere.exe by name; with its folder on PATH it does not complain about it.
$installer = Split-Path $vswhere
if (Test-Path $installer) { $env:PATH = "$env:PATH;$installer" }

# cmake and ninja are called by their full path: putting them on PATH with "set PATH=...;%PATH%" in the
# same cmd line as vcvars64.bat would be expanded before vcvars runs and wipe the compiler's PATH.
$cmake = Join-Path $vs "Common7\IDE\CommonExtensions\Microsoft\CMake\CMake\bin\cmake.exe"
$ninja = Join-Path $vs "Common7\IDE\CommonExtensions\Microsoft\CMake\Ninja\ninja.exe"
if (-not (Test-Path $cmake)) { $cmake = (Get-Command cmake.exe -ErrorAction Stop).Source }
if (-not (Test-Path $ninja)) { $ninja = (Get-Command ninja.exe -ErrorAction Stop).Source }

if ($Clean -and (Test-Path $BuildDir)) { Remove-Item -Recurse -Force $BuildDir }
New-Item -ItemType Directory -Force $BuildDir | Out-Null

# The plug-in is switched on or off explicitly, so that a build folder does not remember the last choice.
$pluginArgs = "-DOPENEXAMPLE_BUILD_PLUGIN=OFF"
if ($Plugin) { $pluginArgs = "-DOPENEXAMPLE_BUILD_PLUGIN=ON `"-DOPENEXAMPLE_MAXSDK=$($MaxSdk -replace '\\', '/')`"" }

$cmd = "call `"$vcvars`" >nul && " +
       "`"$cmake`" -S `"$src`" -B `"$BuildDir`" -G Ninja -DCMAKE_BUILD_TYPE=$Config `"-DCMAKE_MAKE_PROGRAM=$ninja`" $pluginArgs && " +
       "`"$cmake`" --build `"$BuildDir`""
cmd /c $cmd
if ($LASTEXITCODE -ne 0) { throw "Build failed with exit code $LASTEXITCODE" }
Write-Output "Built ($Config): $BuildDir\bin"

if (-not $NoTests) {
    & (Join-Path $BuildDir "bin\openexample_tests.exe")
    if ($LASTEXITCODE -ne 0) { throw "Tests failed with exit code $LASTEXITCODE" }
}

if ($Plugin) {
    # The bundle: the package description from the repo plus the plug-in just built.
    $gup = Join-Path $BuildDir "plugin\OpenExample.gup"
    if (-not (Test-Path $gup)) { throw "The plug-in was not built: $gup is missing" }
    if (Test-Path $Bundle) { Remove-Item -Recurse -Force $Bundle }
    $pluginDir = Join-Path $Bundle "Contents\plugins\2027"
    New-Item -ItemType Directory -Force $pluginDir | Out-Null
    Copy-Item $package (Join-Path $Bundle "PackageContents.xml")
    Copy-Item $gup (Join-Path $pluginDir "OpenExample.gup")
    Write-Output "Bundle ($version): $Bundle"
}
