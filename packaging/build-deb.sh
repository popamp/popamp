#!/bin/sh
# Builds a .deb package from an already-compiled release binary: popamp,
# or popamp-plus for a binary built with --features plus (the binary
# says which in its /VERSION). Each replaces the other.
# Usage: packaging/build-deb.sh [target/release/popamp] [output-dir]
# Needs: dpkg-deb, objdump and strip (binutils). Version is read from Cargo.toml.
# Adapted from Ferramp's build-deb.sh (see THIRD_PARTY.md).
set -eu

here=$(cd "$(dirname "$0")" && pwd)
root=$(cd "$here/.." && pwd)
bin=${1:-"$root/target/release/popamp"}
outdir=${2:-"$root/dist"}

if [ ! -x "$bin" ]; then
    echo "error: binary not found or not executable: $bin" >&2
    echo "hint: run 'cargo build --release -p popamp' first" >&2
    exit 1
fi

version=$(grep -m1 '^version' "$root/Cargo.toml" | sed 's/.*"\(.*\)".*/\1/')
# "popamp 0.5.0" or "popamp-plus 0.5.0".
set -- $("$bin" /VERSION)
package=$1
if [ "${2:-}" != "$version" ]; then
    echo "error: $bin is version ${2:-?}, Cargo.toml says $version" >&2
    exit 1
fi
case "$package" in
    popamp) other=""; extra=""; vulkan=""; summary="Winamp-style skinnable audio player" ;;
    popamp-plus)
        summary="Winamp-style skinnable audio player, with plug-ins"
        # Only the visualizers need it, and they say so if it is missing.
        vulkan=", libvulkan1"
        # Versioned, so a skin's "Depends: popamp (>= x)" takes popamp-plus too.
        other="Provides: popamp (= $version)
Conflicts: popamp
Replaces: popamp
"
        extra=" .
 This is Popamp Plus, built with plug-ins: WebAssembly modules, installed
 separately and allowed one by one, that add sources such as YouTube and
 sound effects such as crossfeed, and visualizers that draw with their
 own shaders on the graphics card."
        ;;
    *) echo "error: $bin says it is $package, not popamp or popamp-plus" >&2; exit 1 ;;
esac
case "$(uname -m)" in
    x86_64) debarch=amd64 ;;
    aarch64|arm64) debarch=arm64 ;;
    *) echo "error: unsupported architecture: $(uname -m)" >&2; exit 1 ;;
esac
# The newest glibc symbol the binary uses: depends on the build machine.
glibc=$(objdump -T "$bin" | grep -o 'GLIBC_[0-9.]*' | sed 's/GLIBC_//' | sort -uV | tail -1)

stagedir=$(mktemp -d)
trap 'rm -rf "$stagedir"' EXIT INT TERM
pkg="$stagedir/$package"
doc="$pkg/usr/share/doc/$package"

sh "$here/install-files.sh" "$bin" "$pkg/usr"
strip "$pkg/usr/bin/popamp"
for f in README.md LICENSE-MIT LICENSE-APACHE THIRD_PARTY.md; do
    install -Dm644 "$root/$f" "$doc/$f"
done
cat > "$doc/copyright" <<EOF
Format: https://www.debian.org/doc/packaging-manuals/copyright-format/1.0/
Source: https://github.com/popamp/popamp

Files: *
Copyright: Popamp contributors
License: MIT or Apache-2.0
 See LICENSE-MIT and LICENSE-APACHE in this directory, and THIRD_PARTY.md
 for third-party credits. The Apache licence is also in
 /usr/share/common-licenses/Apache-2.0.
EOF

# Shared libraries are loaded at run time (dlopen), so dpkg-shlibdeps cannot
# see them: ALSA for sound, X11 and xkbcommon for windows and keys.
mkdir -p "$pkg/DEBIAN"
cat > "$pkg/DEBIAN/control" <<EOF
Package: $package
Version: $version
Section: sound
Priority: optional
Architecture: $debarch
Maintainer: Gunyakov <web@gunyakov.pro>
Installed-Size: $(du -sk "$pkg/usr" | cut -f1)
Homepage: https://github.com/popamp/popamp
Depends: libc6 (>= $glibc), libasound2t64 | libasound2, libx11-6, libx11-xcb1, libxcursor1, libxi6, libxkbcommon0, libxkbcommon-x11-0, libopus0, fontconfig
Recommends: zenity | kdialog, fonts-dejavu-core, libwayland-client0, libopenmpt0t64 | libopenmpt0$vulkan
Suggests: fluid-soundfont-gm | timgm6mb-soundfont
${other}Description: $summary
 Popamp is an audio player in the style of the classic Winamp 2: main
 window, equalizer and playlist editor that dock together, with support
 for classic .wsz skins. It plays MP3, Ogg Vorbis, FLAC, WAV, AIFF,
 AAC/M4A and internet radio streams.
EOF
[ -z "$extra" ] || printf '%s\n' "$extra" >> "$pkg/DEBIAN/control"

mkdir -p "$outdir"
deb="$outdir/${package}-${version}-linux-${debarch}.deb"
dpkg-deb -Zgzip --root-owner-group --build "$pkg" "$deb" >/dev/null
echo "Built $deb"
