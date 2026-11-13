# Features


- **Classic windows:** Main, Equalizer and Playlist Editor at any size from
  x1 to x3 in steps of 0.25 (`Ctrl+D` switches between x1 and the chosen
  size). Classic skins stay crisp at fractional sizes. Windows dock and snap together, and
  support windowshade mode, EasyMove, always-on-top and `region.txt` shapes.
  Tooltips explain the main window's buttons.
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
  [Monkey's Audio](https://github.com/popamp/plugin-ape),
  each allowed by you in Options > Plug-ins…. In a build with
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
  peaks) and oscilloscope (dots, lines, solid).
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
  - Global hotkeys, as in Winamp (off by default, Preferences > Desktop):
    Ctrl+Alt+Insert play, Home pause, End stop, Page Up/Down previous and
    next, Up/Down volume, Left/Right 5 seconds back and forward. On X11
    and Windows; under Wayland through the desktop's GlobalShortcuts
    portal (KDE Plasma 6, GNOME 48, Hyprland), which asks the user once
    and lets them pick other keys. GNOME keeps Ctrl+Alt+Left/Right for its
    workspaces.
  - Bar skins reserve their strip of the screen, like a taskbar.

## Known limitations

- As a Wayland program (by itself where there is no Xwayland, or with
  `wayland=1` in `popamp.ini`) Popamp has no always on top, no bar view
  and no file drops, does not remember where on the screen its windows
  were, and a menu opened at the screen's edge can go past it: Wayland
  tells a program neither where its windows are nor lets it place them.
  The equalizer and the playlist always move with the main window there.
  Through Xwayland, which stays the default where it runs, all of that
  works.
- The Preferences window (`Ctrl+P`) covers the common options; the rest are
  in the menus and `popamp.ini`.
- Tags can be changed in MP3, FLAC, Ogg Vorbis, Opus and MP4 files only, one
  file at a time.
- No media library or Modern (`.wal`) skins yet. See [ROADMAP.md](ROADMAP.md).

