# Testing

## Tested platforms

| | Tested on |
|---|---|
| OS | Fedora Linux 44, KDE Plasma Desktop Edition, x86_64 |
| Kernel | 7.0.12-201.fc44 |
| Session | KDE Plasma on Wayland, Popamp running through Xwayland 24.1.12 |
| Audio | PipeWire 1.6.7 via alsa-lib 1.2.16.1 (`default` device) |
| Rust | rustc / cargo 1.96.0 (minimum supported version 1.96 checked with `cargo +1.96 check --locked`) |
| Dialogs | zenity 4.2.2 |

Windows: CI runs clippy and the tests on GitHub's Windows Server 2025
runners, and the release exe was checked by hand under Proton
Experimental (windows, sound, fonts, dialogs, remote commands, media
controls). Not yet checked on a real Windows machine: the bar's strip
shrinking the work area, the media keys, and the timeline, shuffle,
repeat and cover in the media overlay.

Ubuntu 26.04.1 (GNOME Shell 50.1, Xwayland 24.1.10) was checked by hand
from its live image in QEMU: docking, dragging, snapping and toolbar mode
(see [ubuntu.md](ubuntu.md)). Other systems built on GNOME were not.

Kubuntu 26.04.1 (KDE Plasma 6.6.6, KWin 6.6.6, Xwayland 24.1.10) was
checked the same way: docking, dragging, snapping, and toolbar mode at
the top and the bottom with maximized windows kept out of its strip
(see [kde.md](kde.md)).

Linux Mint 22.3 Cinnamon (Cinnamon 6.6.4, Muffin 6.6.1, Xorg 21.1.12), a
plain X11 session, was checked the same way from its live image. The bar's
strut works there as it is, at the top and the bottom, and the desktop
icons move out of the bar's way.

For 1.3.0 (2026-11-25) the windows docked against the right edge of the
screen were checked as they grow and shrink (Larger Size, Ctrl+D) on
Windows 11 (by hand), Ubuntu 26.04.1, Kubuntu 26.04.1, Linux Mint 22.3
Cinnamon and Linux Mint 22.3 Xfce (Xfwm4 4.18): the group stays on the
edge, the playlist is cut to the bottom, and everything is drawn again at
the new size. On Ubuntu the desktop's scale was changed while Popamp ran
(150% and 133% to 100%): the windows keep their size, but Mutter moves
them apart, so they need arranging again. Pop!_OS (COSMIC) was checked
on a real machine instead.
Arrange windows (Ctrl+Shift+Left/Right) was checked on the same four Linux
desktops at 1280x768 and 1280x1024: at the normal size the windows fill
the screen to its edges and panels. At 2x on a screen 1280 pixels wide the
library does not fit beside the main window even at its narrowest (800 +
550 pixels): GNOME, KDE and Cinnamon push it back over the main window's
side, Xfce leaves it 70 pixels past the edge. Popamp cannot bring the main
window over it: the docked windows are transient for it (owned on
Windows), and the window manager keeps them on top.

**Not tested:**
- Other distributions and desktops; Ubuntu, Kubuntu and Mint only from live images.
- Plain X11 sessions other than Cinnamon and Xfce.
- PulseAudio-only or pure-ALSA systems.
- Non-x86_64 CPUs.
- HiDPI scaling, and dialogs and menus at a display scaling above 100%
  on Linux.

## Test procedure

### Automated

```sh
cargo fmt --all -- --check
cargo clippy --workspace --all-targets    # expected: no warnings
cargo test --workspace
cargo test --workspace --features popamp/plugins   # adds the plug-in tests
cargo test --workspace --features popamp/plus      # adds the visualizer tests
```

The unit tests cover:
- Config parsing and round trips, and title formatting (ATF).
- Playlist navigation, shuffle (each track once per cycle) and editing.
- Playlist files: M3U/PLS/XSPF round trips, relative paths.
- EQF/Q1 presets: byte-exact round trip and dB scale.
- EQ: frequency response of each band ±1.5 dB, limiter ceiling.
- Skins: sprite rectangles inside their sheets, skin fallbacks, region
  rasterization, glyph mapping.
- ICY metadata parsing; a web page is not taken for a stream.
- Plug-ins: manifests, permissions and calls through a test plug-in.
- Window images: every `/RENDER` state of the built-in skin is compared
  pixel by pixel with the PNGs in `apps/popamp/tests/golden/`, on Linux
  and Windows. The states show no playlist text, so system fonts do not
  change them. After a change that is meant to show, rewrite the images
  with `POPAMP_BLESS=1 cargo test -p popamp golden` and look at them
  before committing.
- Command-line parsing and control-socket round trips.
- Tags: each field and the cover written and read back in MP3, FLAC, Ogg
  Vorbis and M4A (silent files in `crates/popamp-audio/tests/fixtures/`,
  made with ffmpeg); tags the editor does not show survive; bad years and
  track numbers are refused; a failed save leaves the file whole. The File
  info and Album art windows as a user drives them.
- Gallery: the catalog, the tabs and the three orders, checksums,
  installing and updating skins and plug-ins, archives that try to
  write outside their folder.
  `cargo test -p popamp gallery_live -- --ignored` installs everything
  popamp.app offers into a folder of its own.

### Published packages

`packaging/check-repos.sh` installs `popamp` and `popamp-plus` from the
published repositories in fresh containers (openSUSE Tumbleweed and Leap
16.0, Fedora 43 and 44, Debian 12 and 13, Ubuntu 22.04 and 24.04), with
the commands of docs/building.md, and checks the version, the installed
files, the libraries, the skin MIME type, a `/RENDER` of the windows,
the skins (and with popamp-plus the plug-ins) from the same repository,
and a clean removal. All eight passed for 0.7.0 on 2026-11-10.

### Flatpak

`packaging/flatpak/io.github.popamp.popamp.yml` builds Popamp Plus with
`flatpak-builder`. On 2026-11-26 the build was installed from bundles
in Kubuntu 26.04 (Plasma Wayland), Ubuntu 26.04 (GNOME Wayland), Linux
Mint 22.3 (Cinnamon, X11) and Pop!_OS 24.04 (COSMIC). Playback, the tray
icon, MPRIS, a second launch handing its file to the running player
(also a file outside the Music folder, through the document portal), and
docked windows following the main one worked everywhere. Toolbar mode at
the top and the bottom, with maximized windows kept out of its strip,
worked on KDE, GNOME and Cinnamon. On COSMIC the Flatpak has no toolbar
mode: cosmic-comp does not offer wlr-layer-shell to sandboxed programs,
so the menu leaves the item out.
The repository at apt.popamp.app/flatpak/, built by popamp/apt's Flatpak
workflow, was installed the same day with `flatpak install --user
https://apt.popamp.app/popamp.flatpakref` in the Pop!_OS VM: the remote
checks the commits and the summary against the key.

### Manual checks performed

All playback checks ran at volume 0 through the real ALSA/PipeWire output.

| Check | How | Result |
|---|---|---|
| Local formats | `cargo run --release -p popamp-audio --example play -- --vol 0 <file>` with one file per format (made from the same track with `ffmpeg`) | MP3, FLAC, Ogg Vorbis, WAV, AIFF, M4A (AAC), MKA (FLAC) play with correct length and bitrate. Opus: covered by `cargo test` (length, level, seeking, tags), not yet checked by ear. Tracker modules: `cargo test` with a generated MOD (pitch, length, title, seeking), not yet checked by ear. CUE sheets: `cargo test` (parsing, a part of a WAV plays, seeks and ends in its bounds, the sheet's tags); not yet checked in the player. MIDI: `cargo test` with a one-sine SoundFont (pitch, length, seeking, tempo, `.rmi`); a 6-minute file with the system SoundFont opens in 0.06 s, seeks in under 0.1 ms and renders 80 times faster than real time; not yet checked by ear. |
| Gapless and end of list | `play` example with two files | next track starts without a gap; "finished" is reported after the last one |
| Internet radio | `play` example with `https://ice1.somafm.com/groovesalad-128-mp3` | plays at 127 kbps; station name and live ICY song titles shown |
| Window rendering | `POPAMP_SNAPSHOT=<dir> popamp /NEW /INIDIR=<tmp>` renders the windows to PNG | Default skin and the original Winamp 5.9.2 base skin both line up exactly. Windowshade, wide playlist (with vis) and oscilloscope modes correct. |
| Skins | `popamp Base.wsz` | `.wsz` is installed to the skins folder and applied; loading the archive matches loading the unpacked folder |
| Remote control | `/PLAY /PAUSE /PLAYPAUSE /NEXT /STOP /VOL= /SHUFFLE= /COMMAND= /STATUS /QUIT` against a running instance | all work |
| MPRIS | `gdbus call --session -d org.mpris.MediaPlayer2.popamp …` (Play, Next, Properties.GetAll, Quit) | all work; metadata and position correct |
| Tray and notifications | `popamp /NEW /INIDIR=<tmp>` with `tray_icon=1` and `notify_tracks=1`, then `busctl` (the item's ToolTip, Scroll; MPRIS Next) and `dbus-monitor` | the item registers with the watcher, its tooltip follows the track; each new track gives one notification with its cover, replacing the last |
| Global hotkeys (X11) | `global_hotkeys=1`, `xdotool key ctrl+alt+<key>` for each key, state through MPRIS | all work but Left/Right, which GNOME's workspace switching takes first |
| Window icon | `xprop -name Popamp _NET_WM_ICON` | 64×64 icon set |
| Session persistence | quit, inspect `popamp.ini` / `popamp.m3u8` | settings, positions and playlist saved |
| Maximized library | VMs (2026-11-23): Windows 11 26300, Linux Mint 22.3 Cinnamon (X11), Pop!_OS 24.04 COSMIC (Wayland); the title's Maximize button and a double click on the title; File info from the track menu | Windows and Mint: it fills the screen above the taskbar or panel and comes back beside the main window; File info opens over it. COSMIC maximizes it itself between the top panel and the dock, the equalizer and playlist hide while it is in front. Under COSMIC its menus open away from the mouse. GNOME on X11 not tried. |

**Not yet tested by hand:**
- Mouse interaction: dragging, docking and snapping, sliders, pop-up menus,
  playlist drag-reorder.
- Keyboard shortcuts.
- The `zenity`/`kdialog` dialogs, Popamp's own dialogs (Jump to file,
  File info, Album art, Skin browser, Preferences) and the menus at each
  size, and drag & drop.
- Saving tags of the track that is playing, on Windows.
- Double size.
- A skin with a `region.txt` window shape.
- ReplayGain-tagged files, and the ReplayGain scanner by ear.
- Crossfade by ear, and the History window.
- Media keys through the desktop.
- The tray icon's clicks and menu on screen.
- Global hotkeys under Wayland with a real portal (tested against a fake
  one on the session bus).
- Global hotkeys and the tray icon on Windows (their threads are covered
  by tests in Proton, where the icon is added; its menu is not seen yet).

Please report problems with these first.

