<h1 align="center">Popamp</h1>

<p align="center">
  <b>English</b> · <a href="README.zh-CN.md">简体中文</a> · <a href="README.ko.md">한국어</a>
</p>

<p align="center">
  <a href="#install"><img src="assets/readme/hero.png" alt="Popamp with three of its skins: Default, Steel and Modern" width="100%"></a>
</p>

<p align="center">
  <a href="https://github.com/popamp/popamp/actions/workflows/ci.yml"><img src="https://github.com/popamp/popamp/actions/workflows/ci.yml/badge.svg" alt="CI"></a>
  <img src="https://img.shields.io/badge/platform-Linux%20%7C%20Windows%20%7C%20macOS-8a5cf6" alt="Linux, Windows and macOS">
  <a href="#license"><img src="https://img.shields.io/badge/license-MIT%20OR%20Apache--2.0-blue.svg" alt="MIT OR Apache-2.0"></a>
</p>

<p align="center">
  <b><a href="#install">Download</a></b> ·
  <a href="docs/features.md">Features</a> ·
  <a href="docs/usage.md">Usage</a> ·
  <a href="docs/skin-format.md">Make a skin</a> ·
  <a href="docs/ROADMAP.md">Roadmap</a> ·
  <a href="#support">Support</a>
</p>

Popamp is a desktop music player in the spirit of Winamp 2: a main
window, a 10-band equalizer and a playlist that snap together, and a
different look whenever you want one. It is written in Rust and runs on
Linux and Windows, with an experimental macOS version.

- **Sharp at any size.** Popamp's own skins are drawn as vectors, so they
  stay crisp from ×1 to ×3 in steps of a quarter, on any monitor.
- **Thousands of classic skins.** Open any `.wsz` file, or browse, search
  and install skins from the [Winamp Skin Museum](https://skins.webamp.org)
  without leaving the player.
- **A gallery in the player.** Install and update the skins and plug-ins
  from [popamp.app](https://popamp.app) in a click, each checked against
  the catalog.
- **Sounds right.** Gapless playback, ReplayGain, a 10-band equalizer
  with `.eqf` presets, internet radio, and the common formats: MP3, AAC, FLAC,
  Ogg Vorbis, Opus, WAV, ALAC, MIDI, tracker modules and more.
- **At home on your desktop.** Media keys and the system's player
  controls show the track and its cover.
- **A player that is a bar.** In [toolbar mode](#toolbar-mode) the main
  window becomes a strip along the top or bottom of the screen, on Linux
  and Windows, with the room for it kept like a taskbar's.
- **Plug-ins you allow.** Sandboxed WebAssembly plug-ins, each switched
  on by you: [YouTube](https://github.com/popamp/plugin-youtube) as a
  source, [Crossfeed](https://github.com/popamp/plugin-crossfeed) for
  headphones, [Monkey's Audio](https://github.com/popamp/plugin-ape) and
  [WavPack](https://github.com/popamp/plugin-wavpack) for `.ape` and
  `.wv` files.
- **Your music stays yours.** No account, no cloud, no library to import:
  it plays files and the streams you open.

### Skins

| [Default](https://github.com/popamp/popamp-skin-default) (built in) | [Steel](https://github.com/popamp/skin-steel) | [Modern](https://github.com/popamp/skin-modern) |
|:---:|:---:|:---:|
| <img src="assets/screenshots/default.png" width="250" alt="Popamp's default skin"> | <img src="assets/screenshots/steel.png" width="250" alt="The Steel skin"> | <img src="assets/screenshots/modern.png" width="250" alt="The Modern skin"> |

### Toolbar mode

The whole player in one strip along the bottom or top of the screen:
transport, visualizer, clock, title, volume and the EQ and playlist
switches, with the equalizer and the playlist above its right end. On
Linux (X11) the strip is reserved with `_NET_WM_STRUT_PARTIAL`, so
maximized windows stop at it as they do at a taskbar, and it keeps clear
of the desktop's own panels; on Windows it is an application bar. Turn it
on, move it and size it under Main menu > *Toolbar mode*. The built-in
skin and the Modern, Steel, Classified and D11 skins each carry a bar
beside their window, in all their colour sets. More in
[features](docs/features.md#toolbar-mode).

<img src="assets/screenshots/bar.png" alt="Toolbar mode with the Modern skin: a strip along the bottom of the screen with the equalizer and playlist above its right end">

## Why?

Popamp began with a search for Winamp on a Linux desktop. Two good
players written in Rust already existed,
[Linamp](https://github.com/sayoojkkarun/linamp) and
[Ferramp](https://github.com/SpankulatorX/ferramp), but both drew the
classic bitmap skins, and on a modern monitor with the desktop scaled up
those skins went soft and blurry, even at double size.

So Popamp took Linamp's code, brought over the best of Ferramp's
features, and added one new idea: skins drawn as vectors, sharp at any
size, fractional ones included. Classic skins got the same care. Then
came plug-ins that cannot reach more than you allow, and a Windows
version, so one player and its skins work on both systems.

## Install

Downloads are on the [Releases page](https://github.com/popamp/popamp/releases),
and Debian, Ubuntu, openSUSE and Fedora can install it from a repository
that keeps it up to date. Both come in two builds: **popamp**, the player, and **popamp-plus**, the player
with [plug-ins](docs/plugins.md) such as YouTube and visualizers.

| System | Download |
|---|---|
| Debian, Ubuntu | add the [apt repository](docs/building.md#debian-and-ubuntu), then `sudo apt install popamp`; or download `popamp-<version>-linux-amd64.deb` (ARM64: `-arm64.deb`) and `sudo apt install ./popamp-…deb` |
| openSUSE, Fedora | add the [repository](docs/building.md#opensuse-and-fedora), then `sudo zypper install popamp` or `sudo dnf install popamp` |
| Arch, Manjaro, EndeavourOS | `popamp-bin.PKGBUILD` from the release: save it as `PKGBUILD` in an empty folder and run `makepkg -si` ([how](docs/building.md#arch-manjaro-endeavouros)) |
| Any other Linux | `Popamp-<version>-x86_64.AppImage`: `chmod +x` it and run it |
| Windows 10 or 11 | `Popamp-<version>-windows-x86_64-setup.exe`, or the portable `…-portable.zip`: unpack it anywhere and run `popamp.exe` |
| macOS 12 or newer (experimental) | `Popamp-<version>-macos-universal.dmg`, for Apple Silicon and Intel: open it and drag Popamp to Applications; the first start needs [a click in System Settings](docs/building.md#macos) |

Or build it yourself with Rust 1.96 or newer:

```sh
git clone https://github.com/popamp/popamp.git && cd popamp
cargo build --release                      # add --features plus for popamp-plus
./target/release/popamp ~/Music
```

### More

- [Features](docs/features.md): everything Popamp does, and what it does not do yet.
- [Usage](docs/usage.md): keys, remote control from the command line, where settings live.
- [Building and installing](docs/building.md): requirements, packages per distribution, installing from source.
- [Plug-ins](docs/plugins.md) and [the skin format](docs/skin-format.md), for making your own.
- [Testing](docs/testing.md): what was tested, where and how.
- [Contributing](CONTRIBUTING.md), [Changelog](CHANGELOG.md), [Roadmap](docs/ROADMAP.md).

### Limits and desktop-specific behaviour

Some desktops limit what a program may do with its windows, or behave in
ways you might not expect. What that means for Popamp, and what to do
about it:

- [COSMIC (Pop!_OS) and Wayland](docs/wayland.md): what works as a Wayland program, and why some things cannot.
- [Ubuntu](docs/ubuntu.md): toolbar mode beside Ubuntu's dock, and a bar smaller than the size you chose.
- [KDE Plasma](docs/kde.md): toolbar mode at the top covers the desktop icons.

## Support

Popamp is free, with no ads, no tracking and no paid features. If you
like it, you can [buy the project a coffee on Ko-fi](https://ko-fi.com/popamp):
it pays for hosting, test hardware and signed builds.

## License

Popamp is free software under the [MIT](LICENSE-MIT) or
[Apache 2.0](LICENSE-APACHE) license, at your option; contributions are
taken under the same terms. Third-party crates keep their own licenses
(the Symphonia decoders are MPL-2.0), and code taken from Linamp and
Ferramp is listed in [THIRD_PARTY.md](THIRD_PARTY.md).

Popamp contains no Winamp code or artwork. Winamp is a trademark of its
owner; Popamp is an independent project and is not affiliated with or
endorsed by it.
