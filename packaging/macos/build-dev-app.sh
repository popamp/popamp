#!/bin/sh
# Wraps a built popamp in Popamp.app, so macOS offers it in Finder's
# "Open With" and hands it the files opened there. For development: the
# release bundle (icon, libraries, disk image) is packaging/build-macos.sh.
#
#   packaging/macos/build-dev-app.sh [binary] [out]
#
# Defaults: target/release/popamp and target/Popamp.app.
set -eu

here=$(cd "$(dirname "$0")" && pwd)
binary=${1:-target/release/popamp}
out=${2:-target/Popamp.app}

[ -x "$binary" ] || { echo "no popamp at $binary: run cargo build --release first" >&2; exit 1; }
version=$("$binary" /VERSION | grep -oE '[0-9]+\.[0-9]+\.[0-9]+' | head -n 1)

rm -rf "$out"
mkdir -p "$out/Contents/MacOS"
cp "$binary" "$out/Contents/MacOS/popamp"
sed "s/@VERSION@/$version/g" "$here/Info.plist" > "$out/Contents/Info.plist"
plutil -lint "$out/Contents/Info.plist" >/dev/null
# Apple Silicon runs only signed code; ad hoc is enough on this Mac.
codesign --force --sign - "$out" 2>/dev/null
echo "$out ($version)"
