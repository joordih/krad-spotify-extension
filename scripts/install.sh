#!/bin/sh
# Builds the extension in release mode and installs the binary into bin/
# (macOS and Linux; on Windows use scripts/install.ps1).
#
# The binary is copied to a temporary file and moved into place, so the
# installed file always gets a new inode. On macOS, overwriting it in place
# while Krad ADE runs the old one makes the system kill the new process with
# "Code Signature Invalid".
#
# Usage: scripts/install.sh [arm64|x64|all]   (default: this machine's architecture)
set -eu

cd "$(dirname "$0")/.."

case "$(uname -s)" in
  Darwin) os="darwin" ;;
  Linux) os="linux" ;;
  *) echo "unsupported OS: $(uname -s); on Windows use scripts/install.ps1" >&2; exit 2 ;;
esac

case "${1:-$(uname -m)}" in
  arm64 | aarch64) arches="arm64" ;;
  x64 | x86_64 | amd64) arches="x64" ;;
  all) arches="arm64 x64" ;;
  *) echo "usage: $0 [arm64|x64|all]" >&2; exit 2 ;;
esac

# Build output lives outside the extension folder (see .cargo/config.toml).
target_dir=$(cargo metadata --format-version 1 --no-deps |
  sed -n 's/.*"target_directory":"\([^"]*\)".*/\1/p')

mkdir -p bin
for arch in $arches; do
  case "$os-$arch" in
    darwin-arm64) target="aarch64-apple-darwin" ;;
    darwin-x64) target="x86_64-apple-darwin" ;;
    linux-arm64) target="aarch64-unknown-linux-gnu" ;;
    linux-x64) target="x86_64-unknown-linux-gnu" ;;
  esac
  dest="bin/krad-spotify-$os-$arch"

  cargo build --release --target "$target"

  tmp="$dest.tmp.$$"
  cp "$target_dir/$target/release/krad-spotify" "$tmp"
  mv -f "$tmp" "$dest"
  if [ "$os" = "darwin" ]; then
    codesign --verify "$dest"
  fi
  echo "installed $dest"
done

# After a release, extension.toml points at the release downloads; point it
# at the local builds so Krad ADE runs what was just built.
python3 scripts/binaries.py local >/dev/null
if ! git diff --quiet -- extension.toml 2>/dev/null; then
  echo "extension.toml now uses the binaries in bin/; do not commit that change"
  echo "(restore the release manifest with: git checkout extension.toml)"
fi

echo "Reload the Spotify extension in Krad ADE to pick up the new binary."
