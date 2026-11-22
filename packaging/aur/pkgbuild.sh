#!/bin/sh
# Writes the AUR PKGBUILD of popamp-bin or popamp-plus-bin for a release:
# the package is made from the release's .deb (x86_64 and aarch64), with
# its sums taken from the release's SHA256SUMS.txt. Running it again for
# the same release writes the same file.
# Usage: packaging/aur/pkgbuild.sh <popamp|popamp-plus> <version> <SHA256SUMS.txt>
#   e.g. gh release download v1.1.0 -R popamp/popamp -p SHA256SUMS.txt
#        packaging/aur/pkgbuild.sh popamp-plus 1.1.0 SHA256SUMS.txt > PKGBUILD
# AUR_MAINTAINER (e.g. "Name <mail at example dot com>") fills the Maintainer line.
set -eu

die() { echo "error: $*" >&2; exit 1; }

[ $# -eq 3 ] || die "usage: $0 <popamp|popamp-plus> <version> <SHA256SUMS.txt>"
deb=$1
version=$2
sums=$3
case $deb in
    popamp) desc='Winamp-style skinnable music player' extra='' ;;
    popamp-plus) desc='Winamp-style skinnable music player, with plug-ins'
                 extra="
            'vulkan-icd-loader: visualizer plug-ins'" ;;
    *) die "$deb: not popamp or popamp-plus" ;;
esac
echo "$version" | grep -Eq '^[0-9]+\.[0-9]+\.[0-9]+$' || die "bad version: $version"

sum() {
    s=$(awk -v f="$deb-$version-linux-$1.deb" '$2 == f || $2 == "*" f { print $1 }' "$sums")
    [ -n "$s" ] || die "$sums has no $deb-$version-linux-$1.deb"
    echo "$s"
}
amd64=$(sum amd64)
arm64=$(sum arm64)

cat <<PKGBUILD
# Maintainer: ${AUR_MAINTAINER:-Popamp}
# Written by packaging/aur/pkgbuild.sh in https://github.com/popamp/popamp;
# change the script, not this file.
pkgname=$deb-bin
_deb=$deb
pkgver=$version
pkgrel=1
pkgdesc='$desc'
arch=(x86_64 aarch64)
url='https://popamp.app'
license=('MIT OR Apache-2.0')
# The binary links only glibc and libgcc; it loads the rest when it starts.
depends=(glibc gcc-libs alsa-lib libx11 libxcursor libxi libxkbcommon libxkbcommon-x11 opus fontconfig
         hicolor-icon-theme)
optdepends=('zenity: open and save dialogs'
            'kdialog: open and save dialogs on KDE'
            'ttf-dejavu: fallback font'
            'wayland: running as a Wayland program'
            'libopenmpt: tracker modules'
            'soundfont-fluid: MIDI playback'$extra)
# A finished binary: makepkg should neither strip it nor split off a debug package.
options=(!strip !debug)
provides=(popamp)
conflicts=(popamp)
source_x86_64=("https://github.com/popamp/popamp/releases/download/v\$pkgver/\$_deb-\$pkgver-linux-amd64.deb")
source_aarch64=("https://github.com/popamp/popamp/releases/download/v\$pkgver/\$_deb-\$pkgver-linux-arm64.deb")
noextract=("\$_deb-\$pkgver-linux-amd64.deb" "\$_deb-\$pkgver-linux-arm64.deb")
sha256sums_x86_64=('$amd64')
sha256sums_aarch64=('$arm64')

package() {
  local deb="\$_deb-\$pkgver-linux-amd64.deb"
  [ "\$CARCH" = aarch64 ] && deb="\$_deb-\$pkgver-linux-arm64.deb"
  bsdtar -xOf "\$deb" 'data.tar.*' | bsdtar -xf - -C "\$pkgdir"
  install -Dm644 -t "\$pkgdir/usr/share/licenses/\$pkgname" \\
    "\$pkgdir/usr/share/doc/\$_deb/LICENSE-MIT" "\$pkgdir/usr/share/doc/\$_deb/LICENSE-APACHE"
}
PKGBUILD
