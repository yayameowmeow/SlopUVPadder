# SPDX-License-Identifier: GPL-3.0-or-later
#
# Packages the GUI build as a portable dist/uvpad-VERSION-windows-ARCH.zip:
# uvpad.exe with the Qt libraries, QML modules and MSVC runtime next to it.
#
#   packaging/windows/package.ps1 -Binary target/release/uvpad.exe -Notices NOTICES -Arch x64
#
# Run from a Visual Studio developer shell (for the runtime DLLs), with the Qt
# to bundle on PATH.
param(
    [Parameter(Mandatory)] [string] $Binary,
    [Parameter(Mandatory)] [string] $Notices,
    [Parameter(Mandatory)] [ValidateSet("x64", "arm64")] [string] $Arch
)
$ErrorActionPreference = "Stop"

$root = Resolve-Path "$PSScriptRoot/../.."
$version = (Select-String -Path "$root/Cargo.toml" -Pattern '^version = "(.*)"').Matches[0].Groups[1].Value
$name = "uvpad-$version-windows-$Arch"
$out = "$root/target/windows/$name"

if (Test-Path $out) { Remove-Item -Recurse -Force $out }
New-Item -ItemType Directory -Force $out | Out-Null
Copy-Item $Binary "$out/uvpad.exe"

# Qt libraries, plugins and the QML modules the app imports. Direct3D is the
# default renderer on Windows and its compiler ships with Windows 10 and later.
windeployqt --release --qmldir "$root/qml" --no-translations `
    --no-system-d3d-compiler --no-opengl-sw "$out/uvpad.exe"
if ($LASTEXITCODE -ne 0) { throw "windeployqt failed" }

# The MSVC runtime, so nothing needs installing (app-local deployment).
$crt = Get-ChildItem "$env:VCToolsRedistDir/$Arch/Microsoft.VC*.CRT" | Select-Object -First 1
if (-not $crt) { throw "MSVC runtime not found; run from a VS developer shell" }
Copy-Item "$crt/*.dll" $out

Copy-Item "$root/LICENSE" "$out/LICENSE.txt"
Copy-Item $Notices "$out/THIRD-PARTY-NOTICES.txt"
Copy-Item "$root/README.md" "$out/README.md"

New-Item -ItemType Directory -Force "$root/dist" | Out-Null
$zip = "$root/dist/$name.zip"
if (Test-Path $zip) { Remove-Item $zip }
Compress-Archive -Path $out -DestinationPath $zip
Write-Host "Built $zip"
