#!/usr/bin/env bash
# Builds build/AutoSort.app on macOS.
#   ./scripts/bundle-macos.sh              native architecture
#   UNIVERSAL=1 ./scripts/bundle-macos.sh  Apple Silicon + Intel in one app
set -euo pipefail
cd "$(dirname "$0")/.."

if [[ "$(uname)" != "Darwin" ]]; then echo "This script must run on macOS." >&2; exit 1; fi

if [[ "${UNIVERSAL:-0}" == "1" ]]; then
  rustup target add aarch64-apple-darwin x86_64-apple-darwin
  cargo build --release --target aarch64-apple-darwin
  cargo build --release --target x86_64-apple-darwin
  BIN="build/autosort-universal"
  mkdir -p build
  lipo -create -output "$BIN" \
    target/aarch64-apple-darwin/release/autosort \
    target/x86_64-apple-darwin/release/autosort
else
  cargo build --release
  BIN="target/release/autosort"
fi

APP="build/AutoSort.app"
rm -rf "$APP"
mkdir -p "$APP/Contents/MacOS" "$APP/Contents/Resources"
cp "$BIN" "$APP/Contents/MacOS/autosort"
cp resources/Info.plist "$APP/Contents/Info.plist"
[[ -f resources/AutoSort.icns ]] && cp resources/AutoSort.icns "$APP/Contents/Resources/AutoSort.icns"

# Ad-hoc signature so it launches locally. For distribution, sign with a Developer ID and notarise.
codesign --force --deep --sign - "$APP"
echo "Built $APP — drag it to /Applications."
