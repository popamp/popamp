# Roadmap

What is left before 1.0, split into versions of about the same size.
Contributions are welcome. Please open an issue first for larger items.

## 0.9: Sound and desktop

- **More formats:** Opus (libopus), MIDI (SoundFont synth), tracker modules
  (libopenmpt), CUE sheets.
- **Decoder plug-ins.**
- **Playback extras:** crossfade, a ReplayGain scanner, history (Ctrl+H).
- **Desktop integration:** tray icon (StatusNotifierItem), track-change
  notifications, global hotkeys through the XDG portal.

## 1.0: The whole player

- **Native Wayland mode:** done; the equalizer, the playlist and menus are
  parts of the main window there, since Wayland does not let apps
  position their own windows. Left: file drops, and menus kept on the
  screen (needs winit to give out the window's `xdg_surface`).
- **Local media library:** folder scanning, search, library playlists.
- **Modern skins:** Wasabi XML layout and the MAKI script VM.
- **Hands-on testing before the release:** mouse interaction, docking,
  menus, dialogs, drag & drop and double size across desktops (GNOME,
  KDE, Xfce, i3), distributions, and Windows 10 and 11.

## Later

- **MilkDrop-style visualizers** of our own, as visual plug-ins.
- **MilkDrop presets:** a `plugin-milkdrop` visual plug-in that reads
  `.milk` presets the user downloads and draws them as scenes, starting
  with MilkDrop 1 presets. The plug-in API needs a preset folder to read,
  scene changes while running, textures and keys for it.
