#!/bin/sh
# Puts what Popamp.app carries into <dir>, for packaging/build-macos.sh:
# libopus and libopenmpt built from their source releases for macOS 12 and
# newer (Homebrew's builds ask for the newest macOS, so they cannot be
# shipped), the GeneralUser GS SoundFont for MIDI, and their licenses under
# <dir>/licenses. Each library's install name points into the bundle's
# Frameworks folder.
# Usage: packaging/macos/fetch-libs.sh <dir>
# Needs: the Xcode Command Line Tools, curl and shasum. ARCH (by default this
# Mac's: arm64 or x86_64) picks the architecture, or universal for both,
# built one after the other and joined with lipo.
set -eu

dir=${1:?usage: fetch-libs.sh <dir>}
arch=${ARCH:-$(uname -m)}
target=12.0
case "$arch" in
    arm64 | x86_64) archs=$arch ;;
    universal) archs="arm64 x86_64" ;;
    *) echo "error: ARCH is $arch, not arm64, x86_64 or universal" >&2; exit 1 ;;
esac

# Pinned downloads; a new version means a new hash here.
opus_version=1.5.2
opus_sha256=65c1d2f78b9f2fb20082c38cbe47c951ad5839345876e46941612ee87f9a7ce1
openmpt_version=0.8.10
openmpt_sha256=976d259288c40cd285771f77d09eff3d02514511d5677ecfcfb53be5013317e2
soundfont_commit=684543d5e5efaef08d02be50dcda8d552478fa60
soundfont_sha256=9575028c7a1f589f5770fccc8cff2734566af40cd26ed836944e9a5152688cfe

mkdir -p "$dir/licenses"
dir=$(cd "$dir" && pwd)
tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT INT TERM

fetch() { # <url> <sha256> <file>
    curl -fsSL --retry 3 -o "$3" "$1"
    echo "$2  $3" | shasum -a 256 -c --quiet -
}

# Built for macOS 12, and blind to Homebrew's libraries, so nothing but
# the system's own is linked.
export MACOSX_DEPLOYMENT_TARGET=$target
# pkg-config sees only a description of the system's zlib.
mkdir -p "$tmp/pkgconfig"
printf 'Name: zlib\nDescription: the system zlib\nVersion: 1.2.12\nLibs: -lz\nCflags:\n' > "$tmp/pkgconfig/zlib.pc"
export PKG_CONFIG_LIBDIR="$tmp/pkgconfig"
jobs=$(sysctl -n hw.ncpu)

fetch "https://downloads.xiph.org/releases/opus/opus-$opus_version.tar.gz" "$opus_sha256" "$tmp/opus.tar.gz"
fetch "https://lib.openmpt.org/files/libopenmpt/src/libopenmpt-$openmpt_version+release.autotools.tar.gz" \
    "$openmpt_sha256" "$tmp/openmpt.tar.gz"

# Builds both libraries for <arch> into $tmp/<arch>.
build() { # <arch>
    a=$1
    src="$tmp/src-$a"
    mkdir -p "$src" "$tmp/$a"
    tar -xzf "$tmp/opus.tar.gz" -C "$src"
    tar -xzf "$tmp/openmpt.tar.gz" -C "$src"
    # The arch goes into the tools as well as the flags: libtool links C++
    # and joins objects (ld -r) without CFLAGS, which would build those
    # steps for this Mac and leave the other slice empty.
    export CC="clang -arch $a" CXX="clang++ -arch $a" LD="ld -arch $a"
    export CFLAGS="-arch $a -mmacosx-version-min=$target -O2"
    export CXXFLAGS="$CFLAGS"
    export LDFLAGS="-arch $a -mmacosx-version-min=$target"
    (
        cd "$src/opus-$opus_version"
        ./configure --host="$a-apple-darwin" --disable-static --disable-doc --disable-extra-programs >/dev/null
        make -j"$jobs" >/dev/null
    )
    cp "$src/opus-$opus_version/.libs/libopus.0.dylib" "$tmp/$a/"
    # Without its optional decoders (mpg123, Ogg Vorbis: only for MO3 files
    # with compressed samples) libopenmpt needs nothing beyond the system.
    (
        cd "$src/libopenmpt-$openmpt_version+release.autotools"
        ./configure --host="$a-apple-darwin" --disable-static \
            --disable-openmpt123 --disable-examples --disable-tests --disable-doxygen-doc \
            --without-mpg123 --without-ogg --without-vorbis --without-vorbisfile \
            --without-portaudio --without-portaudiocpp --without-sndfile --without-flac >/dev/null
        make -j"$jobs" >/dev/null
    )
    cp "$(find "$src/libopenmpt-$openmpt_version+release.autotools" -name libopenmpt.0.dylib -type f | head -n 1)" "$tmp/$a/"
}

for a in $archs; do build "$a"; done

# Puts the library <name> into $dir, every architecture in one file, named
# for the bundle.
for name in libopus.0.dylib libopenmpt.0.dylib; do
    set --
    for a in $archs; do set -- "$@" "$tmp/$a/$name"; done
    lipo -create "$@" -output "$dir/$name"
    install_name_tool -id "@executable_path/../Frameworks/$name" "$dir/$name"
done
cp "$tmp/src-${archs%% *}/opus-$opus_version/COPYING" "$dir/licenses/opus.txt"
mkdir -p "$dir/licenses/libopenmpt"
cp "$tmp/src-${archs%% *}/libopenmpt-$openmpt_version+release.autotools/LICENSE" "$dir/licenses/libopenmpt/LICENSE.txt"

raw="https://raw.githubusercontent.com/mrbumpy409/GeneralUser-GS/$soundfont_commit"
fetch "$raw/GeneralUser-GS.sf2" "$soundfont_sha256" "$dir/GeneralUser-GS.sf2"
curl -fsSL --retry 3 -o "$dir/licenses/GeneralUser-GS.txt" "$raw/documentation/LICENSE.txt"

ls "$dir"
