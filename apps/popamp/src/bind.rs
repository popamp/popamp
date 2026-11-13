//! What the names in a skin layout mean in the player: the flags, values,
//! texts and menus that elements show or change.

use crate::menu::MenuItem;
use crate::model::Model;
use crate::scale::Scale;
use popamp_audio::PlayState;
use popamp_core::text::format_duration_ms;
use popamp_core::Command;
use popamp_skin::layout::{Cond, Dim, Flag, MenuRef, Placement, Text, Value, View, WindowKey};

pub fn flag(model: &Model, f: Flag) -> bool {
    let s = model.settings();
    let sh = model.engine.shared();
    let state = sh.state();
    let running = state != PlayState::Stopped;
    match f {
        Flag::Playing => state == PlayState::Playing,
        Flag::Paused => state == PlayState::Paused,
        Flag::Stopped => !running,
        Flag::Seekable => running && sh.length_ms().is_some_and(|l| l > 0),
        Flag::Stereo => running && sh.channels() >= 2,
        Flag::Mono => running && sh.channels() < 2,
        Flag::Shuffle => s.shuffle,
        Flag::Repeat => s.repeat,
        Flag::EqWindow => s.eq_open,
        Flag::PlaylistWindow => s.pe_open,
        Flag::LibraryWindow => s.ml_open,
        Flag::AlwaysOnTop => s.aot,
        Flag::DoubleSize => s.dsize,
        Flag::EqOn => s.use_eq,
        Flag::EqAutoload => s.autoload_eq,
    }
}

pub fn holds(model: &Model, c: Cond, width: i32) -> bool {
    match c {
        Cond::Always => true,
        Cond::If(f) => flag(model, f),
        Cond::Unless(f) => !flag(model, f),
        Cond::MinWidth(w) => width >= w,
    }
}

/// A value with its range, or `None` while it has none (position of a
/// stream of unknown length).
pub fn value(model: &Model, v: Value) -> Option<(i64, i64, i64)> {
    let s = model.settings();
    Some(match v {
        Value::Volume => (s.volume as i64, 0, 255),
        Value::Balance => (s.balance as i64, -127, 127),
        Value::Position => {
            let sh = model.engine.shared();
            let len = sh.length_ms().filter(|&l| l > 0)? as i64;
            (model.engine.position_ms() as i64, 0, len)
        }
        Value::Preamp => (s.preamp as i64, 0, 63),
        Value::EqBand(i) => (s.eq_bands()[i as usize] as i64, 0, 63),
    })
}

pub fn set_value(model: &mut Model, v: Value, x: i64) {
    match v {
        Value::Volume => model.set_volume(x as i32),
        Value::Balance => model.set_balance(x as i32),
        Value::Position => model.seek_to(x.max(0) as u64),
        Value::Preamp => model.set_preamp(x as i32),
        Value::EqBand(i) => model.set_eq_band(i as usize, x as i32),
    }
}

/// The text to show, or `None` to leave the background as it is.
pub fn text(model: &Model, t: Text) -> Option<String> {
    let sh = model.engine.shared();
    let running = sh.state() != PlayState::Stopped;
    match t {
        Text::Kbps => Some(sh.bitrate_kbps()).filter(|&k| running && k > 0).map(|k| format!("{:>3}", k.min(999))),
        Text::Khz => Some(sh.sample_rate()).filter(|&r| running && r > 0).map(|r| format!("{:>2}", (r / 1000).min(99))),
        Text::ShortTime => running.then(|| {
            let secs = model.engine.position_ms() / 1000;
            format!("{:>2}:{:02}", (secs / 60).min(99), secs % 60)
        }),
        Text::Title => Some(model.current_title()),
        Text::PlaylistTime => {
            let (sel_ms, total_ms, unknown) = model.playlist.durations();
            let plus = if unknown { "+" } else { "" };
            Some(format!("{}/{}{}", format_duration_ms(sel_ms), format_duration_ms(total_ms), plus))
        }
        Text::PlaylistClock => running.then(|| {
            let pos = model.engine.position_ms();
            let shown = match (model.settings().timeleftmode, sh.length_ms()) {
                (true, Some(l)) => format!("-{}", format_duration_ms(l.saturating_sub(pos))),
                _ => format_duration_ms(pos),
            };
            format!("{shown:>6}")
        }),
        Text::LibraryTitle => Some("MEDIA LIBRARY".into()),
        Text::LibraryStatus => Some(model.library.status().to_uppercase()),
    }
}

pub fn menu(model: &Model, m: MenuRef) -> Vec<MenuItem> {
    use Command as C;
    let s = model.settings();
    match m {
        MenuRef::Main => model.main_menu(),
        MenuRef::Options => model.options_menu(),
        MenuRef::Vis => model.vis_menu(),
        MenuRef::EqPresets => model.eq_presets_menu(),
        MenuRef::PlaylistRow => model.playlist_row_menu(),
        MenuRef::PlaylistMisc => model.playlist_misc_menu(),
        MenuRef::PlaylistSort => model.sort_menu(),
        MenuRef::Library => model.library_menu(),
        MenuRef::PlaylistRemoveMisc => vec![
            MenuItem::cmd("Remove missing files", "Alt+Del", C::RemoveMissing),
            MenuItem::cmd("Physically remove selected file(s)…", "", C::PhysicallyRemove),
        ],
        MenuRef::Previous => vec![
            MenuItem::cmd("Previous", "Click", C::Previous),
            MenuItem::cmd("Start of list", "Ctrl+Click", C::StartOfList),
            MenuItem::cmd("Rewind 5 seconds", "Shift+Click", C::Back5s),
        ],
        MenuRef::Play => vec![
            MenuItem::cmd("Play/restart", "Click", C::Play),
            MenuItem::cmd("Open URL…", "Ctrl+Click", C::OpenUrl),
            MenuItem::cmd("Open file…", "Shift+Click", C::OpenFile),
        ],
        MenuRef::Pause => vec![MenuItem::cmd("Pause/Unpause", "Click", C::Pause)],
        MenuRef::Stop => vec![
            MenuItem::cmd("Stop", "Click", C::Stop),
            MenuItem::cmd("Stop w/ fadeout", "Shift+Click", C::StopFadeout),
            MenuItem::cmd("Stop after current", "Ctrl+Click", C::StopAfterCurrent).checked(model.stop_after_current),
        ],
        MenuRef::Next => vec![
            MenuItem::cmd("Next", "Click", C::Next),
            MenuItem::cmd("Fast-forward 5 seconds", "Shift+Click", C::Fwd5s),
            MenuItem::cmd("End of list", "Ctrl+Click", C::EndOfList),
        ],
        MenuRef::Eject => vec![
            MenuItem::cmd("Open file…", "Click", C::OpenFile),
            MenuItem::cmd("Open folder…", "Shift+Click", C::OpenFolder),
            MenuItem::cmd("Open URL…", "Ctrl+Click", C::OpenUrl),
        ],
        MenuRef::Seek => vec![
            MenuItem::cmd("Jump to time…", "Ctrl+J", C::JumpToTime),
            MenuItem::cmd("Rewind 5 seconds", "Left", C::Back5s),
            MenuItem::cmd("Forward 5 seconds", "Right", C::Fwd5s),
        ],
        MenuRef::Repeat => vec![
            MenuItem::cmd("Repeat", "R", C::ToggleRepeat).checked(s.repeat),
            MenuItem::cmd("Manual playlist advance", "Shift+R", C::ToggleManualAdvance).checked(!s.pladv),
        ],
        MenuRef::Shuffle => vec![MenuItem::cmd("Shuffle", "S", C::ToggleShuffle).checked(s.shuffle)],
        MenuRef::Time => vec![
            MenuItem::cmd("Time elapsed", "Ctrl+T", C::TimeElapsed).checked(!s.timeleftmode),
            MenuItem::cmd("Time remaining", "Ctrl+T", C::TimeRemaining).checked(s.timeleftmode),
        ],
        MenuRef::SongTitle => vec![
            MenuItem::cmd("View file info…", "Alt+3", C::FileInfo),
            MenuItem::cmd("Jump to file…", "J", C::JumpToFile),
            MenuItem::cmd("Jump to time…", "Ctrl+J", C::JumpToTime),
            MenuItem::cmd("Autoscroll song name", "", C::ToggleAutoscroll).checked(s.autoscrollname),
        ],
    }
}

/// The look the window has on screen.
pub fn view(model: &Model, key: WindowKey) -> View {
    let as_bar = key == WindowKey::Main && model.settings().main_bar && !model.no_bar;
    model.skin.layout.window(key).map_or(View::Normal, |d| d.view(as_bar, shaded(model, key)))
}

/// Where the window goes, for the look it has now.
pub fn placement(model: &Model, key: WindowKey) -> Placement {
    match model.skin.layout.window(key).map_or(Placement::Docked, |d| d.placement_in(view(model, key))) {
        Placement::Bar(_) if model.no_bar => Placement::Free,
        placement => placement,
    }
}

/// Whether the window shows its shade mode.
pub fn shaded(model: &Model, key: WindowKey) -> bool {
    let s = model.settings();
    match key {
        WindowKey::Main => s.windowshade,
        WindowKey::Eq => s.eq_ws,
        WindowKey::Playlist => s.pe_ws,
        WindowKey::Library => false,
    }
}

pub fn title(key: WindowKey) -> &'static str {
    match key {
        WindowKey::Main => "Popamp",
        WindowKey::Eq => "Popamp Equalizer",
        WindowKey::Playlist => "Popamp Playlist Editor",
        WindowKey::Library => "Popamp Media Library",
    }
}

/// Whether the window is on screen: the skin has it and the user opened it.
pub fn shown(model: &Model, key: WindowKey) -> bool {
    let s = model.settings();
    let open = match key {
        WindowKey::Main => true,
        WindowKey::Eq => s.eq_open,
        WindowKey::Playlist => s.pe_open,
        WindowKey::Library => s.ml_open,
    };
    open && model.skin.layout.window(key).is_some()
}

pub fn position(model: &Model, key: WindowKey) -> (i32, i32) {
    let s = model.settings();
    match key {
        WindowKey::Main => (s.wx, s.wy),
        WindowKey::Eq => (s.eq_wx, s.eq_wy),
        WindowKey::Playlist => (s.pe_wx, s.pe_wy),
        WindowKey::Library => (s.ml_wx, s.ml_wy),
    }
}

pub fn set_position(model: &mut Model, key: WindowKey, p: (i32, i32)) {
    let s = model.settings_mut();
    match key {
        WindowKey::Main => (s.wx, s.wy) = p,
        WindowKey::Eq => (s.eq_wx, s.eq_wy) = p,
        WindowKey::Playlist => (s.pe_wx, s.pe_wy) = p,
        WindowKey::Library => (s.ml_wx, s.ml_wy) = p,
    }
}

/// The window's stored size, for resizable dimensions.
pub fn stored_size(model: &Model, key: WindowKey) -> (i32, i32) {
    let s = model.settings();
    match key {
        WindowKey::Playlist => (s.pe_width, s.pe_height),
        WindowKey::Library => (s.ml_width, s.ml_height),
        // A bar covers its monitor's width.
        WindowKey::Main => (model.screen.map_or(0, |m| scale(model, key).skin_up(m.w)), 0),
        _ => (0, 0),
    }
}

/// Resizes a window to its size at the start of a drag plus the drag, in
/// the steps its layout allows; returns true if the size changed.
pub fn resize(model: &mut Model, key: WindowKey, start: (i32, i32), dx: i32, dy: i32) -> bool {
    let Some(def) = model.skin.layout.window(key) else { return false };
    let size = (def.normal.width.snap(start.0 + dx), def.normal.height.snap(start.1 + dy));
    if size == stored_size(model, key) {
        return false;
    }
    let s = model.settings_mut();
    match key {
        WindowKey::Playlist => (s.pe_width, s.pe_height) = size,
        WindowKey::Library => (s.ml_width, s.ml_height) = size,
        _ => return false,
    }
    true
}

/// The scale a window is shown at: a bar's own size for a bar, the
/// window size for the rest.
pub fn scale(model: &Model, key: WindowKey) -> Scale {
    let s = model.settings();
    match key {
        WindowKey::Main if crate::bar::edge(model).is_some() => Scale::new(s.bar_zoom, s.wsz_scaling),
        _ => Scale::of(s),
    }
}

pub fn window_size(model: &Model, key: WindowKey) -> (i32, i32) {
    let Some(def) = model.skin.layout.window(key) else { return (0, 0) };
    let mode = def.mode(view(model, key));
    let (sw, sh) = stored_size(model, key);
    let len = |d: Dim, stored: i32| match d {
        Dim::Fixed(n) => n,
        Dim::Free { min, .. } => stored.max(min),
    };
    (len(mode.width, sw), len(mode.height, sh))
}

#[cfg(test)]
mod tests {
    use super::*;
    use popamp_core::config::BarEdge;

    #[test]
    fn sizes_come_from_the_layout_and_settings() {
        let mut m = Model::for_tests();
        assert_eq!(window_size(&m, WindowKey::Main), (275, 116));
        m.settings_mut().windowshade = true;
        assert_eq!(window_size(&m, WindowKey::Main), (275, 14));
        m.settings_mut().pe_width = 450;
        m.settings_mut().pe_height = 290;
        assert_eq!(window_size(&m, WindowKey::Playlist), (450, 290));
        m.settings_mut().pe_ws = true;
        assert_eq!(window_size(&m, WindowKey::Playlist), (450, 14));
    }

    #[test]
    fn a_bar_is_as_wide_as_its_monitor_at_the_bar_size() {
        let mut m = Model::for_tests();
        m.skin.layout.windows[0].placement = Placement::Bar(BarEdge::Bottom);
        m.skin.layout.windows[0].normal.width = Dim::Free { min: 400, step: 1 };
        assert_eq!(window_size(&m, WindowKey::Main).0, 400, "no monitor yet: the narrowest");
        m.screen = Some(crate::bar::Screen { x: 0, y: 0, w: 1920, h: 1080 });
        m.settings_mut().bar_zoom = 100;
        assert_eq!(window_size(&m, WindowKey::Main).0, 1920);
        m.settings_mut().bar_zoom = 150;
        assert_eq!(window_size(&m, WindowKey::Main).0, 1280);
        m.settings_mut().bar_zoom = 175;
        assert_eq!(window_size(&m, WindowKey::Main).0, 1098, "covers the monitor: 1097 would leave a pixel");
    }

    #[test]
    fn a_bar_has_its_own_size_whatever_size_the_windows_are() {
        let mut m = Model::for_tests();
        assert_eq!(scale(&m, WindowKey::Main).px(14), 14, "no bar: the window size");
        m.skin.layout.windows[0].placement = Placement::Bar(BarEdge::Bottom);
        assert_eq!(scale(&m, WindowKey::Main).px(14), 28, "as thick as jetAudio's bar");
        m.settings_mut().dsize = true;
        m.settings_mut().zoom = 300;
        assert_eq!(scale(&m, WindowKey::Main).px(14), 28);
        assert_eq!(scale(&m, WindowKey::Eq).px(14), 42, "the other windows follow the window size");
        m.settings_mut().bar_zoom = 150;
        assert_eq!(scale(&m, WindowKey::Main).px(14), 21);
    }

    #[test]
    fn windows_show_when_open_and_in_the_skin() {
        let mut m = Model::for_tests();
        m.settings_mut().eq_open = true;
        m.settings_mut().pe_open = false;
        assert!(shown(&m, WindowKey::Main) && shown(&m, WindowKey::Eq) && !shown(&m, WindowKey::Playlist));
        m.skin.layout.windows.retain(|w| w.key == WindowKey::Main);
        assert!(!shown(&m, WindowKey::Eq), "a skin without an equalizer");
        assert_eq!(window_size(&m, WindowKey::Eq), (0, 0));
    }

    #[test]
    fn the_library_window_opens_on_its_command_and_resizes_in_steps() {
        let mut m = Model::for_tests();
        assert!(!shown(&m, WindowKey::Library), "closed until asked for");
        m.exec(Command::ToggleLibraryWindow);
        assert!(shown(&m, WindowKey::Library) && flag(&m, Flag::LibraryWindow));
        let start = window_size(&m, WindowKey::Library);
        assert!(resize(&mut m, WindowKey::Library, start, 30, 0));
        assert_eq!(window_size(&m, WindowKey::Library), (start.0 + 25, start.1), "in the frame's steps");
        assert_eq!(text(&m, Text::LibraryStatus).as_deref(), Some("0 TRACKS"));
        assert!(menu(&m, MenuRef::Library).len() >= 4);
    }

    #[test]
    fn stopped_player_shows_no_play_details() {
        let m = Model::for_tests();
        assert!(flag(&m, Flag::Stopped) && !flag(&m, Flag::Seekable) && !flag(&m, Flag::Stereo) && !flag(&m, Flag::Mono));
        assert_eq!(text(&m, Text::Kbps), None);
        assert_eq!(text(&m, Text::ShortTime), None);
        assert_eq!(value(&m, Value::Position), None);
        assert_eq!(value(&m, Value::Balance), Some((0, -127, 127)));
    }
}
