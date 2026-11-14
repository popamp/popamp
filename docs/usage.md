# Usage

## Keys and mouse

| Action | How |
|---|---|
| Open files | `L`, the eject button, or drag files onto a window |
| Open a URL / stream / YouTube link | `Ctrl+L`, or `popamp <url>` |
| Transport | `Z X C V B` (prev, play, pause, stop, next) |
| Seek / volume | arrow keys, or drag the bars; mouse wheel changes volume |
| Main menu | click the top-left button, right-click, or `Alt+F` |
| Skins | `Alt+S`, or put `.wsz`/`.psz` files in the skins folder (below) |
| Equalizer keys | `1`–`0` / `Q`–`P` raise/lower bands, `N` on/off, `S` presets |
| Tags and cover / album art | `Alt+3` (File info) / `Alt+A` |
| Double size / shade | `Ctrl+D` / `Ctrl+W` (or double-click the title bar) |
| Media library | `Alt+L`, or the main menu |

In the media library window, typing searches (`Esc` clears it), `Tab` /
`Shift+Tab` go between the artists, the albums and the tracks, the arrows,
`PgUp`/`PgDn` and `Home`/`End` move in that list (`Shift` takes a range of
tracks), `Enter` plays and `Shift+Enter` adds to the playlist, `Ctrl+A`
selects all the tracks. A double click on a track, an artist or an album
plays it.

## Remote control

The same program sends commands to the running instance (`popamp.exe` on
Windows):

```sh
popamp /ADD song.flac      # enqueue
popamp /PLAYPAUSE
popamp /VOL=40 /SHUFFLE=1
popamp /STATUS             # JSON status
popamp /NEW                # start a second, independent instance
popamp /HELP               # all options
```

## Folders

| | Linux | Windows |
|---|---|---|
| Settings, playlist, bookmarks | `~/.config/popamp/` | `%APPDATA%\popamp\` |
| Skins, plug-ins | `~/.local/share/popamp/skins/`, `plugins/` | `%APPDATA%\popamp\skins\`, `plugins\` |
| Skins, plug-ins from packages | `/usr/share/popamp/skins/`, `plugins/` (each of `$XDG_DATA_DIRS`) | none |

Settings are in `popamp.ini`, whose key names follow `winamp.ini`; the
playlist is saved to `popamp.m3u8`. A `popamp.ini` next to the program
keeps everything in the program's folder (portable mode), and
`/INIDIR=<dir>` uses another folder for one run.

Skins and plug-ins installed as packages (`popamp-skin-<name>` with apt,
zypper or dnf)
are read from the system folders and removed with the package manager.
A skin or plug-in folder of the same name in your own folder wins over
the packaged one. Portable mode and `/INIDIR=` read your folder only.

## YouTube

YouTube is a plug-in, [plugin-youtube](https://github.com/popamp/plugin-youtube),
so it needs a build with plug-ins (`--features plugins`) and the plug-in
installed; its README says how. It plays videos, playlists and channels
through `yt-dlp` and `ffmpeg`. Without it a YouTube link fails with "no
plug-in plays this link".

The settings `ytdlp_path`, `ffmpeg_path` and `ytdlp_format` of earlier
versions move to the plug-in's settings (`[plugin.youtube]` in
`popamp.ini`) the first time the new version starts.

