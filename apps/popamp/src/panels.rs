//! Popamp's own dialog windows: Jump to file, Preferences, the Skin
//! browser and the Skin Museum (File info and Album art are in `fileinfo.rs`, the Plug-ins
//! window in `plugins.rs`).
//! Each is a [`Form`] plus the logic that turns its events into changes to
//! the [`Model`]; the windows themselves live in `app.rs`.

use crate::fileinfo::{AlbumArt, FileInfo};
#[cfg(linux_like)]
use crate::filetypes::Group;
use crate::model::{Model, UiReq};
use crate::museum::{self, Fetched, Job};
use crate::widgets::{Event, Form, Rect, Widget, BUTTON, PAGE_H, ROW_H};
use popamp_audio::output::Device;
use popamp_core::config::Settings;
use popamp_core::Command;
use popamp_skin::Bitmap;
use std::collections::HashMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PanelKind {
    JumpToFile,
    FileInfo,
    Preferences,
    #[cfg(feature = "plugins")]
    Plugins,
    SkinMuseum,
    Gallery,
    AlbumArt,
    SkinBrowser,
    History,
    ReplayGainScan,
}

/// What a button or check box in a panel stands for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Act {
    Ok,
    Close,
    /// Picks a picture for the cover in File info.
    ChangeCover,
    RemoveCover,
    Flag(Flag),
    /// A section of Preferences was picked.
    Section,
    /// A file type check box, read on OK.
    #[cfg(linux_like)]
    FileType,
    /// Opens the system's default apps.
    #[cfg(windows)]
    DefaultApps,
    Search,
    More,
    /// Asks for a skin file to install.
    InstallSkin,
    /// Opens the Gallery.
    Gallery,
    /// The check box of the n-th plug-in.
    #[cfg(feature = "plugins")]
    Plugin(usize),
    /// A host of plug-in `.0`, by its place in the manifest.
    #[cfg(feature = "plugins")]
    PluginHost(usize, usize),
    #[cfg(feature = "plugins")]
    PluginJournal(usize),
}

pub enum Panel {
    Jump(JumpToFile),
    Info(FileInfo),
    Prefs(Preferences),
    #[cfg(feature = "plugins")]
    Plugins(crate::plugins::PluginsPanel),
    Museum(SkinMuseum),
    Gallery(crate::gallery::GalleryPanel),
    Art(AlbumArt),
    Skins(SkinBrowser),
    History(crate::history::HistoryPanel),
    Scan(crate::scan::ScanPanel),
}

impl Panel {
    pub fn open(kind: PanelKind, model: &Model) -> Panel {
        match kind {
            PanelKind::JumpToFile => Panel::Jump(JumpToFile::new(model)),
            PanelKind::FileInfo => Panel::Info(FileInfo::new(model)),
            PanelKind::Preferences => {
                Panel::Prefs(Preferences::new(model.settings(), popamp_audio::output::devices(), FileTypes::from_env()))
            }
            #[cfg(feature = "plugins")]
            PanelKind::Plugins => Panel::Plugins(crate::plugins::PluginsPanel::new(model)),
            PanelKind::SkinMuseum => Panel::Museum(SkinMuseum::new()),
            PanelKind::Gallery => Panel::Gallery(crate::gallery::GalleryPanel::new(model)),
            PanelKind::AlbumArt => Panel::Art(AlbumArt::new(model)),
            PanelKind::SkinBrowser => Panel::Skins(SkinBrowser::new(model)),
            PanelKind::History => Panel::History(crate::history::HistoryPanel::new(model)),
            PanelKind::ReplayGainScan => Panel::Scan(crate::scan::ScanPanel::new(model)),
        }
    }

    pub fn kind(&self) -> PanelKind {
        match self {
            Panel::Jump(_) => PanelKind::JumpToFile,
            Panel::Info(_) => PanelKind::FileInfo,
            Panel::Prefs(_) => PanelKind::Preferences,
            #[cfg(feature = "plugins")]
            Panel::Plugins(_) => PanelKind::Plugins,
            Panel::Museum(_) => PanelKind::SkinMuseum,
            Panel::Gallery(_) => PanelKind::Gallery,
            Panel::Art(_) => PanelKind::AlbumArt,
            Panel::Skins(_) => PanelKind::SkinBrowser,
            Panel::History(_) => PanelKind::History,
            Panel::Scan(_) => PanelKind::ReplayGainScan,
        }
    }

    pub fn title(&self) -> &'static str {
        match self {
            Panel::Jump(_) => "Jump to file",
            Panel::Info(_) => "File info",
            Panel::Prefs(_) => "Popamp Preferences",
            #[cfg(feature = "plugins")]
            Panel::Plugins(_) => "Plug-ins",
            Panel::Museum(_) => "Winamp Skin Museum",
            Panel::Gallery(_) => "Popamp Gallery",
            Panel::Art(_) => "Album art",
            Panel::Skins(_) => "Skin browser",
            Panel::History(_) => "History",
            Panel::Scan(_) => "ReplayGain scan",
        }
    }

    pub fn form(&self) -> &Form<Act> {
        match self {
            Panel::Jump(p) => &p.form,
            Panel::Info(p) => &p.form,
            Panel::Prefs(p) => &p.form,
            #[cfg(feature = "plugins")]
            Panel::Plugins(p) => &p.form,
            Panel::Museum(p) => &p.form,
            Panel::Gallery(p) => &p.form,
            Panel::Art(p) => &p.form,
            Panel::Skins(p) => &p.form,
            Panel::History(p) => &p.form,
            Panel::Scan(p) => &p.form,
        }
    }

    pub fn form_mut(&mut self) -> &mut Form<Act> {
        match self {
            Panel::Jump(p) => &mut p.form,
            Panel::Info(p) => &mut p.form,
            Panel::Prefs(p) => &mut p.form,
            #[cfg(feature = "plugins")]
            Panel::Plugins(p) => &mut p.form,
            Panel::Museum(p) => &mut p.form,
            Panel::Gallery(p) => &mut p.form,
            Panel::Art(p) => &mut p.form,
            Panel::Skins(p) => &mut p.form,
            Panel::History(p) => &mut p.form,
            Panel::Scan(p) => &mut p.form,
        }
    }

    /// Acts on what happened in the form. Returns `false` when the panel
    /// should close.
    pub fn handle(&mut self, ev: Event<Act>, model: &mut Model) -> bool {
        match self {
            Panel::Jump(p) => p.handle(ev, model),
            Panel::Info(p) => p.handle(ev, model),
            Panel::Prefs(p) => p.handle(ev, model),
            #[cfg(feature = "plugins")]
            Panel::Plugins(p) => p.handle(ev, model),
            Panel::Museum(p) => p.handle(ev, model),
            Panel::Gallery(p) => p.handle(ev, model),
            Panel::Art(p) => p.handle(ev),
            Panel::Skins(p) => p.handle(ev, model),
            Panel::History(p) => p.handle(ev, model),
            Panel::Scan(p) => p.handle(ev, model),
        }
    }

    /// Network work the panel asked for since the last call.
    pub fn take_jobs(&mut self) -> Vec<Job> {
        match self {
            Panel::Museum(p) => std::mem::take(&mut p.jobs),
            _ => Vec::new(),
        }
    }

    /// Gallery work the panel asked for since the last call.
    pub fn take_gallery_jobs(&mut self) -> Vec<crate::gallery::Job> {
        match self {
            Panel::Gallery(p) => std::mem::take(&mut p.jobs),
            _ => Vec::new(),
        }
    }

    /// A gallery job finished.
    pub fn gallery_fetched(&mut self, f: crate::gallery::Fetched, model: &mut Model) {
        if let Panel::Gallery(p) = self {
            p.fetched(f, model);
        }
    }

    /// A background job finished.
    pub fn fetched(&mut self, f: Fetched, model: &mut Model) {
        if let Panel::Museum(p) = self {
            p.fetched(f, model);
        }
    }

    /// The playlist changed while the panel is open.
    pub fn playlist_changed(&mut self, model: &Model) {
        if let Panel::Jump(p) = self {
            p.refilter(model);
        }
    }

    /// The model may have moved to another track or changed tags. Returns
    /// whether the panel needs drawing again.
    pub fn refresh(&mut self, model: &Model) -> bool {
        match self {
            Panel::Art(p) => p.refresh(model),
            Panel::Skins(p) => p.refresh(model),
            Panel::History(p) => p.refresh(model),
            Panel::Scan(p) => p.refresh(model),
            _ => false,
        }
    }

    /// The panel's work is over and it closes by itself.
    pub fn wants_close(&self) -> bool {
        matches!(self, Panel::Scan(p) if p.wants_close())
    }

    /// The picture the user chose for a cover.
    pub fn cover_picked(&mut self, path: &std::path::Path) {
        if let Panel::Info(p) = self {
            p.cover_picked(path);
        }
    }
}

pub(crate) fn button(form: &mut Form<Act>, x: i32, y: i32, label: &str, action: Act, default: bool) {
    form.add(Rect::new(x, y, BUTTON.0, BUTTON.1), Widget::Button { label: label.into(), action, default });
}

// --- Jump to file ---------------------------------------------------------------

/// Playlist positions whose title contains every word of `query`, ignoring case.
pub fn search<'a>(titles: impl Iterator<Item = &'a str>, query: &str) -> Vec<usize> {
    let query = query.to_lowercase();
    let words: Vec<&str> = query.split_whitespace().collect();
    titles
        .enumerate()
        .filter(|(_, t)| {
            let t = t.to_lowercase();
            words.iter().all(|w| t.contains(w))
        })
        .map(|(i, _)| i)
        .collect()
}

/// Search the playlist as you type; Enter plays the selected entry.
pub struct JumpToFile {
    form: Form<Act>,
    search: usize,
    list: usize,
    /// The playlist position of each visible row.
    shown: Vec<usize>,
}

impl JumpToFile {
    fn new(model: &Model) -> JumpToFile {
        let (w, list_h) = (460, 4 + 16 * ROW_H);
        let mut form = Form::new(w, 10 + 24 + 8 + list_h + 10 + BUTTON.1 + 10);
        form.add(Rect::new(10, 10, 70, 24), Widget::Label { text: "Search for:".into(), dim: false });
        let search = form.add(Rect::new(80, 10, w - 90, 24), Widget::Input { text: String::new(), cursor: 0 });
        let list = form.add(Rect::new(10, 42, w - 20, list_h), Widget::List { rows: vec![], selected: None, scroll: 0 });
        let by = 42 + list_h + 10;
        button(&mut form, w - 2 * BUTTON.0 - 16, by, "Jump", Act::Ok, true);
        button(&mut form, w - BUTTON.0 - 10, by, "Close", Act::Close, false);
        let mut p = JumpToFile { form, search, list, shown: vec![] };
        p.refilter(model);
        p
    }

    /// Lists the entries matching the search, keeping the selected entry (or
    /// else the playing one) selected.
    fn refilter(&mut self, model: &Model) {
        let keep = self.form.selected(self.list).and_then(|r| self.shown.get(r).copied()).or(model.playlist.current());
        let entries = model.playlist.entries();
        self.shown = search(entries.iter().map(|e| e.display.as_str()), self.form.text(self.search));
        let rows = self.shown.iter().map(|&i| format!("{}. {}", i + 1, entries[i].display)).collect();
        let select = keep.and_then(|k| self.shown.iter().position(|&i| i == k)).unwrap_or(0);
        self.form.set_rows(self.list, rows, Some(select));
    }

    fn handle(&mut self, ev: Event<Act>, model: &mut Model) -> bool {
        match ev {
            Event::Edited => {
                // A new search starts from the best match, not the old selection.
                self.form.set_rows(self.list, vec![], None);
                self.refilter(model);
                true
            }
            Event::Clicked(Act::Ok) | Event::Activated => match self.form.selected(self.list).and_then(|r| self.shown.get(r)) {
                Some(&i) => {
                    model.play_index(i);
                    false
                }
                None => true,
            },
            Event::Clicked(_) | Event::Cancel => false,
            _ => true,
        }
    }
}

// --- Preferences ----------------------------------------------------------------

/// The on/off preferences. Check boxes take effect at once.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Flag {
    Snap,
    EasyMove,
    KeepOnScreen,
    AlwaysOnTop,
    DoubleSize,
    SkinCursors,
    ScrollTitle,
    DimInactive,
    ShowNumbers,
    ZeroPad,
    AutoAdvance,
    TrayIcon,
    MinimizeToTray,
    NotifyTracks,
    GlobalHotkeys,
    KeepScanOpen,
    CheckUpdates,
    EnqueueDefault,
}

impl Flag {
    fn label(self) -> &'static str {
        match self {
            Flag::Snap => "Snap windows together",
            Flag::EasyMove => "Easy move",
            Flag::KeepOnScreen => "Keep windows on screen",
            Flag::AlwaysOnTop => "Always on top",
            Flag::DoubleSize => "Double size",
            Flag::SkinCursors => "Use skin cursors",
            Flag::ScrollTitle => "Scroll song title",
            Flag::DimInactive => "Dim inactive playlist",
            Flag::ShowNumbers => "Show playlist numbers",
            Flag::ZeroPad => "Zero-pad numbers",
            Flag::AutoAdvance => "Advance to next track",
            Flag::TrayIcon => "Icon in the system tray",
            Flag::MinimizeToTray => "Minimize to the tray",
            Flag::NotifyTracks => "Notify of each new track",
            Flag::CheckUpdates => "Check for a new version",
            Flag::GlobalHotkeys => "Global hotkeys (Ctrl+Alt)",
            Flag::KeepScanOpen => "Keep the ReplayGain scan window open",
            Flag::EnqueueDefault => "Enter and double-click enqueue instead of playing",
        }
    }

    /// The command that toggles the preference elsewhere (menu, keys), when
    /// changing it needs more than storing it.
    fn command(self) -> Option<Command> {
        match self {
            Flag::EasyMove => Some(Command::ToggleEasyMove),
            Flag::AlwaysOnTop => Some(Command::ToggleAlwaysOnTop),
            Flag::DoubleSize => Some(Command::ToggleDoubleSize),
            Flag::ScrollTitle => Some(Command::ToggleAutoscroll),
            Flag::AutoAdvance => Some(Command::ToggleManualAdvance),
            _ => None,
        }
    }

    fn slot(self, s: &mut Settings) -> &mut bool {
        match self {
            Flag::Snap => &mut s.snap,
            Flag::EasyMove => &mut s.easymove,
            Flag::KeepOnScreen => &mut s.keeponscreen,
            Flag::AlwaysOnTop => &mut s.aot,
            Flag::DoubleSize => &mut s.dsize,
            Flag::SkinCursors => &mut s.usecursors,
            Flag::ScrollTitle => &mut s.autoscrollname,
            Flag::DimInactive => &mut s.dim_inactive,
            Flag::ShowNumbers => &mut s.shownumsinpl,
            Flag::ZeroPad => &mut s.zeropadplnum,
            Flag::AutoAdvance => &mut s.pladv,
            Flag::TrayIcon => &mut s.tray_icon,
            Flag::MinimizeToTray => &mut s.minimize_to_tray,
            Flag::NotifyTracks => &mut s.notify_tracks,
            Flag::CheckUpdates => &mut s.update_check,
            Flag::GlobalHotkeys => &mut s.global_hotkeys,
            Flag::KeepScanOpen => &mut s.rg_scan_keep_open,
            Flag::EnqueueDefault => &mut s.enqueuedef,
        }
    }

    pub fn get(self, s: &Settings) -> bool {
        // A copy keeps the list of fields in one place, `slot`; it is
        // only read when a check box is drawn or clicked.
        *self.slot(&mut s.clone())
    }

    /// Sets the preference the way its menu entry or key would, so that
    /// windows, flashes and the engine follow.
    pub fn set(self, model: &mut Model, on: bool) {
        if self.get(model.settings()) == on {
            return;
        }
        match self.command() {
            Some(c) => model.exec(c),
            None => *self.slot(model.settings_mut()) = on,
        }
        if self == Flag::SkinCursors {
            model.ui.push(UiReq::SkinChanged);
        }
        model.pl_dirty += 1;
    }
}

/// The preferences typed in. They apply on OK; Cancel forgets the edits.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Field {
    FontSize,
    TitleFormat,
    FadeMs,
    CrossfadeMs,
    BufferMs,
    Proxy,
    SoundFont,
}

impl Field {
    fn label(self) -> &'static str {
        match self {
            Field::FontSize => "Playlist font size",
            Field::TitleFormat => "Title format",
            Field::FadeMs => "Fade-out (ms)",
            Field::CrossfadeMs => "Crossfade (ms)",
            Field::BufferMs => "Output buffer (ms)",
            Field::Proxy => "HTTP proxy",
            Field::SoundFont => "MIDI SoundFont",
        }
    }

    fn get(self, s: &Settings) -> String {
        match self {
            Field::FontSize => s.pe_fontsize.to_string(),
            Field::TitleFormat => s.titlefmt.clone(),
            Field::FadeMs => s.fade_ms.to_string(),
            Field::CrossfadeMs => s.crossfade_ms.to_string(),
            Field::BufferMs => s.buffer_ms.to_string(),
            Field::Proxy => s.proxy.clone(),
            Field::SoundFont => s.soundfont.clone(),
        }
    }

    /// Checks `text` without changing anything.
    fn check(self, text: &str) -> Result<(), String> {
        let number = |what| text.trim().parse::<u32>().map(drop).map_err(|_| format!("{what}: enter a whole number"));
        match self {
            Field::FontSize => number("Font size"),
            Field::FadeMs => number("Fade-out"),
            Field::CrossfadeMs => number("Crossfade"),
            Field::BufferMs => number("Output buffer"),
            Field::TitleFormat if text.trim().is_empty() => Err("Title format: must not be empty".into()),
            Field::SoundFont if !text.trim().is_empty() && !std::path::Path::new(text.trim()).is_file() => {
                Err("MIDI SoundFont: no such file (leave it empty for the system's)".into())
            }
            Field::TitleFormat | Field::Proxy | Field::SoundFont => Ok(()),
        }
    }

    /// Stores a checked value; numbers are kept in range by the settings.
    fn apply(self, model: &mut Model, text: &str) {
        let n = || text.trim().parse::<u32>().unwrap_or(0);
        match self {
            Field::FontSize => model.settings_mut().pe_fontsize = n().min(1000) as i32,
            Field::FadeMs => model.settings_mut().fade_ms = n(),
            Field::CrossfadeMs => model.settings_mut().crossfade_ms = n(),
            Field::BufferMs => model.settings_mut().buffer_ms = n(),
            Field::Proxy => model.settings_mut().proxy = text.trim().to_string(),
            Field::SoundFont => model.settings_mut().soundfont = text.trim().to_string(),
            Field::TitleFormat => {
                if text != model.settings().titlefmt {
                    model.set_title_format(text);
                }
            }
        }
    }
}

#[cfg(linux_like)]
type FileTypes = crate::filetypes::Desktop;
/// Windows and macOS keep the choice to themselves.
#[cfg(not(linux_like))]
struct FileTypes;
#[cfg(not(linux_like))]
impl FileTypes {
    fn from_env() -> FileTypes {
        FileTypes
    }
}

/// The sections of Preferences, listed down its left side; each shows its
/// check boxes, then its typed values.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Section {
    Windows,
    Playlist,
    Playback,
    Library,
    Desktop,
    Internet,
}

impl Section {
    const ALL: [Section; 6] =
        [Section::Windows, Section::Playlist, Section::Playback, Section::Library, Section::Desktop, Section::Internet];

    fn label(self) -> &'static str {
        match self {
            Section::Windows => "Windows",
            Section::Playlist => "Playlist",
            Section::Playback => "Playback",
            Section::Library => "Media Library",
            Section::Desktop => "Desktop",
            Section::Internet => "Internet",
        }
    }

    fn flags(self) -> &'static [Flag] {
        match self {
            Section::Windows => {
                &[Flag::Snap, Flag::EasyMove, Flag::KeepOnScreen, Flag::AlwaysOnTop, Flag::DoubleSize, Flag::SkinCursors, Flag::ScrollTitle]
            }
            Section::Playlist => &[Flag::DimInactive, Flag::ShowNumbers, Flag::ZeroPad],
            Section::Playback => &[Flag::AutoAdvance, Flag::KeepScanOpen],
            Section::Library => &[Flag::EnqueueDefault],
            Section::Desktop => &[Flag::TrayIcon, Flag::MinimizeToTray, Flag::NotifyTracks, Flag::GlobalHotkeys],
            Section::Internet => &[Flag::CheckUpdates],
        }
    }

    fn fields(self) -> &'static [Field] {
        match self {
            Section::Playlist => &[Field::FontSize, Field::TitleFormat],
            Section::Playback => &[Field::FadeMs, Field::CrossfadeMs, Field::BufferMs, Field::SoundFont],
            Section::Internet => &[Field::Proxy],
            Section::Windows | Section::Library | Section::Desktop => &[],
        }
    }
}

pub struct Preferences {
    form: Form<Act>,
    /// Where the default players are chosen, and the check box of each
    /// group in [`Group::ALL`] order.
    #[cfg_attr(not(linux_like), allow(dead_code))]
    file_types: FileTypes,
    #[cfg(linux_like)]
    type_checks: [usize; 3],
    fields: Vec<(Field, usize)>,
    /// The devices to play on, and the list that shows them.
    devices: Vec<Device>,
    device_list: usize,
    status: usize,
}

/// Rows the device list shows at once.
const DEVICE_ROWS: i32 = 4;

impl Preferences {
    fn new(s: &Settings, mut devices: Vec<Device>, file_types: FileTypes) -> Preferences {
        let w = 560;
        let mut form = Form::new(w, 0);
        let labels = Section::ALL.map(|p| p.label().to_string()).to_vec();
        let (x0, cw) = (160, w - 170);
        let mut bottom = 14 + labels.len() as i32 * PAGE_H;
        let mut fields = Vec::new();
        #[cfg(linux_like)]
        let mut type_checks = [0; 3];
        let mut device_list = 0;
        for (page, section) in Section::ALL.into_iter().enumerate() {
            let flags = section.flags();
            let rows = flags.len() as i32;
            form.add_on(page, Rect::new(x0, 10, cw, 22 + rows * 22), Widget::Group { label: section.label().into() });
            for (n, &f) in flags.iter().enumerate() {
                form.add_on(
                    page,
                    Rect::new(x0 + 12, 30 + n as i32 * 22, cw - 24, 20),
                    Widget::Check { label: f.label().into(), checked: f.get(s), action: Act::Flag(f) },
                );
            }
            let mut y = 40 + rows * 22;
            for &f in section.fields() {
                form.add_on(page, Rect::new(x0 + 4, y, 136, 24), Widget::Label { text: f.label().into(), dim: false });
                let text = f.get(s);
                let cursor = text.chars().count();
                fields.push((f, form.add_on(page, Rect::new(x0 + 140, y, cw - 140, 24), Widget::Input { text, cursor })));
                y += 30;
            }
            match section {
                Section::Playback => {
                    if !devices.iter().any(|d| d.name == s.output_device) {
                        // Unplugged, or typed into the settings file: kept until changed.
                        devices.push(Device { label: format!("{} (not found)", s.output_device), name: s.output_device.clone() });
                    }
                    let current = devices.iter().position(|d| d.name == s.output_device);
                    form.add_on(page, Rect::new(x0 + 4, y, 136, 24), Widget::Label { text: "Output device".into(), dim: false });
                    let list = Widget::List { rows: vec![], selected: None, scroll: 0 };
                    device_list = form.add_on(page, Rect::new(x0 + 140, y, cw - 140, DEVICE_ROWS * ROW_H + 4), list);
                    form.set_rows(device_list, devices.iter().map(|d| d.label.clone()).collect(), current);
                    y += DEVICE_ROWS * ROW_H + 10;
                }
                Section::Desktop => {
                    form.add_on(page, Rect::new(x0, y, cw, 22 + 3 * 22), Widget::Group { label: "Open with Popamp".into() });
                    #[cfg(linux_like)]
                    {
                        let on = file_types.defaults();
                        type_checks = std::array::from_fn(|n| {
                            let label = Group::ALL[n].label().into();
                            let r = Rect::new(x0 + 12, y + 20 + n as i32 * 22, cw - 24, 20);
                            form.add_on(page, r, Widget::Check { label, checked: on[n], action: Act::FileType })
                        });
                    }
                    #[cfg(windows)]
                    {
                        let note = "Windows lets you choose the default player in its Settings.".into();
                        form.add_on(page, Rect::new(x0 + 12, y + 20, cw - 24, 24), Widget::Label { text: note, dim: false });
                        let r = Rect::new(x0 + 12, y + 48, BUTTON.0 + 20, BUTTON.1);
                        form.add_on(page, r, Widget::Button { label: "Default apps".into(), action: Act::DefaultApps, default: false });
                    }
                    #[cfg(target_os = "macos")]
                    {
                        let note = "Choose Popamp in Finder: Get Info, then Open with.".into();
                        form.add_on(page, Rect::new(x0 + 12, y + 20, cw - 24, 24), Widget::Label { text: note, dim: false });
                    }
                    y += 30 + 3 * 22;
                }
                _ => {}
            }
            bottom = bottom.max(y);
        }
        let y = bottom + 10;
        let status = form.add(Rect::new(x0, y, w - 2 * BUTTON.0 - 22 - x0, BUTTON.1), Widget::Label { text: String::new(), dim: false });
        button(&mut form, w - 2 * BUTTON.0 - 16, y, "OK", Act::Ok, true);
        button(&mut form, w - BUTTON.0 - 10, y, "Cancel", Act::Close, false);
        form.h = y + BUTTON.1 + 10;
        // The list of sections runs down to the buttons, room to spare.
        form.add(Rect::new(10, 10, 140, form.h - 20), Widget::Pages { labels, selected: 0, action: Act::Section });
        Preferences {
            form,
            file_types,
            #[cfg(linux_like)]
            type_checks,
            fields,
            devices,
            device_list,
            status,
        }
    }

    fn handle(&mut self, ev: Event<Act>, model: &mut Model) -> bool {
        match ev {
            Event::Toggled(Act::Flag(f), on) => {
                f.set(model, on);
                true
            }
            Event::Clicked(Act::Ok) => self.apply(model),
            #[cfg(windows)]
            Event::Clicked(Act::DefaultApps) => {
                crate::filetypes::open_default_apps();
                true
            }
            Event::Clicked(_) | Event::Cancel => false,
            _ => true,
        }
    }

    /// Stores the typed values when all are valid; otherwise shows the first
    /// problem and keeps the window open.
    fn apply(&mut self, model: &mut Model) -> bool {
        let checked = self.fields.iter().try_for_each(|&(f, i)| f.check(self.form.text(i)));
        if let Err(e) = checked {
            self.form.set_label(self.status, e);
            return true;
        }
        #[cfg(feature = "plugins")]
        let proxy = model.settings().proxy.clone();
        for &(f, i) in &self.fields {
            f.apply(model, self.form.text(i));
        }
        if let Some(d) = self.form.selected(self.device_list).and_then(|i| self.devices.get(i)) {
            model.settings_mut().output_device = d.name.clone();
        }
        model.settings_mut().sanitize();
        model.push_engine_settings();
        #[cfg(feature = "plugins")]
        if model.settings().proxy != proxy {
            // Plug-ins get the proxy when they load.
            model.load_plugins();
        }
        model.pl_dirty += 1;
        // The rest is kept either way; a file that cannot be written says so.
        #[cfg(linux_like)]
        if let Err(e) = self.file_types.set(self.type_checks.map(|i| self.form.checked(i))) {
            self.form.set_label(self.status, e);
            return true;
        }
        false
    }
}

// --- Skin browser --------------------------------------------------------------

/// The installed skins; picking one puts it on at once.
pub struct SkinBrowser {
    form: Form<Act>,
    list: usize,
    /// The skin of each row after the first, the built-in one.
    skins: Vec<std::path::PathBuf>,
    /// The skin on when the rows were made.
    current: Option<std::path::PathBuf>,
}

impl SkinBrowser {
    fn new(model: &Model) -> SkinBrowser {
        let (w, list_h) = (380, 4 + 16 * ROW_H);
        let mut form = Form::new(w, 10 + list_h + 10 + BUTTON.1 + 10);
        let list = form.add(Rect::new(10, 10, w - 20, list_h), Widget::List { rows: vec![], selected: None, scroll: 0 });
        let by = 10 + list_h + 10;
        form.add(
            Rect::new(10, by, 120, BUTTON.1),
            Widget::Button { label: "Install from file…".into(), action: Act::InstallSkin, default: false },
        );
        form.add(Rect::new(136, by, BUTTON.0, BUTTON.1), Widget::Button { label: "Gallery…".into(), action: Act::Gallery, default: false });
        button(&mut form, w - BUTTON.0 - 10, by, "Close", Act::Close, true);
        let mut p = SkinBrowser { form, list, skins: vec![], current: None };
        p.relist(model);
        p
    }

    /// Lists the skins installed now, with the one on selected.
    fn relist(&mut self, model: &Model) {
        self.skins = popamp_skin::list_skins(&model.paths.skins_dirs());
        self.current = model.skin_path.clone();
        let names = self.skins.iter().map(|p| p.file_name().unwrap_or_default().to_string_lossy().into_owned());
        let rows = std::iter::once("Popamp Default".to_string()).chain(names).collect();
        let on = self.current.as_ref().and_then(|c| self.skins.iter().position(|s| s == c)).map_or(0, |i| i + 1);
        self.form.set_rows(self.list, rows, Some(on));
    }

    /// A skin installed or put on elsewhere shows here too.
    fn refresh(&mut self, model: &Model) -> bool {
        if model.skin_path == self.current {
            return false;
        }
        self.relist(model);
        true
    }

    fn handle(&mut self, ev: Event<Act>, model: &mut Model) -> bool {
        match ev {
            Event::Selected => {
                let skin = self.form.selected(self.list).and_then(|r| r.checked_sub(1)).and_then(|i| self.skins.get(i).cloned());
                if skin != model.skin_path {
                    model.set_skin(skin);
                }
                self.current = model.skin_path.clone();
                true
            }
            Event::Clicked(Act::InstallSkin) => {
                model.request_dialog(crate::dialogs::DialogKind::InstallSkin);
                true
            }
            Event::Clicked(Act::Gallery) => {
                model.exec(Command::Gallery);
                true
            }
            Event::Activated | Event::Clicked(_) | Event::Cancel => false,
            _ => true,
        }
    }
}

// --- Skin Museum -----------------------------------------------------------------

/// Browse and search skins.webamp.org, preview one, download and use it.
pub struct SkinMuseum {
    form: Form<Act>,
    search: usize,
    list: usize,
    shot: usize,
    status: usize,
    /// The search the list shows (empty: the museum's own order).
    query: String,
    entries: Vec<museum::Entry>,
    /// A page request is under way.
    loading: bool,
    /// The last page was full, so there may be more.
    more: bool,
    shots: HashMap<String, Bitmap>,
    /// Requests for `app.rs` to run in the background.
    jobs: Vec<Job>,
}

impl SkinMuseum {
    const CREDIT: &'static str = "Skins from skins.webamp.org belong to their authors.";

    fn new() -> SkinMuseum {
        let (list_w, shot_w, shot_h) = (260, 275, 348);
        let w = 10 + list_w + 10 + shot_w + 10;
        let mut form = Form::new(w, 0);
        let search = form.add(Rect::new(10, 10, list_w - 80, 24), Widget::Input { text: String::new(), cursor: 0 });
        button(&mut form, 10 + list_w - 74, 10, "Search", Act::Search, true);
        let list_h = 4 + 16 * ROW_H;
        let list = form.add(Rect::new(10, 42, list_w, list_h), Widget::List { rows: vec![], selected: None, scroll: 0 });
        button(&mut form, 10, 42 + list_h + 8, "More", Act::More, false);
        let shot = form.add(Rect::new(10 + list_w + 10, 10, shot_w, shot_h), Widget::Image { image: None, note: String::new() });
        let y = 10 + shot_h + 14;
        let status = form.add(Rect::new(10, y, w - 2 * BUTTON.0 - 36, BUTTON.1), Widget::Label { text: Self::CREDIT.into(), dim: true });
        form.add(
            Rect::new(w - 2 * BUTTON.0 - 56, y, BUTTON.0 + 40, BUTTON.1),
            Widget::Button { label: "Download & use".into(), action: Act::Ok, default: false },
        );
        button(&mut form, w - BUTTON.0 - 10, y, "Close", Act::Close, false);
        form.h = y + BUTTON.1 + 10;
        let mut p = SkinMuseum {
            form,
            search,
            list,
            shot,
            status,
            query: String::new(),
            entries: vec![],
            loading: false,
            more: false,
            shots: HashMap::new(),
            jobs: vec![],
        };
        p.load(String::new());
        p
    }

    /// Starts a new list for `query`.
    fn load(&mut self, query: String) {
        self.entries.clear();
        self.form.set_rows(self.list, vec![], None);
        self.form.set_image(self.shot, None, "");
        self.query = query;
        self.request_page();
    }

    fn request_page(&mut self) {
        self.loading = true;
        self.form.set_label(self.status, "Loading…".into());
        self.jobs.push(Job::Page { query: self.query.clone(), offset: self.entries.len() });
    }

    fn selected(&self) -> Option<&museum::Entry> {
        self.form.selected(self.list).and_then(|i| self.entries.get(i))
    }

    /// Shows the selected skin's screenshot, fetching it the first time.
    fn show_selected(&mut self) {
        let Some(md5) = self.selected().map(|e| e.md5.clone()) else { return };
        match self.shots.get(&md5) {
            Some(img) => self.form.set_image(self.shot, Some(img.clone()), ""),
            None => {
                self.form.set_image(self.shot, None, "Loading preview…");
                self.jobs.push(Job::Screenshot { md5 });
            }
        }
    }

    fn handle(&mut self, ev: Event<Act>, model: &mut Model) -> bool {
        match ev {
            Event::Clicked(Act::Search) => self.load(self.form.text(self.search).trim().to_string()),
            Event::Clicked(Act::More) if self.more && !self.loading => self.request_page(),
            Event::Selected => self.show_selected(),
            Event::Clicked(Act::Ok) | Event::Activated => {
                if let Some(entry) = self.selected().cloned() {
                    self.form.set_label(self.status, format!("Downloading {}…", entry.name()));
                    self.jobs.push(Job::Download { entry, dir: model.paths.skins_dir() });
                }
            }
            Event::Clicked(Act::Close) | Event::Cancel => return false,
            _ => {}
        }
        true
    }

    fn fetched(&mut self, f: Fetched, model: &mut Model) {
        match f {
            Fetched::Page { query, offset, result } => self.page_fetched(&query, offset, result),
            Fetched::Screenshot { md5, result } => self.screenshot_fetched(md5, result),
            Fetched::Download { name, result } => {
                let note = match result {
                    Ok(path) => {
                        model.set_skin(Some(path));
                        format!("Using {name}.")
                    }
                    Err(e) => format!("{name}: {e}"),
                };
                self.form.set_label(self.status, note);
            }
        }
    }

    fn page_fetched(&mut self, query: &str, offset: usize, result: Result<Vec<museum::Entry>, String>) {
        // Answers to an older search, or a page already shown, are dropped.
        if query != self.query || offset != self.entries.len() {
            return;
        }
        self.loading = false;
        match result {
            Ok(page) => self.add_page(page),
            Err(e) => self.form.set_label(self.status, format!("Museum: {e}")),
        }
    }

    fn screenshot_fetched(&mut self, md5: String, result: Result<Bitmap, String>) {
        let showing = self.selected().is_some_and(|e| e.md5 == md5);
        match result {
            Ok(img) => {
                self.shots.insert(md5, img);
                if showing {
                    self.show_selected();
                }
            }
            Err(e) if showing => self.form.set_image(self.shot, None, &format!("No preview: {e}")),
            Err(_) => {}
        }
    }

    fn add_page(&mut self, page: Vec<museum::Entry>) {
        self.more = page.len() >= museum::PAGE;
        let first = self.entries.is_empty();
        let names: Vec<String> = page.iter().map(museum::Entry::name).collect();
        self.entries.extend(page);
        if first {
            self.form.set_rows(self.list, names, Some(0));
            self.show_selected();
        } else {
            self.form.push_rows(self.list, names);
        }
        let note = match self.entries.len() {
            0 => "Nothing found.".to_string(),
            n => format!("{n} skins. {}", Self::CREDIT),
        };
        self.form.set_label(self.status, note);
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::widgets::Key;
    use popamp_core::plformats::Item;

    pub(crate) fn songs(titles: &[&str]) -> Model {
        let mut m = Model::for_tests();
        let items = titles.iter().enumerate().map(|(i, t)| Item {
            location: format!("/music/{i}.mp3"),
            title: Some(t.to_string()),
            length_ms: None,
        });
        m.on_expanded(items.collect(), crate::model::AddMode::Append);
        m
    }

    pub(crate) fn type_text(p: &mut Panel, m: &mut Model, s: &str) -> bool {
        s.chars().all(|c| {
            let ev = p.form_mut().key(Key::Char(c));
            p.handle(ev, m)
        })
    }

    pub(crate) fn press(p: &mut Panel, m: &mut Model, k: Key) -> bool {
        let ev = p.form_mut().key(k);
        p.handle(ev, m)
    }

    fn rows(p: &Panel) -> Vec<String> {
        let Panel::Jump(j) = p else { panic!("not jump to file") };
        match j.form.widget(j.list) {
            Widget::List { rows, .. } => rows.clone(),
            _ => unreachable!(),
        }
    }

    fn copy_dir(from: &std::path::Path, to: &std::path::Path) {
        std::fs::create_dir_all(to).unwrap();
        for e in std::fs::read_dir(from).unwrap().flatten() {
            if e.path().is_dir() {
                copy_dir(&e.path(), &to.join(e.file_name()));
            } else {
                std::fs::copy(e.path(), to.join(e.file_name())).unwrap();
            }
        }
    }

    /// Clicks the button for `act`.
    pub(crate) fn click_button(p: &mut Panel, m: &mut Model, act: Act) -> bool {
        let i = (0..).find(|&i| matches!(p.form().widget(i), Widget::Button { action, .. } if *action == act)).unwrap();
        let r = p.form().rect(i);
        p.form_mut().mouse_down(r.x + 2, r.y + 2, false);
        let ev = p.form_mut().mouse_up(r.x + 2, r.y + 2);
        p.handle(ev, m)
    }

    #[test]
    fn the_skin_browser_puts_on_the_skin_picked_and_follows_new_ones() {
        let mut m = Model::for_tests();
        let fixtures = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../crates/popamp-skin/tests/fixtures");
        let dir = m.paths.skins_dir();
        copy_dir(&fixtures.join("fonts-shade"), &dir.join("fonts-shade"));
        let mut p = Panel::open(PanelKind::SkinBrowser, &m);
        let Panel::Skins(b) = &p else { panic!("not the skin browser") };
        let (rows, selected) = match b.form.widget(b.list) {
            Widget::List { rows, selected, .. } => (rows.clone(), *selected),
            _ => unreachable!(),
        };
        assert_eq!(rows[0], "Popamp Default");
        assert_eq!(selected, Some(0), "the built-in skin is on");
        let at = rows.iter().position(|r| r == "fonts-shade").expect("the installed skin is listed");
        for _ in 0..at {
            assert!(press(&mut p, &mut m, Key::Down));
        }
        assert_eq!(m.skin_path, Some(dir.join("fonts-shade")), "on at once");
        assert!(press(&mut p, &mut m, Key::PageUp));
        assert_eq!(m.skin_path, None, "back to the built-in one");

        copy_dir(&fixtures.join("shaped"), &dir.join("shaped"));
        m.set_skin(Some(dir.join("shaped")));
        assert!(p.refresh(&m), "a skin put on elsewhere");
        let Panel::Skins(b) = &p else { unreachable!() };
        let Widget::List { rows, selected, .. } = b.form.widget(b.list) else { unreachable!() };
        assert_eq!(selected.map(|i| rows[i].as_str()), Some("shaped"));
        assert!(!p.refresh(&m), "nothing new");
    }

    #[test]
    fn the_skin_browser_installs_from_a_file_or_the_gallery() {
        let mut m = Model::for_tests();
        let mut p = Panel::open(PanelKind::SkinBrowser, &m);
        assert!(click_button(&mut p, &mut m, Act::InstallSkin));
        assert!(matches!(m.ui.last(), Some(UiReq::Dialog(crate::dialogs::DialogKind::InstallSkin, _))));
        assert!(click_button(&mut p, &mut m, Act::Gallery));
        assert!(matches!(m.ui.last(), Some(UiReq::Panel(PanelKind::Gallery))));
        assert!(!press(&mut p, &mut m, Key::Escape));
    }

    #[test]
    fn search_needs_every_word_in_any_case() {
        let titles = ["Pink Floyd - Time", "Floyd Cramer - Last Date", "Time After Time"];
        assert_eq!(search(titles.into_iter(), "time"), [0, 2]);
        assert_eq!(search(titles.into_iter(), "FLOYD time"), [0]);
        assert_eq!(search(titles.into_iter(), "  "), [0, 1, 2]);
        assert!(search(titles.into_iter(), "jazz").is_empty());
    }

    #[test]
    fn jump_to_file_filters_and_plays() {
        let mut m = songs(&["Alpha", "Beta", "Gamma", "Alphabet"]);
        let mut p = Panel::open(PanelKind::JumpToFile, &m);
        assert_eq!(rows(&p).len(), 4);
        assert!(type_text(&mut p, &mut m, "alp"));
        assert_eq!(rows(&p), ["1. Alpha", "4. Alphabet"]);
        press(&mut p, &mut m, Key::Down);
        assert!(!press(&mut p, &mut m, Key::Enter), "jumping closes the window");
        assert_eq!(m.playlist.current(), Some(3));
    }

    #[test]
    fn jump_to_file_follows_the_playlist() {
        let mut m = songs(&["Alpha", "Beta"]);
        let mut p = Panel::open(PanelKind::JumpToFile, &m);
        type_text(&mut p, &mut m, "a");
        m.on_expanded(
            vec![Item { location: "/x.mp3".into(), title: Some("Delta".into()), length_ms: None }],
            crate::model::AddMode::Append,
        );
        p.playlist_changed(&m);
        assert_eq!(rows(&p), ["1. Alpha", "2. Beta", "3. Delta"]);
        // Nothing matches: Enter does nothing, Escape closes.
        type_text(&mut p, &mut m, "zzz");
        assert!(press(&mut p, &mut m, Key::Enter));
        assert!(!press(&mut p, &mut m, Key::Escape));
        assert_eq!(m.playlist.current(), None);
    }

    #[test]
    fn file_info_lists_the_selected_entry() {
        let mut m = songs(&["Alpha", "Beta"]);
        m.playlist.select_only(1);
        let p = Panel::open(PanelKind::FileInfo, &m);
        let labels: Vec<String> = (0..4)
            .map(|i| match p.form().widget(i) {
                Widget::Label { text, .. } => text.clone(),
                _ => String::new(),
            })
            .collect();
        assert_eq!(labels, ["Location", "/music/1.mp3", "Title", "Beta"]);
        let mut p = p;
        assert!(!press(&mut p, &mut m, Key::Enter));
    }

    /// Shows the section of Preferences control `i` is on, picking it on
    /// the left as a user would, and gives where the control is.
    fn reach(p: &mut Panel, m: &mut Model, i: usize) -> Rect {
        let Panel::Prefs(pr) = &*p else { panic!("not preferences") };
        let (page, r) = (pr.form.page_of(i), pr.form.rect(i));
        if let Some(page) = page {
            click(p, m, Rect::new(10, 10 + page as i32 * PAGE_H, 1, 1));
        }
        r
    }

    fn check_index(p: &Panel, flag: Flag) -> usize {
        let Panel::Prefs(pr) = p else { panic!("not preferences") };
        (0..).find(|&i| matches!(pr.form.widget(i), Widget::Check { action: Act::Flag(f), .. } if *f == flag)).unwrap()
    }

    /// Where the check box for `flag` is, its section shown.
    fn check_box(p: &mut Panel, m: &mut Model, flag: Flag) -> Rect {
        let i = check_index(p, flag);
        reach(p, m, i)
    }

    pub(crate) fn click(p: &mut Panel, m: &mut Model, r: Rect) -> bool {
        let ev = p.form_mut().mouse_down(r.x + 2, r.y + 2, false);
        p.handle(ev, m)
    }

    #[test]
    fn preference_check_boxes_apply_at_once() {
        let mut m = songs(&[]);
        let mut p = Panel::open(PanelKind::Preferences, &m);
        assert!(m.settings().snap && m.settings().pladv && !m.settings().aot);
        let r = check_box(&mut p, &mut m, Flag::Snap);
        click(&mut p, &mut m, r);
        let r = check_box(&mut p, &mut m, Flag::AutoAdvance);
        click(&mut p, &mut m, r);
        let r = check_box(&mut p, &mut m, Flag::AlwaysOnTop);
        click(&mut p, &mut m, r);
        assert!(!m.settings().snap && !m.settings().pladv && m.settings().aot);
        assert!(m.ui.iter().any(|r| matches!(r, UiReq::AlwaysOnTop)), "the windows are told");
    }

    #[test]
    fn each_section_shows_only_its_own_controls() {
        let mut m = songs(&[]);
        let mut p = Panel::open(PanelKind::Preferences, &m);
        let snap = check_box(&mut p, &mut m, Flag::Snap);
        let enqueue = check_box(&mut p, &mut m, Flag::EnqueueDefault);
        assert_eq!(snap, enqueue, "both are the first box of their section");
        let Panel::Prefs(pr) = &p else { unreachable!() };
        assert_eq!(pr.form.shown_page(), Section::ALL.iter().position(|&s| s == Section::Library).unwrap());
        assert!(m.settings().enqueuedef && m.settings().snap);
        click(&mut p, &mut m, enqueue);
        assert!(!m.settings().enqueuedef && m.settings().snap, "the box shown is the one ticked");
        for section in Section::ALL {
            for &f in section.flags() {
                let Panel::Prefs(pr) = &p else { unreachable!() };
                assert_eq!(pr.form.page_of(check_index(&p, f)), Section::ALL.iter().position(|&s| s == section), "{f:?}");
            }
        }
    }

    fn field(p: &Panel, f: Field) -> usize {
        let Panel::Prefs(pr) = p else { panic!("not preferences") };
        pr.fields.iter().find(|(g, _)| *g == f).unwrap().1
    }

    fn retype(p: &mut Panel, m: &mut Model, f: Field, text: &str) {
        let i = field(p, f);
        let r = reach(p, m, i);
        click(p, m, r);
        for _ in 0..100 {
            press(p, m, Key::Backspace);
        }
        type_text(p, m, text);
    }

    #[test]
    fn typed_preferences_apply_on_ok_only_when_valid() {
        let mut m = songs(&["Alpha"]);
        let mut p = Panel::open(PanelKind::Preferences, &m);
        retype(&mut p, &mut m, Field::FontSize, "14");
        retype(&mut p, &mut m, Field::FadeMs, "soon");
        assert!(press(&mut p, &mut m, Key::Enter), "a bad number keeps the window open");
        assert_eq!(m.settings().pe_fontsize, 10);
        let status = match &p {
            Panel::Prefs(pr) => pr.form.widget(pr.status).clone(),
            _ => unreachable!(),
        };
        assert!(matches!(status, Widget::Label { text, .. } if text.contains("Fade-out")));
        retype(&mut p, &mut m, Field::FadeMs, "500");
        retype(&mut p, &mut m, Field::TitleFormat, "%title%!");
        retype(&mut p, &mut m, Field::CrossfadeMs, "60000");
        assert!(!press(&mut p, &mut m, Key::Enter));
        assert_eq!((m.settings().pe_fontsize, m.settings().fade_ms), (14, 500));
        assert_eq!(m.settings().crossfade_ms, 10_000, "kept to ten seconds");
        assert_eq!(m.settings().titlefmt, "%title%!");
    }

    #[test]
    fn cancel_forgets_typed_preferences() {
        let mut m = songs(&[]);
        let mut p = Panel::open(PanelKind::Preferences, &m);
        retype(&mut p, &mut m, Field::FontSize, "30");
        assert!(!press(&mut p, &mut m, Key::Escape));
        assert_eq!(m.settings().pe_fontsize, 10);
    }

    fn device(name: &str, label: &str) -> Device {
        Device { name: name.into(), label: label.into() }
    }

    /// Preferences listing a default device and two cards.
    fn prefs_with_devices(m: &Model) -> Panel {
        let cards =
            vec![device("default", "Default device"), device("hdmi:CARD=NVidia,DEV=0", "HDMI"), device("front:CARD=Audio,DEV=0", "USB")];
        Panel::Prefs(Preferences::new(m.settings(), cards, test_types("devices")))
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn preferences_say_where_a_mac_chooses_the_player() {
        let m = songs(&[]);
        let p = Panel::Prefs(Preferences::new(m.settings(), vec![], test_types("mac")));
        let Panel::Prefs(pr) = &p else { unreachable!() };
        assert!(pr.form.labels().iter().any(|l| l.starts_with("Choose Popamp in Finder")));
    }

    /// File types kept in a folder of the test's own.
    #[cfg(linux_like)]
    fn test_types(name: &str) -> FileTypes {
        let root = std::env::temp_dir().join(format!("popamp-prefs-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        test_types_at(&root)
    }

    #[cfg(not(linux_like))]
    fn test_types(_: &str) -> FileTypes {
        FileTypes
    }

    #[cfg(linux_like)]
    fn type_check(p: &mut Panel, m: &mut Model, g: Group) -> Rect {
        let Panel::Prefs(pr) = &*p else { panic!("not preferences") };
        let i = pr.type_checks[Group::ALL.iter().position(|&x| x == g).unwrap()];
        reach(p, m, i)
    }

    #[cfg(linux_like)]
    #[test]
    fn ticked_file_types_open_with_popamp_after_ok() {
        let mut m = songs(&[]);
        let types = test_types("ok");
        let root = types.config.parent().unwrap().to_path_buf();
        let mut p = Panel::Prefs(Preferences::new(m.settings(), vec![], types));
        let r = type_check(&mut p, &mut m, Group::Playlists);
        click(&mut p, &mut m, r);
        assert!(!root.exists(), "nothing written before OK");
        assert!(!press(&mut p, &mut m, Key::Enter));
        let Panel::Prefs(pr) = &p else { unreachable!() };
        assert_eq!(pr.file_types.defaults(), [false, true, false]);
        let reopened = Panel::Prefs(Preferences::new(m.settings(), vec![], test_types_at(&root)));
        let Panel::Prefs(pr) = &reopened else { unreachable!() };
        assert!(pr.form.checked(pr.type_checks[1]) && !pr.form.checked(pr.type_checks[0]), "shown ticked next time");
        let _ = std::fs::remove_dir_all(root);
    }

    #[cfg(linux_like)]
    fn test_types_at(root: &std::path::Path) -> FileTypes {
        FileTypes { config: root.join("config"), data: root.join("data"), dirs: vec![], exe: Some("/usr/bin/popamp".into()) }
    }

    #[cfg(linux_like)]
    #[test]
    fn cancel_leaves_the_file_types() {
        let mut m = songs(&[]);
        let types = test_types("cancel");
        let root = types.config.parent().unwrap().to_path_buf();
        let mut p = Panel::Prefs(Preferences::new(m.settings(), vec![], types));
        let r = type_check(&mut p, &mut m, Group::Audio);
        click(&mut p, &mut m, r);
        assert!(!press(&mut p, &mut m, Key::Escape));
        assert!(!root.exists());
    }

    fn device_rows(p: &Panel) -> (Vec<String>, Option<usize>) {
        let Panel::Prefs(pr) = p else { panic!("not preferences") };
        match pr.form.widget(pr.device_list) {
            Widget::List { rows, selected, .. } => (rows.clone(), *selected),
            _ => unreachable!(),
        }
    }

    /// Clicks the n-th row of the device list.
    fn pick_device(p: &mut Panel, m: &mut Model, row: i32) {
        let Panel::Prefs(pr) = &*p else { panic!("not preferences") };
        let r = reach(p, m, pr.device_list);
        click(p, m, Rect::new(r.x, r.y + row * ROW_H, r.w, ROW_H));
    }

    #[test]
    fn a_picked_device_is_kept_on_ok() {
        let mut m = songs(&[]);
        let mut p = prefs_with_devices(&m);
        assert_eq!(device_rows(&p), (vec!["Default device".into(), "HDMI".into(), "USB".into()], Some(0)));
        pick_device(&mut p, &mut m, 2);
        assert_eq!(m.settings().output_device, "default", "not before OK");
        assert!(!press(&mut p, &mut m, Key::Enter));
        assert_eq!(m.settings().output_device, "front:CARD=Audio,DEV=0");
        assert_eq!(device_rows(&prefs_with_devices(&m)).1, Some(2), "shown as chosen next time");
    }

    #[test]
    fn cancel_keeps_the_device() {
        let mut m = songs(&[]);
        let mut p = prefs_with_devices(&m);
        pick_device(&mut p, &mut m, 1);
        assert!(!press(&mut p, &mut m, Key::Escape));
        assert_eq!(m.settings().output_device, "default");
    }

    #[test]
    fn a_missing_device_stays_chosen_until_changed() {
        let mut m = songs(&[]);
        m.settings_mut().output_device = "usb-dac".into();
        let mut p = prefs_with_devices(&m);
        let (rows, selected) = device_rows(&p);
        assert_eq!((rows.last().map(String::as_str), selected), (Some("usb-dac (not found)"), Some(3)));
        assert!(!press(&mut p, &mut m, Key::Enter));
        assert_eq!(m.settings().output_device, "usb-dac");
    }

    #[test]
    fn numbers_are_kept_in_range() {
        let mut m = songs(&[]);
        let mut p = Panel::open(PanelKind::Preferences, &m);
        retype(&mut p, &mut m, Field::FontSize, "999");
        press(&mut p, &mut m, Key::Enter);
        assert_eq!(m.settings().pe_fontsize, 40);
    }

    fn museum_entry(n: u8) -> museum::Entry {
        museum::Entry { md5: format!("{n:032x}"), filename: format!("Skin {n}.wsz") }
    }

    fn museum_page(query: &str, offset: usize, n: usize) -> Fetched {
        let result = Ok((0..n).map(|i| museum_entry((offset + i) as u8)).collect());
        Fetched::Page { query: query.into(), offset, result }
    }

    fn museum(p: &Panel) -> &SkinMuseum {
        match p {
            Panel::Museum(m) => m,
            _ => panic!("not the museum"),
        }
    }

    #[test]
    fn museum_lists_pages_and_previews_the_selection() {
        let mut m = songs(&[]);
        let mut p = Panel::open(PanelKind::SkinMuseum, &m);
        assert!(matches!(&p.take_jobs()[..], [Job::Page { query, offset: 0 }] if query.is_empty()));
        p.fetched(museum_page("", 0, museum::PAGE), &mut m);
        assert_eq!(museum(&p).form.selected(museum(&p).list), Some(0));
        let md5_0 = museum_entry(0).md5;
        assert!(matches!(&p.take_jobs()[..], [Job::Screenshot { md5 }] if *md5 == md5_0));
        // More appends the next page; a late copy of the first is ignored.
        let ev = Event::Clicked(Act::More);
        p.handle(ev, &mut m);
        assert!(matches!(&p.take_jobs()[..], [Job::Page { offset, .. }] if *offset == museum::PAGE));
        p.fetched(museum_page("", 0, 3), &mut m);
        p.fetched(museum_page("", museum::PAGE, 2), &mut m);
        assert_eq!(museum(&p).entries.len(), museum::PAGE + 2);
        p.handle(Event::Clicked(Act::More), &mut m);
        assert!(p.take_jobs().is_empty(), "a short page was the last");
        // A preview is fetched once, then shown from the cache.
        p.fetched(Fetched::Screenshot { md5: md5_0.clone(), result: Ok(Bitmap::new(2, 2, 0)) }, &mut m);
        assert!(matches!(museum(&p).form.widget(museum(&p).shot), Widget::Image { image: Some(_), .. }));
        press(&mut p, &mut m, Key::Down);
        assert_eq!(p.take_jobs().len(), 1);
        press(&mut p, &mut m, Key::Up);
        assert!(p.take_jobs().is_empty());
    }

    #[test]
    fn museum_search_replaces_the_list() {
        let mut m = songs(&[]);
        let mut p = Panel::open(PanelKind::SkinMuseum, &m);
        p.fetched(museum_page("", 0, 5), &mut m);
        type_text(&mut p, &mut m, "bento");
        press(&mut p, &mut m, Key::Enter);
        assert!(matches!(&p.take_jobs()[..], [_, _, Job::Page { query, offset: 0 }] if query == "bento"));
        p.fetched(museum_page("", 5, 5), &mut m);
        assert!(museum(&p).entries.is_empty(), "answer to the old list");
        p.fetched(Fetched::Page { query: "bento".into(), offset: 0, result: Ok(vec![]) }, &mut m);
        let status = museum(&p).form.widget(museum(&p).status).clone();
        assert!(matches!(status, Widget::Label { text, .. } if text == "Nothing found."));
    }

    #[test]
    fn museum_downloads_into_the_skins_folder() {
        let mut m = songs(&[]);
        let mut p = Panel::open(PanelKind::SkinMuseum, &m);
        p.fetched(museum_page("", 0, 2), &mut m);
        p.take_jobs();
        p.handle(Event::Activated, &mut m);
        let dir = m.paths.skins_dir();
        assert!(matches!(&p.take_jobs()[..], [Job::Download { entry, dir: d }] if entry.md5 == museum_entry(0).md5 && *d == dir));
        let failed = Fetched::Download { name: "Skin 0".into(), result: Err("not a usable skin".into()) };
        p.fetched(failed, &mut m);
        let status = museum(&p).form.widget(museum(&p).status).clone();
        assert!(matches!(status, Widget::Label { text, .. } if text == "Skin 0: not a usable skin"));
        assert!(!press(&mut p, &mut m, Key::Escape));
    }
}
