# Popamp for openSUSE and Fedora, built on the openSUSE Build Service.
# One spec, two packages: _multibuild's "plus" flavour is popamp-plus,
# built with every function; each replaces the other, as the debs do.
# The sources are two tarballs made by packaging/obs/publish.sh: the
# project at a tag, and its crates (cargo vendor), since OBS builds offline.

%{!?flavor:%define flavor @BUILD_FLAVOR@%{nil}}
%if "%{flavor}" == "plus"
%define pkgname popamp-plus
%define features --features plus
%else
%define pkgname popamp
%define features %{nil}
%endif

# Release builds carry no debug info, so there is nothing to split off.
%global debug_package %{nil}

Name:           %{pkgname}
# publish.sh sets it from Cargo.toml.
Version:        0.9.1
Release:        0
%if "%{flavor}" == "plus"
Summary:        Winamp-style skinnable audio player, with plug-ins
%else
Summary:        Winamp-style skinnable audio player
%endif
# Popamp itself; the crates linked in add MPL-2.0 (Symphonia) and
# permissive licences, listed in THIRD_PARTY.md and the crates.
License:        (MIT OR Apache-2.0) AND MPL-2.0
%if 0%{?suse_version}
Group:          Productivity/Multimedia/Sound/Players
%endif
URL:            https://github.com/popamp/popamp
Source0:        popamp-%{version}.tar.xz
Source1:        popamp-%{version}-vendor.tar.xz
ExclusiveArch:  x86_64 aarch64

BuildRequires:  cargo >= 1.96
BuildRequires:  rust >= 1.96
# zstd-sys and wasmtime's helpers compile a little C.
BuildRequires:  gcc
BuildRequires:  hicolor-icon-theme

# ALSA, X11, xkbcommon and Opus are loaded at run time (dlopen), so rpm cannot
# see them in the binary; fonts are found with fc-match.
Requires:       libasound.so.2()(64bit)
Requires:       libX11.so.6()(64bit)
Requires:       libX11-xcb.so.1()(64bit)
Requires:       libXcursor.so.1()(64bit)
Requires:       libXi.so.6()(64bit)
Requires:       libxkbcommon.so.0()(64bit)
Requires:       libxkbcommon-x11.so.0()(64bit)
Requires:       libopus.so.0()(64bit)
Requires:       fontconfig
Requires:       hicolor-icon-theme
Recommends:     (zenity or kdialog)
# Windows as a Wayland program, where there is no Xwayland.
Recommends:     libwayland-client.so.0()(64bit)
# Tracker modules; loaded at run time, and they say so if it is missing.
Recommends:     libopenmpt.so.0()(64bit)
# A General MIDI SoundFont, for MIDI files.
%if 0%{?suse_version}
Suggests:       fluid-soundfont
%else
Suggests:       fluid-soundfont-gm
%endif
%if 0%{?suse_version}
Recommends:     dejavu-fonts
%else
Recommends:     dejavu-sans-fonts
%endif

%if "%{flavor}" == "plus"
# Only the visualizers need it, and they say so if it is missing.
Recommends:     libvulkan.so.1()(64bit)
Provides:       popamp = %{version}-%{release}
Conflicts:      popamp
%else
Conflicts:      popamp-plus
%endif

%description
Popamp is an audio player in the style of the classic Winamp 2: main
window, equalizer and playlist editor that dock together, with support
for classic .wsz skins and its own vector skins, sharp at any size. It
plays MP3, Ogg Vorbis, FLAC, WAV, AIFF, AAC/M4A, ALAC and internet radio
streams.
%if "%{flavor}" == "plus"

This is Popamp Plus, built with plug-ins: WebAssembly modules, installed
separately and allowed one by one, that add sources such as YouTube and
sound effects such as crossfeed, and visualizers that draw with their
own shaders on the graphics card.
%endif

%prep
%setup -q -n popamp-%{version} -b 1
mkdir -p .cargo
cp cargo-config.toml .cargo/config.toml

%build
cargo build --release --offline --locked -p popamp %{features}

%install
sh packaging/install-files.sh target/release/popamp %{buildroot}%{_prefix}

%check
# The binary says which build it is.
test "$(target/release/popamp /VERSION)" = "%{name} %{version}"

%files
%license LICENSE-MIT LICENSE-APACHE
%doc README.md CHANGELOG.md THIRD_PARTY.md
%{_bindir}/popamp
%{_datadir}/applications/popamp.desktop
%{_datadir}/metainfo/popamp.metainfo.xml
%{_datadir}/mime/packages/popamp-skin.xml
%{_datadir}/icons/hicolor/*/apps/popamp.*

%changelog
