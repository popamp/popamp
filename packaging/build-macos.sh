#!/bin/sh
# Builds Popamp's macOS download from an already-compiled release popamp:
# Popamp.app in a disk image, Popamp-Plus-… for one built with --features
# plus (its /VERSION says which). The app carries libopus, libopenmpt and
# the MIDI SoundFont, and runs on macOS 12 and newer.
#
# Signing: with MACOS_SIGN_IDENTITY (a "Developer ID Application: …"
# certificate in the keychain) the app is signed with the hardened runtime;
# with NOTARY_KEY_PATH, NOTARY_KEY_ID and NOTARY_ISSUER (an App Store
# Connect API key) the disk image is also notarised and stapled. Without
# them it is signed ad hoc, and Gatekeeper asks before the first start.
#
# Usage: packaging/build-macos.sh [target/release/popamp] [output-dir] [libs-dir]
# libs-dir (default dist/libs) holds what packaging/macos/fetch-libs.sh
# made. Build popamp with MACOSX_DEPLOYMENT_TARGET=12.0. ARCH is this
# Mac's (arm64 or x86_64) by default, or universal for a popamp and
# libraries that hold both (lipo -create), packaged as
# …-macos-universal.dmg.
# Needs: the Xcode Command Line Tools.
set -eu

here=$(cd "$(dirname "$0")" && pwd)
root=$(cd "$here/.." && pwd)
bin=${1:-"$root/target/release/popamp"}
outdir=${2:-"$root/dist"}
libs=${3:-"$root/dist/libs"}
arch=${ARCH:-$(uname -m)}
target=12.0
case "$arch" in
    arm64 | x86_64) archs=$arch ;;
    universal) archs="arm64 x86_64" ;;
    *) echo "error: ARCH is $arch, not arm64, x86_64 or universal" >&2; exit 1 ;;
esac

if [ ! -x "$bin" ]; then
    echo "error: binary not found: $bin" >&2
    echo "hint: run 'MACOSX_DEPLOYMENT_TARGET=$target cargo build --release -p popamp' first" >&2
    exit 1
fi
[ -f "$libs/libopus.0.dylib" ] && [ -f "$libs/libopenmpt.0.dylib" ] && [ -f "$libs/GeneralUser-GS.sf2" ] || {
    echo "error: $libs lacks the libraries or the SoundFont" >&2
    echo "hint: run 'sh packaging/macos/fetch-libs.sh $libs' first" >&2
    exit 1
}

version=$(grep -m1 '^version' "$root/Cargo.toml" | sed 's/.*"\(.*\)".*/\1/')
# "popamp 0.8.0" or "popamp-plus 0.8.0".
set -- $("$bin" /VERSION)
if [ "${2:-}" != "$version" ]; then
    echo "error: $bin is version ${2:-?}, Cargo.toml says $version" >&2
    exit 1
fi
case "$1" in
    popamp) name=Popamp; title=Popamp; plus=false ;;
    popamp-plus) name=Popamp-Plus; title="Popamp Plus"; plus=true ;;
    *) echo "error: $bin says it is $1, not popamp or popamp-plus" >&2; exit 1 ;;
esac

mkdir -p "$outdir"
outdir=$(cd "$outdir" && pwd)
work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT INT TERM

# The bundle.
app="$work/stage/Popamp.app"
mkdir -p "$app/Contents/MacOS" "$app/Contents/Frameworks" "$app/Contents/Resources/licenses"
cp "$bin" "$app/Contents/MacOS/popamp"
cp "$libs/libopus.0.dylib" "$libs/libopenmpt.0.dylib" "$app/Contents/Frameworks/"
cp "$libs/GeneralUser-GS.sf2" "$root/assets/logo/popamp.icns" "$app/Contents/Resources/"
cp -R "$libs/licenses/." "$app/Contents/Resources/licenses/"
cp "$root/LICENSE-MIT" "$root/LICENSE-APACHE" "$root/THIRD_PARTY.md" "$app/Contents/Resources/"
sed "s/@VERSION@/$version/g" "$here/macos/Info.plist" > "$app/Contents/Info.plist"
plutil -lint "$app/Contents/Info.plist" >/dev/null

# Every binary inside must hold each architecture, and each of those must
# start on macOS 12 and need nothing outside the system and the bundle.
for f in "$app/Contents/MacOS/popamp" "$app/Contents/Frameworks/"*.dylib; do
    has=$(lipo -archs "$f" | tr ' ' '\n' | sort | xargs)
    [ "$has" = "$archs" ] || { echo "error: $(basename "$f") is $has, not $archs" >&2; exit 1; }
    for a in $archs; do
        minos=$(otool -arch "$a" -l "$f" | awk '/LC_BUILD_VERSION/ { found = 1 } found && /minos/ { print $2; exit }')
        [ "$minos" = "$target" ] || { echo "error: $(basename "$f") ($a) needs macOS $minos, not $target" >&2; exit 1; }
        outside=$(otool -arch "$a" -L "$f" | tail -n +2 | awk '{ print $1 }' | grep -v -e '^/usr/lib/' -e '^/System/' -e '^@executable_path/\.\./Frameworks/' || true)
        [ -z "$outside" ] || { echo "error: $(basename "$f") ($a) links $outside" >&2; exit 1; }
        # A slice can be there and still be empty: each must hold what
        # Popamp looks up.
        case "$(basename "$f")" in
            libopus.0.dylib) symbol=_opus_multistream_decoder_create ;;
            libopenmpt.0.dylib) symbol=_openmpt_module_create_from_memory2 ;;
            *) symbol= ;;
        esac
        if [ -n "$symbol" ] && ! nm -arch "$a" -gU "$f" 2>/dev/null | grep -q " $symbol\$"; then
            echo "error: $(basename "$f") ($a) lacks $symbol" >&2
            exit 1
        fi
    done
done
"$app/Contents/MacOS/popamp" /VERSION >/dev/null

# Signing: the libraries first, then the app around them.
if [ -n "${MACOS_SIGN_IDENTITY:-}" ]; then
    sign() { codesign --force --timestamp --options runtime --sign "$MACOS_SIGN_IDENTITY" "$@"; }
    for f in "$app/Contents/Frameworks/"*.dylib; do sign "$f"; done
    # Popamp Plus runs plug-ins through wasmtime, which writes machine code.
    if [ "$plus" = true ]; then
        sign --entitlements "$here/macos/plus.entitlements" "$app"
        codesign -d --entitlements - "$app" 2>/dev/null | grep -q allow-unsigned-executable-memory ||
            { echo "error: Popamp Plus lacks allow-unsigned-executable-memory" >&2; exit 1; }
    else
        sign "$app"
    fi
    codesign --verify --deep --strict "$app"
else
    codesign --force --deep --sign - "$app"
    echo "note: signed ad hoc; Gatekeeper will ask before the first start (see docs/building.md)"
fi

# The disk image: the app beside a way to Applications.
ln -s /Applications "$work/stage/Applications"
dmg="$outdir/${name}-${version}-macos-${arch}.dmg"
rm -f "$dmg"
hdiutil create -quiet -volname "$title $version" -srcfolder "$work/stage" -fs HFS+ -format UDZO "$dmg"

if [ -n "${MACOS_SIGN_IDENTITY:-}" ] && [ -n "${NOTARY_KEY_PATH:-}" ] && [ -n "${NOTARY_KEY_ID:-}" ] && [ -n "${NOTARY_ISSUER:-}" ]; then
    xcrun notarytool submit "$dmg" --key "$NOTARY_KEY_PATH" --key-id "$NOTARY_KEY_ID" --issuer "$NOTARY_ISSUER" --wait
    xcrun stapler staple "$dmg"
else
    echo "note: not notarised"
fi
echo "$dmg"
