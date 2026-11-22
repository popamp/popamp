# Features


- **Classic windows:** Main, Equalizer and Playlist Editor at any size from
  x1 to x3 in steps of 0.25 (`Ctrl+D` switches between x1 and the chosen
  size). Classic skins stay crisp at fractional sizes. Windows dock and snap together, and
  support windowshade mode, EasyMove, always-on-top and `region.txt` shapes.
  With *Move docked windows together* (Options), the equalizer or the
  playlist drags the whole docked group by its title bar.
  Tooltips explain the main window's buttons.
- **Media library** (`Alt+L`): a window of its own, docking with the
  others, for the folders you give it. It lists all the music, the newest
  tracks and each folder; artists and albums side by side, each with its
  number of albums or year and tracks, narrow the track table under them,
  and typing searches every tag and the file path. Play replaces the
  playlist, Enqueue adds to it; Enter and a double click enqueue, or play
  when "Enter and double-click enqueue" is off in Preferences > Media
  Library, and Shift+Enter does the other. Tracks dragged onto the
  playlist go in where they are let go. The folders are read again in the
  background at start (and on Rescan); only changed files are read anew,
  and the audio files themselves are never written. Add folder takes
  several folders picked together.
- **Plays, ratings and playlists in the library:** it counts the plays
  of each track and keeps its 0 to 5 stars, in its own `stats.txt`.
  Smart playlists pick tracks by rules (tags, plays, rating, when added
  or last played), made in an editor and kept in `smart.toml`; plain
  playlists are made by hand and kept as M3U8 files.
- **Own dialogs:** Jump to file (search as you type), File info, the Skin
  browser (`Alt+S`) and a Preferences window (`Ctrl+P`). They and the menus
  follow the display scaling (Windows' scaling setting, `Xft.dpi` on X11),
  not the size of the player's windows, with sharp text. Opening and saving
  files uses the desktop's dialog: `zenity` or `kdialog` on Linux, the
  system's own on Windows.
- **Vector skins** (`.psz`): drawn in SVG, sharp at every size, with TrueType text
  and live reload while you edit them. Popamp's default skin is one. See
  [skin-format.md](skin-format.md).
- **Classic skins:** `.wsz`/`.zip` archives or unpacked directories. Each file
  falls back to the default skin if missing or broken. `viscolor.txt`,
  `pledit.txt` and the skin's mouse cursors (`*.cur`) are supported.
  Skins > Skin Museum browses, searches and downloads skins from
  [skins.webamp.org](https://skins.webamp.org); Popamp contacts the
  museum only while that window is open.
- **Gallery:** Skins > Gallery (also under Options) lists the skins,
  plug-ins and visualizations on [popamp.app](https://popamp.app), each
  on a tab of its own, by popularity, rating or date as on the site,
  with a preview, the rating and downloads, and what each plug-in may
  do, and installs or updates one in a click. Each
  download is checked against the catalog's size and SHA-256; what the
  gallery did not install itself is left alone. In the build without
  plug-ins only the Skins tab lists anything. Popamp contacts the site only while
  that window is open.
- **Update check:** at start, at most once a day, Popamp asks
  [popamp.app](https://popamp.app) for the latest version (the request
  carries only Popamp's version) and, when a newer one is out, offers
  its page at the top of the main menu, or says to update through the
  package manager when Popamp came from a .deb or .rpm. It never
  downloads or installs anything itself. Off in Preferences > Internet.
- **Tags and covers:** File info (`Alt+3`) changes the title, artist,
  album, album artist, year, genre, track and disc numbers, composer and
  comment of MP3, FLAC, Ogg Vorbis, Opus and MP4/M4A files, and sets or removes
  the cover. Tags it does not show are kept. The change goes into a copy
  that then replaces the file, so a failed save leaves the file as it was.
  The Album art window (`Alt+A`) shows the cover of the current track: the
  one in its tags, else a `cover`/`folder`/`front` picture beside it.
- **Playback:**
  - Formats: MP3, AAC/M4A, FLAC, Ogg Vorbis, WAV, AIFF, MKA (Matroska audio)
    and ALAC, decoded by [Symphonia](https://github.com/pdeljanov/Symphonia);
    Opus (in Ogg, Matroska and WebM) through the system's libopus; MIDI
    (`.mid`, `.midi`, `.kar`, `.rmi`) through a SoundFont synthesizer
    ([rustysynth](https://github.com/sinshu/rustysynth)), with the
    system's General MIDI SoundFont (on Windows, GeneralUser GS, which
    comes with Popamp) or the one set in Preferences; tracker
    modules (MOD, XM, S3M, IT, MPTM and more) through the system's
    libopenmpt.
  - Gapless playback, crossfade between tracks, stop
    with fadeout, stop after current.
  - ReplayGain from tags, or measured by Popamp's own scanner (EBU R128,
    track and album gain: select tracks, then *Scan ReplayGain* in the
    playlist's menu). Scanned values are kept in `replaygain.txt` in the
    settings folder; Popamp never writes them into the files.
  - History (Ctrl+H): the last 5000 plays, searchable, kept in
    `history.txt`.
  - HTTP(S)/Icecast/SHOUTcast streams with ICY titles.
  - YouTube videos, playlists and channels through the
    [YouTube plug-in](https://github.com/popamp/plugin-youtube).
- **Equalizer:** 10 bands plus preamp, with either the Winamp or the ISO
  band set and an optional limiter. Reads and writes `.eqf` and `.q1`
  presets, and can auto-load presets per file.
- **Plug-ins** (in a build with `--features plugins`): WebAssembly
  sources such as [YouTube](https://github.com/popamp/plugin-youtube),
  sound effects such as [Crossfeed](https://github.com/popamp/plugin-crossfeed),
  and decoders of more formats such as
  [Monkey's Audio](https://github.com/popamp/plugin-ape) and
  [WavPack](https://github.com/popamp/plugin-wavpack),
  each allowed by you in Preferences > Plug-ins. In a build with
  `--features visualizers`, also GPU visualizers such as
  [Spectrum](https://github.com/popamp/plugin-spectrum), in a window that
  can go full screen. See [plugins.md](plugins.md).
- **Playlist:**
  - M3U, M3U8, PLS and XSPF.
  - CUE sheets: an album in one file opens as its tracks, each with its
    title, performer and length, and seeking stays inside the track.
  - Sort, reverse, randomize, crop, remove dead entries.
  - Drag to reorder, jump to time.
  - Bookmarks for favourite files and streams (`Alt+I`, main menu).
  - Export as an HTML page (MISC > Misc options).
  - Advanced Title Formatting (`[%artist% - ]$if2(%title%,…)`).
- **Visualization:** classic spectrum analyzer (normal, fire and line styles,
  peaks) and oscilloscope (dots, lines, solid, and lines that fade out over five
  frames, like a cathode-ray tube's afterglow).
- **Control:**
  - Winamp keyboard shortcuts and Winamp-style menus.
  - Command line with `/PLAY /NEXT /VOL=50 /COMMAND=40045 …` sent to the
    running instance.
  - Media keys and the desktop's player controls, with the track's cover:
    MPRIS2 on Linux (desktop widgets, `playerctl`), the media overlay on
    Windows.
  - A tray icon, on Windows and on Linux (StatusNotifierItem: KDE, Xfce,
    GNOME with the AppIndicator extension): a click brings Popamp up, a
    middle click plays or pauses, the wheel sets the volume (Linux), and
    its menu has the transport buttons. With "Minimize to the tray",
    minimizing hides the windows and shows the icon until they come back,
    even when the icon is otherwise off. On Linux, notifications of each
    new track with its cover, off by default. Both are in Preferences >
    Desktop.
  - Title-bar shortcuts (Preferences > Shortcuts): a right or middle click
    on the title bar of the main, equalizer or playlist window (full size
    or shaded) can go to the previous or next track, pause, stop or switch
    shuffle or repeat, or turn the volume up or down. The wheel over a
    title bar (Scroll Up, Scroll Down) works without a click. The rules
    are a table: pick the window, the mouse button or wheel direction and
    the action, then Assign; one action per click of a window.
  - Global hotkeys, as in Winamp (off by default, Preferences > Desktop):
    Ctrl+Alt+Insert play, Home pause, End stop, Page Up/Down previous and
    next, Up/Down volume, Left/Right 5 seconds back and forward. On X11
    and Windows; under Wayland through the desktop's GlobalShortcuts
    portal (KDE Plasma 6, GNOME 48, Hyprland), which asks the user once
    and lets them pick other keys. GNOME keeps Ctrl+Alt+Left/Right for its
    workspaces.
  - Toolbar mode: the main window as a bar along the screen's edge (see
    below).

## Toolbar mode

The main window can be a bar along the top or bottom edge of the screen,
the way jetAudio's bar does, on Linux as well as on Windows:

- **One strip with the whole player:** the transport buttons, the
  visualizer, the clock, the scrolling title, volume and the EQ, PL,
  shuffle and repeat switches, across the full width of the screen.
- **Room kept for it, like a taskbar.** On Linux (X11) Popamp reserves
  the strip with `_NET_WM_STRUT_PARTIAL`, so maximized windows stop at the
  bar instead of going under it, and it stays inside the panels the
  desktop already has at that edge (GNOME's top bar, a KDE or Xfce
  panel). KDE Plasma on Wayland ignores struts of the programs that run
  through Xwayland, so there Popamp holds the strip with an invisible
  layer of the desktop under the bar (wlr-layer-shell, on a Wayland
  connection of its own) while the bar stays an X11 window. On Windows
  it is an application bar, as the taskbar is. As a
  Wayland program it is a layer of the desktop (wlr-layer-shell, which
  COSMIC, KDE Plasma and the wlroots compositors such as Sway and
  Hyprland have), beside the desktop's own panels.
- **The other windows go along.** The playlist and the equalizer sit
  against the bar at its right end. Drag one away and it stays where it
  was put until the bar is turned off; the media library moves freely.
- **Main menu > Toolbar mode:** *On* / *Off*, *Position* (top or bottom of
  the screen) and *Size* (×1 to ×3, apart from the size of the other
  windows; only the sizes the bar fits across the screen at are offered,
  and on a smaller screen the bar takes the biggest of them). Position and
  size can be chosen before the bar is on.
- **Full width, under side panels** (off at first, not as a Wayland
  program): the bar spans the screen's whole width. Ubuntu's dock makes
  room for the bar but tells the system only of its full height, which
  leaves an empty corner beside the bar; a side panel that does not make
  room covers the bar's end. See [Ubuntu](ubuntu.md).
- **Skins:** a vector skin describes its bar beside its window
  (`[window.bar]` in `skin.toml`, see [skin-format.md](skin-format.md)),
  so one skin gives both, in all its colour sets. The built-in Default
  and the Modern, Steel, Classified and D11 skins have one. Classic
  `.wsz` skins have no bar.

As a Wayland program the bar takes no keys: use the mouse, the media
keys or the global hotkeys. GNOME has no layer shell, and there the bar
is not offered to a Wayland program; under Xwayland, the default there,
it works as on X11.

## Known limitations

- As a Wayland program (by itself where there is no Xwayland, on COSMIC,
  whose Xwayland does not let a program move its windows, or with
  `wayland=1` in `popamp.ini`) Popamp has no always on top and no file
  drops, does not remember where on the screen its windows
  were, and a menu opened at the screen's edge can go past it: Wayland
  tells a program neither where its windows are nor lets it place them.
  The equalizer and the playlist move with the main window there, but
  on COSMIC, which hides the ones not docked to it while it is dragged.
  On COSMIC they snap to the screen's edges and the desktop's panels, as
  Popamp learns from it where the main window is; the main window, which
  COSMIC moves, snaps to them with its *Edge gravity* (Settings →
  Desktop → Window management), and so does the group dragged by the
  equalizer or the playlist with *Move docked windows together*.
  Through Xwayland, which stays the default where it runs (but on
  COSMIC), all of that works.
  [wayland.md](wayland.md) explains each limit and why.
- The Preferences window (`Ctrl+P`) covers the common options, in sections
  listed down its left side; the rest are in the menus and `popamp.ini`.
- Tags can be changed in MP3, FLAC, Ogg Vorbis, Opus and MP4 files only, one
  file at a time.
- No Modern (`.wal`) skins yet. See
  [ROADMAP.md](ROADMAP.md).

