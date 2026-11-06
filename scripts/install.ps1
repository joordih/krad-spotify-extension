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

# After a release, extension.toml points at the release downloads; point it
# at the local builds so Krad ADE runs what was just built (same result as
# scripts/binaries.py local).
$platforms = [ordered]@{
    darwin_arm64 = "krad-spotify-darwin-arm64"; darwin_x64 = "krad-spotify-darwin-x64"
    linux_x64 = "krad-spotify-linux-x64"; linux_arm64 = "krad-spotify-linux-arm64"
    win_x64 = "krad-spotify-win-x64.exe"; win_arm64 = "krad-spotify-win-arm64.exe"
}
$entries = foreach ($target in $platforms.Keys) {
    "[[program.binary]]`ntargets = [`"$target`"]`npath = `"bin/$($platforms[$target])`"`n"
}
$block = "# BEGIN BINARIES (local builds; run scripts/binaries.py to change)`n" + ($entries -join "`n") + "# END BINARIES"
$manifest = Get-Content extension.toml -Raw
$updated = [regex]::Replace($manifest, '(?ms)^# BEGIN BINARIES.*?^# END BINARIES[^\r\n]*', { param($m) $block })
if ($updated -ne $manifest) {
    [IO.File]::WriteAllText((Join-Path (Get-Location) "extension.toml"), $updated)
    Write-Host "extension.toml now uses the binaries in bin/; do not commit that change"
    Write-Host "(restore the release manifest with: git checkout extension.toml)"
}
Write-Host "Reload the Spotify extension in Krad ADE to pick up the new binary."
