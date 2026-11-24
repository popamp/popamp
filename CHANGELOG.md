# Changelog

All notable changes to this project are documented here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and versions follow
[Semantic Versioning](https://semver.org/).

## [Unreleased]

## [1.2.0] - 2026-11-24

The media library grows toward jetAudio's: a display with the visualizer
over the list, transport under it, a cover panel, a views tree, and its own
playing. The skins' new library windows come with skins 2.1.0.

### Added

- **Library display and transport**: the library window has a display
  under its title (play state, clock, song title, bit rate and a
  visualizer) and, under the list, a position bar, the transport buttons
  and the volume. They move like the main window's.
- **Maximize the library** (Toggle library maximize, a double click on its
  title, or "Maximized" in its menu): it fills the usable part of its
  monitor as a window of its own and comes back maximized next time.
- **The library plays on its own**, apart from the playlist: a double
  click or Enter plays, a click on another track plays it while the library
  plays, the middle button adds the track to the playlist (Ctrl+click
  still picks several).
- **Cover panel** under the views: the cover of the track at the cursor with
  its title, artist, album, year, format and bit rate.
- **More columns** in the library table: track number, file name, comment,
  bit rate, plays, last played and added. A right click on the header
  turns them on and off; a border in the header drags.
- **Views tree**: Playlists, Smart playlists and Folders open and shut, and
  a library folder opens into its folders, so the table can show any
  folder inside the library. The tree comes back as it was left.
- **A picture dropped on tracks becomes their cover**: JPEG, PNG, GIF or
  BMP onto a row, the picked rows or the cover panel. Popamp asks first and
  writes only the formats it writes tags into.
- **Resizable library parts**: the gap right of the left panel and the gap
  over the table (under the artists and albums) drag, with a resize
  cursor; a double click gives the size back to the window.
- **Timed library scan** ([#71](https://github.com/popamp/popamp/issues/71)):
  Media Library preferences has two check boxes: "Scan the folders at
  startup" (on by default; until now only in the settings file) and "Scan
  the folders periodically" (off by default, as a scan of a large library
  keeps the disk busy), every "Scan every (minutes)", 60 by default.
  Each scan takes in new and changed files and drops the ones gone from
  disk; a folder that cannot be read, such as an unplugged drive, keeps its
  tracks.

### Changed

- The library has no shade mode any more; its title maximizes it instead.
- The library's button row under the table (Play, Enqueue, Add folder,
  Rescan) is gone; all of them are in its menus. The "enqueue by default"
  option is gone with the library's own playing.
- The labels of typed values in Preferences are wider and no longer cut
  off.

### Fixed

- **Vector skins at in-between sizes** (such as 1.5x): each part of the
  picture is drawn at its exact place instead of being stretched to whole
  pixels, so lines no longer thicken or shift by a pixel between parts.
- **COSMIC: the equalizer and the playlist drag with a tablet or in a
  virtual machine.** A pointer that reports only where it is, without
  how far it moved, left them in place.

### Known issues

- **COSMIC: dialogs open under the equalizer and the playlist.** COSMIC
  draws them over the program's other windows, so a dialog such as File
  info can come up partly covered; drag it aside by its title each time
  ([details](docs/wayland.md#what-does-not-and-why)).

## [1.1.0] - 2026-11-22

### Added

- **Play counts and ratings** ([#65](https://github.com/popamp/popamp/issues/65)):
  the library counts a play once enough of the track was heard ("Count a
  play after hearing" in Media Library preferences, 50 % by default;
  pause adds nothing, a seek does not reset). Tracks get 0 to 5 stars
  from a Rating column and a Rate menu. Both are kept in Popamp's own
  `stats.txt`; audio files are never written to. A track that was renamed
  or moved keeps its plays and stars after a rescan.
- **Smart playlists** ([#65](https://github.com/popamp/popamp/issues/65)):
  lists that pick tracks by rules (artist, genre, plays, rating, when it
  was added or last played, …), with groups, all/any, two levels of order
  or random order, and a limit in tracks or minutes. They are made and
  changed in an editor (New smart playlist, Save search as smart
  playlist, Edit, Delete) and kept in `smart.toml`. Built in: Recently
  added, Last played, Most played, Never played, Favourites, Random 50.
- **Own playlists in the library:** New playlist… makes a plain list;
  tracks dragged onto it go in at the end. They are M3U8 files in the
  config folder's `playlists/`. Every playlist shows how many tracks it
  has, and the Playlists group is there before the first one is made.
- **Several folders at once:** Add folder… in the library, and Open
  folder for the playlist, take several folders picked together (with
  KDE's kdialog it is still one at a time). The library's dialog opens in
  the folder that holds the one added last, so the next one is a click
  away; the playlist's opens beside the last file or folder picked.
- **macOS downloads (experimental):** a release also builds
  `Popamp-<version>-macos-universal.dmg` and its Popamp Plus twin, one
  disk image for Apple Silicon and Intel Macs on macOS 12 and newer, with
  libopus, libopenmpt and the MIDI SoundFont inside. If the macOS build
  fails, the release goes out without it. Until the app is signed with a
  Developer ID, macOS asks once at the first start
  ([how to open it](docs/building.md#macos)). Made by Martin Kovachev
  ([miraclebg](https://github.com/miraclebg)).

### Changed

- **Menus in the library:** a right click on a track gives what is done
  with tracks (Play, Enqueue, Rate, File info…); one on the left pane
  gives the folders' and the playlists'. File info opens on the library
  track without adding it to the playlist.
- **File info says why tags cannot be changed:** for a track of a CUE
  sheet (its tags are in the sheet), a stream, a file that is gone, or a
  file Popamp cannot read (damaged, or still downloading).

### Fixed

- **The shaded playlist's left end:** a skin with a full outline kept its
  left border only while the playlist was focused.

## [1.0.4] - 2026-11-20

### Added

- **Removing a plug-in:** a Remove button on the plug-in's page in
  Preferences asks, then deletes its folder and takes back its
  permission, so a new copy asks again. A plug-in installed as a package
  says to remove it with the package manager instead.

Taken from the fork [Popamp Deluxe](https://github.com/Thermoptic/popampdeluxe)
by Thermoptic.

- **Volume and balance in the shaded equalizer,** as in Winamp: a classic
  skin's sliders in the windowshade strip, with the thumb that changes in
  the lower, middle and upper third of the range.
- **Windowshade for the Media Library:** a double click on its title bar
  rolls it up to a strip and another unrolls it; a classic skin also has
  the shade button. A skin that draws the library window itself gets the
  strip too, unless it gives its own `shade`. The setting is `ml_ws`.
- **Title-bar shortcuts** (Preferences > Shortcuts): a right or middle
  click, or the wheel, over the title bar of the main, equalizer or
  playlist window runs previous, next, pause, stop, shuffle, repeat,
  volume up or volume down. The rules are a table with Assign and Remove,
  one action per click of a window; a right-click rule replaces that title
  bar's menu. The setting is `title_shortcuts`.

### Changed

- **Plug-ins moved into Preferences** ([#62](https://github.com/popamp/popamp/issues/62)):
  Preferences > Plug-ins says how to turn one on and lists every plug-in
  under it. A plug-in's page has an Enabled tick at the top, what it
  does and may do, and its permissions and settings, greyed out while it
  is off. The commands it runs are put away behind "Show the commands it
  runs". The menu "Plug-ins…" opens Preferences there. OK loads the
  plug-ins again only when something changed, or one stopped.
- **A skin folder from Install from file:** picking a vector skin's
  `skin.toml` puts on the folder it is in, where it is, instead of copying
  the lone file; the skin reloads as its files change.
- **macOS:** the code takes the Mac's keys (⌘ first, and menus that show
  them; the table is in [usage.md](docs/usage.md)), pastes copied links
  and files into the playlist with ⌘V, and cuts classic skins to their
  shape. There is still no macOS download from this project.

### Fixed

- **Dialogs on a low screen** ([#62](https://github.com/popamp/popamp/issues/62)):
  a dialog is no taller than what the desktop's panels leave of the
  screen. A page of Preferences that does not fit scrolls, with the
  wheel, a scroll bar, Page Up and Page Down, and Tab brings the field it
  moves to into view; OK and Cancel stay in sight.

## [1.0.3] - 2026-11-19

### Added

- **WavPack:** a new decoder plug-in,
  [plugin-wavpack](https://github.com/popamp/plugin-wavpack), plays `.wv`
  files, whole-album rips with their CUE sheets included.
- **CUE sheets inside the album file:** a FLAC, APE or WavPack file that
  keeps its sheet in a CUESHEET tag, or a FLAC file with a CUESHEET
  block, is listed as its tracks.

### Changed

- **The default skin** shows Popamp's lightning bolt in the main window,
  in place of the little spectrum in its well.

### Fixed

- **Decoder plug-ins that hand over seconds of audio at once (APE):** the
  position bar and the visualizer move smoothly instead of jumping once a
  second, and pause and seeking answer at once.
- **Album rips with a CUE sheet:** an album file that a `.cue` beside it
  cuts into tracks is listed as those tracks, not as one long file, when
  it is added alone or with its folder, to the playlist or the library.
  When a rip has two sheets of the same file (one for the WAV, one for the FLAC), its
  tracks are listed once. A sheet that names a WAV file the album was
  since converted from also finds it in a format a decoder plug-in adds,
  such as APE.
- **Cyrillic CUE sheets and playlists** saved in Windows-1251 read
  correctly; before, their titles came out garbled and the sheet could
  not find its audio file.

## [1.0.2] - 2026-11-18

### Added

- **Toolbar mode as a Wayland program:** the bar is a layer of the
  desktop where the compositor has wlr-layer-shell (COSMIC, KDE Plasma,
  Sway, Hyprland), keeps maximized windows out of its strip and takes the
  width the desktop's panels leave it. The equalizer and the playlist sit
  against it as on X11. It takes no keys there.
- **Snapping to the screen's edges on COSMIC:** the equalizer and the
  playlist snap to the edges of the monitor as Popamp's windows do
  elsewhere. COSMIC tells Popamp where its main window is
  (cosmic-toplevel-info). The main window, which the compositor moves,
  snaps with COSMIC's *Edge gravity* (Settings → Desktop → Window
  management). They stop at the desktop's panels, not under them:
  Popamp asks COSMIC how much room the panels leave on the monitor.
- **Move docked windows together** (Options menu, off by default):
  dragging the equalizer or the playlist by its title bar, while it is
  docked to the main window, moves the whole docked group, which snaps
  as one. On COSMIC the compositor moves the group, which then snaps
  only with COSMIC's *Edge gravity*.
- **COSMIC:** while the main window is dragged, the equalizer and the
  playlist not docked to it are hidden, and come back where they were,
  instead of going along with it.
- **Toolbar mode > Full width, under side panels** (off by default, X11
  and Xwayland): the bar spans the whole screen, under a side dock that
  makes room for it. Ubuntu's dock does, but tells the system only of
  its full height, so the bar used to leave an empty corner beside it.

### Fixed

- **Dragging at a screen edge under X11:** docked windows stay together
  when the group reaches an edge. The window manager holds each window
  inside the work area, so the windows further from the edge went on
  and slid over the held one, and the next drag started with the held
  window stuck at the edge. The group now stops as a whole.
- **COSMIC:** Popamp runs there as a Wayland program, since COSMIC's
  Xwayland does not let a program move its windows. The equalizer and
  the playlist follow the mouse when dragged, even at the screen's edge,
  and the main window changes size with the others (Ctrl+D). COSMIC
  calls every floating window tiled, and winit then ignored the new
  size; Popamp builds with a fork of winit that fixes this until the
  fix ([winit#4847](https://github.com/rust-windowing/winit/pull/4847))
  is released.
- **COSMIC:** the main window dropped in a screen corner keeps its size
  instead of being stretched to the quarter of the screen COSMIC tiles
  it into.
- **Toolbar mode on KDE Plasma (Wayland):** maximized windows no longer
  go under the bar. KWin ignores the struts of programs that run
  through Xwayland, so Popamp keeps the bar's strip with an invisible
  layer of the desktop (wlr-layer-shell) under the bar.

### Changed

- **Software centres:** the AppStream data has screenshots and a
  current description, so the AppImage catalog and the software centres
  show the player with its skins instead of an empty playlist.
- **Toolbar size:** a bar too wide for the screen takes the biggest
  smaller size that fits, and *Toolbar mode > Size* offers only those;
  the size chosen is kept for a wider screen.
- **Documentation:** pages on Ubuntu and KDE Plasma
  ([ubuntu.md](docs/ubuntu.md), [kde.md](docs/kde.md)), linked from the
  README's new *Limits and desktop-specific behaviour*; checked from the
  live images of Ubuntu 26.04.1, Kubuntu 26.04.1 and Linux Mint 22.3.

## [1.0.1] - 2026-11-16

### Fixed

- **Dragging under X11:** a window and the windows docked to it follow
  the mouse again. With window managers that report moves late, such as
  Xfce's, they fell behind the mouse, jumped back, and could end up far
  off the screen ([#46](https://github.com/popamp/popamp/issues/46)).
- **Raising under X11:** a click on one of Popamp's windows brings up
  the others with it, so another program's window no longer cuts the
  docked group in two.
- **Dialogs on Linux:** when neither zenity nor kdialog is installed,
  the main window says *DIALOGS NEED ZENITY* instead of doing nothing;
  when the dialog program fails, it says *DIALOG FAILED, SEE THE LOG*
  and the log has the reason.

### Changed

- **macOS:** the code takes the Mac's own dialogs, fonts and Finder's
  "Open with", and draws sharp on Retina screens. There is still no
  macOS download from this project.

- **AppImage:** it carries the libraries a desktop may lack (xkbcommon,
  Xcursor, Xi, Opus, libopenmpt and what they need), so it starts on
  systems without them; before, it quit at once where xkbcommon-x11 was
  missing. Its file name drops `linux`: `Popamp-<version>-x86_64.AppImage`.

## [1.0.0] - 2026-11-14

### Added

- **A window and a bar in one skin:** a skin can describe its main
  window both as a window and as a bar along a screen edge
  (`[window.bar]` beside `[window.normal]`; a skin on classic sheets can
  add just the bar). The main menu then offers *Main window as: A window
  / A bar*. The clock takes `font_size`, so the bar can show a smaller
  one.
- **Colour sets:** a skin can name its colours in `[colors.<set>]` and
  give several sets, so one skin has many looks. SVG shapes take a
  colour by class, and the same names work in `[text]`, `[clock]`,
  `[playlist]` and `[vis]`. The main menu offers *Colours* when a skin
  has more than one set, and the pick is remembered.
- **macOS:** sound plays through CoreAudio, on the default device or the
  one picked, and the Mac's sound devices are listed by name (by
  miraclebg).
- **Wayland:** Popamp can run as a Wayland program, without Xwayland. It
  does so by itself on a Wayland desktop where X11 programs cannot run;
  elsewhere it stays an X11 program unless `wayland=1` is set in
  `popamp.ini`. Wayland does not let a program place its windows, so the
  equalizer, the playlist, menus and tooltips are parts of the main
  window there: they dock, snap and move as before, and all go along
  when the main window is moved. Not there, because Wayland has no way
  for it: always on top, the bar view, remembering where on the screen
  the windows were, and keeping menus on the screen at its edges.
  Dropping files onto the windows does not work there yet.
- **Media library** (`Alt+L`), a window of its own: give it folders and
  it lists their tracks, with all the music, the newest tracks and each
  folder on the left, artists and albums side by side above the track
  table, and a search over every tag and the path. Play puts the picked
  tracks in place of the playlist, Enqueue adds them; Enter and a double
  click enqueue, or play when Preferences > Media Library says so, and
  Shift+Enter does the other. Tracks dragged onto the playlist go in
  where they are let go, the cursor telling whether they can. The folders are
  read again in the background at start; files that did not change are
  not read again. Classic skins and vector skins drawn on the classic
  sheets give it the playlist's frame; a vector skin with its own windows
  draws it as a `[[window]]` with `key = "library"`.
- **Oscilloscope afterglow:** a fourth oscilloscope style (Visualization
  menu, *Scope: afterglow*) leaves each trace to fade over five frames
  through the skin's oscilloscope colours, like the glow of a
  cathode-ray tube.
- **Update check:** at start, at most once a day, Popamp reads
  `https://popamp.app/latest.json` and, when it names a newer version,
  says so in the display and puts *Popamp X is out* at the top of the
  main menu, with *Download…* (opens the release's page) and *Skip this
  version*. Installed from a .deb or .rpm, it says to update through the
  package manager instead. Popamp never downloads or installs an update
  itself; the request carries only its version, in the User-Agent, and
  without network nothing is said. Preferences > Internet > *Check for a
  new version* turns it off.

### Changed

- The default skin is redrawn from one set of parts, with crisp
  transport signs. It has a bar view as well as the window, and five
  colour sets: Slate, Graphite, Plum, Pine and Wine.
- Preferences is split into sections, listed down its left side:
  Windows, Playlist, Playback, Media Library, Desktop and Internet.
- The main menu's *Bar position* and *Bar size* are now *Position* and
  *Size* in its *Toolbar mode*, which also turns the bar on and off when
  the skin has a window as well, and they can be set before it is on.

### Fixed

- A song title with letters a classic skin's font lacks (Cyrillic, say)
  is written in the playlist's font, in the colour of the skin's, instead
  of leaving those letters out.
- What a menu item changes shows at once in every window: an equalizer
  preset picked from the Presets menu moved the sliders only after the
  next click in that window.
- The media library, equalizer and playlist windows can be moved while
  the main window is a bar; a window moved off the bar stays where it
  was put until the bar is turned off.

## [0.9.1] - 2026-11-13

### Fixed

- An internet radio that sends Ogg (Vorbis, Opus or FLAC in Ogg) no
  longer freezes Popamp when the next song starts: playback goes on, and
  Stop and closing the window work again.
- Such a radio's display now shows each next song's artist and title,
  read from the song's own tags.

### Changed

- The code builds and runs on macOS, without sound yet; there is no
  macOS download.

## [0.9.0] - 2026-11-12

### Added

- **Opus:** `.opus` files (and Opus in Matroska and WebM) play, seek and
  have their tags read and changed. Decoding goes through the system's
  libopus, loaded when the first Opus track opens, so building needs no
  Opus headers; the packages depend on it.
- **MIDI:** `.mid`, `.midi`, `.kar` and `.rmi` files play through a
  SoundFont synthesizer (rustysynth). The SoundFont is the system's
  General MIDI one (`fluid-soundfont-gm` and the like), or the file set
  under MIDI SoundFont in Preferences. Seeking is instant: it jumps, and
  the notes held at that point start again.
- **Tracker modules:** MOD, XM, S3M, IT, MPTM and the other common module
  types play through the system's libopenmpt, loaded at run time, with
  their title, artist and message. The packages recommend it.
- **CUE sheets:** opening a `.cue` adds its tracks, each one a part of the
  album file with the sheet's title and performer. A track plays, seeks
  and ends within its part; a sheet that names `album.wav` finds
  `album.flac` beside it. Tracks are kept in playlists as
  `album.cue#3`.
- The Windows downloads carry what these need: `opus.dll`, libopenmpt and
  the GeneralUser GS SoundFont for MIDI, with their licences (see
  THIRD_PARTY.md). The installer offers Popamp for the new file types.
- **Decoder plug-ins** (plug-in API 1.1): a plug-in of `kind = "decoder"`
  plays local files of the extensions it lists, in folders, playlists,
  CUE sheets and the Open dialog. It reads only the file it plays and
  may ask for no permissions. Popamp's own formats always come first.

- **Crossfade** (Preferences > "Crossfade (ms)", off by default): the
  end of a track overlaps the start of the next one. Tracks of different
  sample rates still change gapless, without the overlap.
- **History** (`Ctrl+H`): the last 5000 plays, searchable, kept in
  `history.txt` in the settings folder.
- **ReplayGain scanner:** select tracks, then *Scan ReplayGain* in the
  playlist's menu. It measures track and album gain (EBU R128) and keeps
  the values in `replaygain.txt`; the audio files are never written.
  A window counts the tracks and can cancel the scan (nothing is saved
  then). At the end it waits for OK, unless "Keep the scan window open"
  (in it and in Preferences) is off. Playback uses the values once
  *Options > ReplayGain* is on.
- **Tray icon** on Linux (StatusNotifierItem) and Windows, on by default:
  a click brings Popamp up, a middle click plays or pauses, and its menu
  has the transport buttons. "Minimize to the tray" hides the windows
  into the icon, as Winamp does, also with `/STARTMIN`.
- **Track notifications** on Linux, with the cover, off by default.
- **Global hotkeys** as in Winamp (Ctrl+Alt+Insert, Home, End, Page
  Up/Down, arrows), off by default. On X11 and Windows; under Wayland
  through the desktop's GlobalShortcuts portal.
- The tray, notifications and hotkeys are set in Preferences > Desktop.

### Fixed

- Windows: Popamp could stop responding a few minutes after start, on
  Play or when files were dropped on it from Explorer. The media
  controls' hidden window lived on a thread that never answered window
  messages, and whatever sent it one, the UI with it, waited for good.
- Windows: after a pause, starting another track (or seeking or
  stopping) could leave the sound stuck, and closing Popamp then hung.
  Emptying the output kept it marked paused, so it never started again
  and the next write waited for room forever. Since 0.5.0.
- Windows: the media card by the volume flyout kept the cover of the
  last track that had one; a track without a cover now shows the logo.
- The cover picture from a track's tags, written to the temporary folder
  for the desktop's media controls, stayed there after Popamp closed.
  The next start removes the ones left behind.

## [0.8.0] - 2026-11-11

### Added

- Golden-image tests: `cargo test` draws every window state of the
  built-in skin and compares it pixel by pixel with committed PNGs, on
  Linux and Windows, so a change to a window cannot go unnoticed.
- Output device in Preferences: a list of the sound servers and card
  outputs on ALSA, and of the playback devices on Windows. The choice is
  kept by name (`output_device` in the settings); playback moves to it at
  once, from where it was, and an unplugged device falls back to the
  default one.
- "Open with Popamp" in Preferences: on Linux, check boxes that make it
  the default player for audio files, playlists and skins (in
  `~/.config/mimeapps.list`, with a desktop entry of its own for an
  AppImage or a build from source); on Windows, a button that opens
  Settings > Default apps at Popamp, where the installer now lists it.
- Gallery window (Skins or Options > Gallery): the skins, plug-ins and
  visualizations of popamp.app on tabs of their own, sorted by
  popularity, rating or date as on the site, with previews, ratings,
  what each plug-in may do, and install or update in a click. Downloads are checked against the catalog's size
  and SHA-256; plug-ins unpack into a folder of their own, replacing an
  older version only once the new one is whole, and stay off until
  turned on in Plug-ins. In the build without plug-ins only the Skins
  tab lists anything.
- Tag editor: File info (`Alt+3`) changes the tags and the cover of MP3,
  FLAC, Ogg Vorbis and MP4/M4A files, keeping the tags it does not show.
  A save goes into a copy that then replaces the file. Years must be
  dates and track or disc numbers numbers; WAV, AIFF and streams are
  shown only.
- Album art window (`Alt+A`, main menu): the cover of the current track,
  from its tags or a cover picture beside it, updated as the track or its
  tags change.
- rpms of the skins and plug-ins, in the openSUSE and Fedora repositories
  next to popamp: `popamp-skin-<name>` and `popamp-plugin-<id>`, the same
  files as their debs (`packaging/obs/publish-addons.sh`).
- Skins: `align = "left" | "center" | "right"` on a `clock` element places
  a font's play time across it, so it can keep against a line or a label
  whatever the width of the font's digits. Left is the default.
- `min_popamp = "0.8.0"` in a skin's `[meta]` or a `plugin.toml`: the
  oldest Popamp it works with. Older ones say a newer Popamp is needed,
  as the first thing they check, instead of failing on a key they do not
  know; the gallery keeps the installed plug-in rather than replace it
  with one that would not start.

### Changed

- The default skin has a repository of its own,
  [popamp-skin-default](https://github.com/popamp/popamp-skin-default), and
  is still built into the program. It can be forked without the player.
- Menus, tooltips and Popamp's own windows (Preferences, Gallery, File
  info and the others) follow the display scaling (Windows' scaling
  setting, `Xft.dpi` on X11), not the size of the player's windows, with
  their text set at that size, so it stays sharp. Covers and previews are
  kept large enough for ×3.
- The Skin browser (`Alt+S`, Options) is a window of Popamp's own instead
  of a zenity or kdialog list: a click puts a skin on at once, and its
  buttons install a skin from a file or open the Gallery.

### Fixed

- Bringing the player up from its taskbar icon, the desktop's media
  controls or by starting it again raises the equalizer and the playlist
  too, not only the main window. On X11 the two are tied to the main
  window, so they also minimize with it.
- The playlist no longer grows past the bottom or the right of the screen
  when resized, which made the window manager push it over the equalizer.
- A bigger window size shortens a playlist that would no longer fit, so
  its resize grip stays on the screen.
- The options menu opens the Preferences; before, only Ctrl+P did.
- A click on the position bar no longer makes the slider and the clock
  jump back for a moment before they show the new place, and a second
  seek while paused shows its own position.
- Windows: right-click menus closed as soon as they opened.
- Windows: the equalizer and playlist did not come back after the main
  window was minimized and restored from the taskbar; they are now owned
  by the main window and minimize and restore with it.
- Windows: visualizer plug-ins closed at once ("the underlying handle is
  not available"): winit gives out a window's handle only on the event
  loop's thread, and the GPU draws on a thread of its own. Why a plug-in
  stopped is now also written to the log.
- Windows: a bar crept up the screen, reserving one strip above another:
  Windows moves the bar out of the strip just reserved for it, and the
  strip was told apart from the desktop's panels by where the bar was.
  It is now known by what was reserved, and the bar goes back into it.
- Vector skins at x1.5 and other sizes between whole pixels: text came out
  longer than its place, and the play time lost its last digit (Steel).
  Glyphs are still drawn at a whole size, but spaced at the exact one.
- The play time of a vector skin sits in the middle of its place by the
  digits themselves, not by the room a font leaves above them: with the
  system font (Modern) it sat low on Windows, over the visualization.

## [0.7.0] - 2026-11-09

### Added

- **Visualizer plug-ins** (`--features visualizers`, part of popamp-plus):
  a WebAssembly plug-in supplies WGSL shaders and the player makes every
  GPU call, in a window of its own that can go full screen. While it is
  full screen and music is playing, the screen saver and monitor sleep
  stay off. The first one is
  [Spectrum](https://github.com/popamp/plugin-spectrum).
- A journal for each plug-in records every command, network request and
  file read, allowed or refused.

### Changed

- **A plug-in now runs only the commands its manifest spells out.**
  `permissions.exec` is a list of command templates: you see each full
  command when you allow it, any change asks again, and the plug-in only
  fills in typed values. A plug-in made for 0.6.0 that runs programs has
  to be updated; [YouTube](https://github.com/popamp/plugin-youtube)
  1.1.0 is.
- **You decide what a plug-in may reach and read.** Any address can be
  unticked. An unusual port, this computer or your network must be
  written out in the manifest and stay off until ticked; redirects are
  not followed. A plug-in that reads files gets the folders you choose.
- The taskbar icon at 32 px is the bolt, like the larger sizes.
- The default skin is drawn from whole shapes instead of thousands of
  small ones: its files are five times smaller and show no seams at
  fractional sizes.

### Security

- An `<image>` in a skin's SVG can no longer read files on the computer;
  only pictures carried inside the SVG are drawn.

## [0.6.0] - 2026-11-07

### Added

- Skins and plug-ins are also read from `popamp/skins` and
  `popamp/plugins` in each of `$XDG_DATA_DIRS` (`/usr/share/popamp/…`),
  where their packages install them. Your own copy of the same name wins,
  and a packaged plug-in stays off until you allow its permissions.

### Changed

- The apt repository moved from GitHub Pages to https://apt.popamp.app,
  built and signed by [popamp/apt](https://github.com/popamp/apt) from
  the latest release of every popamp repository, so skins and plug-ins
  can join it as packages. Add it again with the commands in
  `docs/building.md`; the signing key stays the same.
- The `popamp-plus` deb provides `popamp` at its own version, so a
  package that needs a newer Popamp, such as a skin, installs next to
  either build.

## [0.5.0] - 2026-11-06

The first release of Popamp. It starts at 0.5.0 rather than 0.1.0, which
was Linamp's, since much has changed since then.

### Changed

- Building needs Rust 1.96 or newer (was 1.89): the plug-in host,
  wasmtime through Extism, needs it.
- YouTube left the core for a plug-in,
  [plugin-youtube](https://github.com/popamp/plugin-youtube), which needs
  a build with plug-ins. The settings `ytdlp_path`, `ffmpeg_path` and
  `ytdlp_format` move to `[plugin.youtube]` when the new version starts
  (only those changed from their defaults). A YouTube link without the
  plug-in fails with "no plug-in plays this link", as does any web page
  opened as a stream.
- Tracks open on a thread of their own. Stop, Next and Prev work while a
  stream is still connecting, instead of waiting for it (up to 20
  seconds for a silent server).
- Each location is played, listed and described by a source: files,
  HTTP streams, and plug-ins. A source names the program to run when a
  track plays through one, and the player runs it. A slow source shows
  "CONNECTING…" while it opens.
- A menu that does not fit below the mouse opens above it instead of
  over it, so a second click on the button that opened it closes it.
- The project is now Popamp, a separate project grown from
  [Linamp](https://github.com/sayoojkkarun/linamp) 0.1.0. The program,
  crates, packages and settings folders (`~/.config/popamp`,
  `~/.local/share/popamp`) carry the new name.

### Added

- Windows (x86_64). Releases have an installer and a portable zip of
  each build. The installer adds a Start menu entry, opens skins with
  Popamp and lists it in "Open with" for audio files and playlists;
  settings are in `%APPDATA%\popamp`, or, in the portable zip, beside
  `popamp.exe`. The program has the Popamp icon. Sound goes to the
  default device through WASAPI, fonts come from the system's font list,
  and files, folders and questions use Windows' own dialogs. Started
  from cmd or PowerShell, `/STATUS`, `/VERSION` and errors print in that
  console; started otherwise, no console window opens. Remote commands reach a
  running instance through a named pipe. A bar skin's strip is taken out
  of the work area, as the taskbar's is. The media keys and the player
  card by the volume flyout show the track and its cover, play, pause,
  stop, skip, seek, and switch shuffle and repeat.
- Media controls show the track's cover: the picture in its tags (the
  front cover first), else an image beside it named `cover`, `folder`,
  `front`, `album` or `albumart` (`.jpg`, `.jpeg`, `.png`). On Linux it is
  MPRIS's `mpris:artUrl`, for desktop widgets and `playerctl`.
- Two builds: `popamp`, the player, and `popamp-plus`, built with
  plug-ins. Releases have a deb and an AppImage of each; `popamp-plus`
  replaces `popamp` and the other way round. `/VERSION`, About and the
  title display say which one runs.
- Plug-ins: WebAssembly modules run by [Extism](https://extism.org), each
  in its own folder under `~/.local/share/popamp/plugins/` with a
  `plugin.toml`. A source plug-in takes links on the hosts its manifest
  names, lists them, describes them and tells the player how to play
  them; it can start only the programs its manifest names, and reach only
  the hosts it names. "Plug-ins…" in the Options menu shows what each one
  may do next to its check box, turns it on and edits its settings; a new
  version that asks for more stays off until it is allowed again. A
  plug-in that crashes or runs over 60 seconds three times in a row is
  switched off, and the title display and "Plug-ins…" say why. A dsp
  plug-in changes the sound after the equalizer, block by block, on the
  engine thread; a block that fails or is late
  plays unchanged. A track given at startup waits for the plug-ins to
  load (up to 30 seconds), so it goes to its plug-in. The calls are in the
  [`popamp-plugin-api`](https://github.com/popamp/popamp-plugin-api) crate and in
  [docs/plugins.md](docs/plugins.md). Plug-ins are built only with
  `cargo build --features plugins`; the default build is the player and
  its skins, without the WebAssembly runtime.
- [Popamp Steel](https://github.com/popamp/skin-steel), a silver and
  navy vector skin laid out like the "modern" players of the 2000s: a
  dotted blue display with a segmented analyzer, round transport buttons,
  toggles with blue lamps, an equalizer with a tab and a dB scale, and a
  shade mode for all three windows. Its texts use its own pixel fonts (`fonts/*.ttf`, Latin and
  Cyrillic).
- Bar skins: `placement = "bar-bottom"` (or `"bar-top"`) makes a vector
  skin's main window a strip across the screen at that edge, inside the
  desktop's own panels (it moves when they appear or resize). Its strip is
  reserved like a taskbar's, the playlist and the equalizer sit against it
  at the right end, and the main menu's "Bar position" moves it to the
  other edge. The bar has its own size, ×2 unless "Bar size" in the main
  menu says otherwise, whatever size the other windows are.
  [Popamp Bar](https://github.com/popamp/skin-bar) is Popamp Modern with
  a 14 pixel high bar, as high as a classic window shade.
- A marquee without `area` writes the title in all of its element, so it
  can stretch with a resizable window.
- `see_through = true` in a vector skin's window: the window shows only
  where its images are opaque (and its list, vis and EQ graph), so a
  resizable window can have rounded corners.
- [Popamp Modern](https://github.com/popamp/skin-modern), a silver
  vector skin with its own layout, round buttons that take clicks only on
  their shape (the new `hit` key) and rounded windows.
- Popamp's own skins other than the default one live in repositories of
  their own, each with the script that generates it. A skin folder skips
  hidden files, so a skin cloned into the skins folder with git works and
  a commit in it does not reload it.
- The default skin is a vector skin folder, `skins/default/` (SVG files and
  `skin.toml`), built into the program; it replaces the skin drawn in code.
  Vector skins take any sheet they leave out from it.
- `.psz`: Popamp's vector skin archive. Opening one installs it, like
  `.wsz`, and it has its own MIME type.
- `.deb` and AppImage packages for x86_64 and ARM64, built by a release
  workflow on version tags (`packaging/build-deb.sh`,
  `packaging/build-appimage.sh`), and AppStream metadata.
- rpm packages for openSUSE Tumbleweed and Leap 16.0 and Fedora 43 and
  44 (x86_64 and ARM64), built and published in a repository by the
  openSUSE Build Service (`packaging/obs/`), so they update with the
  system.
- An apt repository on GitHub Pages for Debian and Ubuntu, filled with
  each release's debs by the release workflow and signed
  (`packaging/apt/`), so Popamp updates with `apt upgrade`.
- `/RENDER=<dir>` (with optional `/SKIN=<file>` and files) draws every
  window in a set of fixed states to PNG files without a display or sound
  card, for skin previews and before/after comparisons. The set of states
  follows Ferramp's `--render`.
- Tooltips on the main window: the title bar buttons, the clutter bar and
  the song title explain themselves after the mouse rests on them; elsewhere
  the tooltip shows the program name.
- Skin cursors: the skin's `*.cur` files (including animated ones, first
  frame) replace the mouse pointer over the matching controls, as in the
  original. `usecursors=0` in `popamp.ini` turns them off.
- Bookmarks: `Alt+I` bookmarks the playing track (in the playlist: the
  selected ones), the main menu's Bookmarks submenu plays or removes them
  (`Ctrl+Alt+I`). They are kept in `bookmarks.m3u8` next to `popamp.ini`.
  Remote command ids 50021–50023.
- Equalizer Presets > Load/Save > Default: keep your own default curve and
  return to it in one click (ids 50024/50025).
- Playlist MISC > Misc options > Generate HTML playlist… saves the playlist
  as a web page with titles, lengths and the total (id 50026). The general
  options stay reachable there as a submenu.
- Popamp's own dialog windows, drawn like its menus and no longer needing
  `zenity`:
  - Jump to file (`J`) filters the playlist as you type; the arrows pick,
    Enter plays.
  - File info (`Alt+3`) lists tags and format of the selected track.
  - Preferences (`Ctrl+P`, previously the options menu): window behaviour,
    playlist display and auto-advance take effect at once; playlist font
    size, title format, fade-out, output buffer and HTTP proxy apply on OK.
- Skin Museum (main menu > Skins, id 50027): browse or search the Winamp
  Skin Museum at skins.webamp.org with previews, then download a skin into
  the skins folder and use it. Downloads are checked to be skins before
  they are saved; the HTTP proxy setting applies.

## [0.1.0] - 2026-10-22

First release.

### Added

- **Classic interface:**
  - Main, Equalizer and Playlist Editor windows, drawn from classic skins at
    1x or double size.
  - Docking and snapping, windowshade mode, EasyMove, always-on-top and
    `region.txt` window shapes.
- **Skins:**
  - Classic `.wsz`/`.zip`/folder skins with per-file fallback.
  - `viscolor.txt` and `pledit.txt` support.
  - An original procedurally drawn default skin.
- **Playback:**
  - MP3, AAC/M4A, FLAC, Ogg Vorbis, WAV, AIFF, MKA and ALAC through
    Symphonia, with ALSA output loaded at runtime.
  - Gapless playback, stop with fadeout, stop after current, ReplayGain.
  - HTTP(S)/Icecast/SHOUTcast streams with ICY titles.
  - YouTube videos, playlists and channels through `yt-dlp` and `ffmpeg`.
- **Equalizer:** 10 bands with Winamp or ISO band sets, preamp and
  limiter. `.eqf`/`.q1` preset import/export and per-file auto-load.
- **Playlist:**
  - M3U, M3U8, PLS and XSPF.
  - Sorting, shuffle, repeat, manual advance, drag-reorder, jump to
    file/time.
  - Advanced Title Formatting.
- **Visualization:** classic spectrum analyzer and oscilloscope.
- **Control:**
  - Winamp keyboard shortcuts and menus.
  - Single-instance remote control with Winamp-style `/SWITCH` options and
    command ids.
  - MPRIS2 D-Bus interface.
- **Packaging:** desktop entry, `.wsz` MIME type, icons and a per-user
  install script.

[Unreleased]: https://github.com/popamp/popamp/compare/v1.2.0...HEAD
[1.2.0]: https://github.com/popamp/popamp/compare/v1.1.0...v1.2.0
[1.1.0]: https://github.com/popamp/popamp/compare/v1.0.4...v1.1.0
[1.0.4]: https://github.com/popamp/popamp/compare/v1.0.3...v1.0.4
[1.0.3]: https://github.com/popamp/popamp/compare/v1.0.2...v1.0.3
[1.0.2]: https://github.com/popamp/popamp/compare/v1.0.1...v1.0.2
[1.0.1]: https://github.com/popamp/popamp/compare/v1.0.0...v1.0.1
[1.0.0]: https://github.com/popamp/popamp/compare/v0.9.1...v1.0.0
[0.9.1]: https://github.com/popamp/popamp/compare/v0.9.0...v0.9.1
[0.9.0]: https://github.com/popamp/popamp/compare/v0.8.0...v0.9.0
[0.8.0]: https://github.com/popamp/popamp/compare/v0.7.0...v0.8.0
[0.7.0]: https://github.com/popamp/popamp/compare/v0.6.0...v0.7.0
[0.6.0]: https://github.com/popamp/popamp/compare/v0.5.0...v0.6.0
[0.5.0]: https://github.com/popamp/popamp/releases/tag/v0.5.0
[0.1.0]: https://github.com/sayoojkkarun/linamp/releases/tag/v0.1.0
