# Builds the extension in release mode and installs the binary into bin\ (Windows).
#
# Windows locks a running .exe, so close or disable the Spotify extension in
# Krad ADE before installing, then reload it.
#
# Usage: scripts\install.ps1 [-Arch x64|arm64]   (default: this machine's architecture)
param(
    [ValidateSet("x64", "arm64")]
    [string]$Arch = $(if ($env:PROCESSOR_ARCHITECTURE -eq "ARM64") { "arm64" } else { "x64" })
)

$ErrorActionPreference = "Stop"
Set-Location (Join-Path $PSScriptRoot "..")

$target = @{ x64 = "x86_64-pc-windows-msvc"; arm64 = "aarch64-pc-windows-msvc" }[$Arch]
$dest = "bin\krad-spotify-win-$Arch.exe"

cargo build --release --target $target
if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }

# Build output lives outside the extension folder (see .cargo/config.toml).
$targetDir = (cargo metadata --format-version 1 --no-deps | ConvertFrom-Json).target_directory

New-Item -ItemType Directory -Force -Path bin | Out-Null
$tmp = "$dest.tmp"
Copy-Item (Join-Path $targetDir "$target\release\krad-spotify.exe") $tmp -Force
Move-Item $tmp $dest -Force
Write-Host "installed $dest"
Write-Host "Reload the Spotify extension in Krad ADE to pick up the new binary."
