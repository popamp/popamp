# Building and installing

## Requirements

The tables below are for Linux; Windows is [further down](#windows).

### Build

| Need | Version | Why |
|---|---|---|
| Rust toolchain (`rustc`, `cargo`) | 1.96 or newer | compiler (install with [rustup](https://rustup.rs)) |
| C compiler (`gcc` or `clang`) | any | the `ring` crate (TLS for HTTPS streams) compiles C code |
| Internet access | — | `cargo` downloads crates on the first build |

No `-devel`/`-dev` packages and no `pkg-config` are needed. The system
libraries below are loaded at runtime, not linked at build time.

### Runtime

| Library / tool | Required? | Used for |
|---|---|---|
| X11 server, XWayland or a Wayland compositor | **required** | windows (X11 where it runs, also on Wayland desktops; Wayland itself where it does not, or with `wayland=1` in `popamp.ini`) |
| `libasound.so.2` (ALSA) | **required** | audio output; on PipeWire/PulseAudio it goes through their ALSA `default` device |
| `libX11`, `libX11-xcb`, `libxcb`, `libXcursor`, `libXi`, `libxkbcommon`, `libxkbcommon-x11` | **required** | windowing and keyboard (loaded with `dlopen`) |
| `libwayland-client` | for Wayland | windows as a Wayland program (loaded with `dlopen`) |
| `fontconfig` (`fc-match`) and any TrueType font | recommended | playlist and menu text (blank without them) |
| `zenity` or `kdialog` | recommended | open-file, open-URL, save and preset dialogs |
| D-Bus session bus | optional | MPRIS (media keys, desktop widgets) |

### Packages per distribution

Fedora was used for testing. The Debian/Ubuntu and Arch names are the usual
package names but **have not been tested**.

**Fedora**

```sh
sudo dnf install gcc alsa-lib libX11 libX11-xcb libxcb libXcursor libXi \
    libxkbcommon libxkbcommon-x11 fontconfig zenity
```

**Debian / Ubuntu** (untested)

```sh
sudo apt install build-essential libasound2t64 libx11-6 libx11-xcb1 libxcb1 \
    libxcursor1 libxi6 libxkbcommon0 libxkbcommon-x11-0 fontconfig zenity
```

On Debian 12 and Ubuntu before 24.04 the ALSA package is `libasound2`.

**Arch Linux** (untested)

```sh
sudo pacman -S --needed base-devel alsa-lib libx11 libxcb libxcursor libxi \
    libxkbcommon libxkbcommon-x11 fontconfig zenity
```

### Windows

Windows 10 or 11, x86_64. The release downloads need nothing else: sound goes
through WASAPI, fonts and dialogs are the system's. Building needs Rust
1.96 or newer with its default MSVC toolchain (rustup offers to install
the Visual Studio Build Tools it needs). So far Popamp runs on Windows in
CI and under Proton; reports from real machines are welcome.

## Installation

### Packages

Releases on the [Releases page](https://github.com/popamp/popamp/releases)
come with Linux packages for x86_64 and ARM64 (built on Ubuntu 22.04, so
they need glibc 2.35 or newer) and a Windows installer and portable zip;
Debian and Ubuntu can take the debs from an [apt repository](#debian-and-ubuntu),
openSUSE and Fedora get rpms from a [repository](#opensuse-and-fedora).
All come in two builds:

- **popamp:** the player.
- **popamp-plus:** the player with everything: [plug-ins](plugins.md),
  such as YouTube, and visualizers. It is larger, since it carries a
  WebAssembly runtime and GPU code.

Both install `popamp` and share its settings; installing one replaces
the other.

| System | Package |
|---|---|
| Debian / Ubuntu | `sudo apt install ./popamp-<version>-linux-amd64.deb`, or `./popamp-plus-…` (ARM64: `-arm64.deb`) |
| Anything else | `Popamp-<version>-linux-x86_64.AppImage`, or `Popamp-Plus-…`: `chmod +x` it and run it |
| Windows | `Popamp-<version>-windows-x86_64-setup.exe`, or `Popamp-Plus-…`: run it |
| Windows, portable | `Popamp-<version>-windows-x86_64-portable.zip`, or `Popamp-Plus-…`: unpack it anywhere and run `popamp.exe` |

The installer asks whether to install for you alone or for all users,
adds Popamp to the Start menu, opens `.wsz` and `.psz` skins with it and
puts it in "Open with" for audio files and playlists, and in Settings >
Default apps, where you can make it the player for them (Preferences has
a button that opens that page). Settings and skins
live in `%APPDATA%\popamp` and stay there when Popamp is uninstalled.

On Linux, "Open with Popamp" in Preferences makes it the default player
for audio files, playlists or skins, the way `xdg-mime default` does. An
AppImage or a build of your own gets a desktop entry in
`~/.local/share/applications` for that.

The portable zip has an empty `popamp.ini` next to `popamp.exe`, which
keeps the settings and skins in that folder instead, for a copy on a USB
stick; unpack it where you can write.

`popamp /VERSION` says which one you have.

#### Debian and Ubuntu

Each release puts its debs, for amd64 and arm64, in the apt repository
at apt.popamp.app (built by [popamp/apt](https://github.com/popamp/apt)).
Add it once, and Popamp updates with `apt upgrade`:

```sh
sudo mkdir -p /etc/apt/keyrings
sudo curl -fsSLo /etc/apt/keyrings/popamp.gpg https://apt.popamp.app/popamp.gpg
sudo curl -fsSLo /etc/apt/sources.list.d/popamp.sources https://apt.popamp.app/popamp.sources
sudo apt update
sudo apt install popamp           # or popamp-plus
```

It works on Debian 12 and Ubuntu 22.04 or newer. The repository keeps
only the latest release.

#### openSUSE and Fedora

The [openSUSE Build Service](https://download.opensuse.org/repositories/home:/GoodZon:/popamp/) builds rpms of each release for
openSUSE Tumbleweed and Leap 16.0 and Fedora 43 and 44, x86_64 and
ARM64. Add the repository once, and Popamp updates with the rest of the
system:

```sh
# openSUSE Tumbleweed (Leap 16.0: openSUSE_Leap_16.0 instead of openSUSE_Tumbleweed)
sudo zypper addrepo https://download.opensuse.org/repositories/home:/GoodZon:/popamp/openSUSE_Tumbleweed/home:GoodZon:popamp.repo
sudo zypper install popamp        # or popamp-plus

# Fedora 44 (or Fedora_43)
sudo dnf config-manager addrepo --from-repofile=https://download.opensuse.org/repositories/home:/GoodZon:/popamp/Fedora_44/home:GoodZon:popamp.repo
sudo dnf install popamp           # or popamp-plus
```

The first install asks you to trust the repository's signing key.

The skins and plug-ins with a release are in both repositories too, as
`popamp-skin-<name>` and `popamp-plugin-<id>` (e.g. `popamp-skin-steel`,
`popamp-plugin-youtube`); a plug-in needs popamp-plus.

The AppImage uses the system's ALSA, X11 and xkbcommon libraries, which
nearly every desktop has. To build the packages yourself after
`cargo build --release` (or with `--features plus` for popamp-plus;
the scripts ask the binary which it is):

```sh
packaging/build-deb.sh        # needs dpkg-deb and binutils
packaging/build-appimage.sh   # needs appimagetool
```

Both write to `dist/`. Or build from source:

### 1. Get Rust (if you don't have it)

```sh
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
source "$HOME/.cargo/env"
```

### 2. Build and run from the source tree

```sh
git clone https://github.com/popamp/popamp.git && cd popamp
cargo build --release
./target/release/popamp ~/Music
```

The first build downloads and compiles all crates (a few minutes). The
binary is `target/release/popamp`.

This builds the player and its skins. For plug-ins (see
[plugins.md](plugins.md)), build with the `plugins` feature;
it adds a WebAssembly runtime, so the build takes longer and the binary
is larger:

```sh
cargo build --release --features plugins
```

Visualizer plug-ins draw with their own shaders on the GPU, through wgpu
(Vulkan on Linux, Direct3D 12 on Windows). They are a separate choice and
turn plug-ins on as well; without this feature the program has no GPU code:

```sh
cargo build --release --features visualizers
```

`--features plus` is everything, and is what popamp-plus is built with.
Only such a build calls itself Popamp Plus.

### 3. Install for the current user (optional)

```sh
packaging/install.sh
```

This builds a release binary and installs:

| File | Destination |
|---|---|
| `popamp` binary | `~/.local/bin/popamp` |
| Desktop entry | `~/.local/share/applications/popamp.desktop` |
| `.wsz`/`.psz` MIME types | `~/.local/share/mime/packages/popamp-skin.xml` |
| Icons (SVG + 16–256 px PNG) | `~/.local/share/icons/hicolor/*/apps/popamp.*` |
| Skins folder | `~/.local/share/popamp/skins/` (created empty) |

Make sure `~/.local/bin` is on your `PATH`. For a system-wide install, run
`sudo PREFIX=/usr/local packaging/install.sh`.

### Uninstall

```sh
rm -f ~/.local/bin/popamp ~/.local/share/applications/popamp.desktop \
      ~/.local/share/mime/packages/popamp-skin.xml \
      ~/.local/share/metainfo/popamp.metainfo.xml
rm -f ~/.local/share/icons/hicolor/*/apps/popamp.png \
      ~/.local/share/icons/hicolor/scalable/apps/popamp.svg
update-mime-database ~/.local/share/mime
# Settings and skins, if you want them gone too:
rm -rf ~/.config/popamp ~/.local/share/popamp
```

