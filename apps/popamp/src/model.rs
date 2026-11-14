//! Application state and command handling, independent of windowing.

use crate::cli::Remote;
use crate::dialogs::{DialogKind, DialogResult, Request};
use crate::menu::{MenuAction, MenuItem};
use crate::panels::PanelKind;
use crate::vis::Vis;
use popamp_audio::formats::Decoders;
use popamp_audio::source::{Context, Sources};
use popamp_audio::{Engine, EngineCmd, EngineEvent, EqSettings, PlayState, ReplayGainSettings, StreamInfo};
use popamp_core::atf::Format;
use popamp_core::config::{BarEdge, Config, Paths, Settings};
use popamp_core::eqf::{self, Preset};
use popamp_core::history::{self, History, Played};
use popamp_core::meta::TrackMeta;
use popamp_core::playlist::{is_url, Entry, Playlist, SortKey};
use popamp_core::plformats::{self, Item};
use popamp_core::replaygain::Scanned;
use popamp_core::text::format_duration_ms;
use popamp_core::Command;
use popamp_skin::layout::WindowKey;
use popamp_skin::Skin;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};
use winit::event_loop::EventLoopProxy;

/// Where background threads send their results. Headless rendering has
/// no event loop, so its sink drops everything.
#[derive(Clone)]
pub struct Events(Option<EventLoopProxy<AppEvent>>);

impl Events {
    /// Returns false once nobody is listening (or never was).
    pub fn send(&self, ev: AppEvent) -> bool {
        self.0.as_ref().is_some_and(|p| p.send_event(ev).is_ok())
    }
}

/// Events delivered to the UI thread.
#[derive(Debug)]
pub enum AppEvent {
    Engine(EngineEvent),
    Meta(u64, Result<TrackMeta, String>),
    Expanded {
        items: Vec<Item>,
        mode: AddMode,
    },
    Dialog(DialogResult),
    Remote(crate::ipc::Request),
    Museum(crate::museum::Fetched),
    Gallery(crate::gallery::Fetched),
    /// How a ReplayGain scan goes.
    Scan(crate::scan::ScanEvent),
    /// How a scan of the media library's folders goes.
    Library(crate::library::LibEvent),
    /// What popamp.app says the latest Popamp is.
    Update(Result<crate::update::Release, String>),
    /// The plug-ins, loaded anew, and an instance of each dsp one for the
    /// engine; the number is that of the request.
    #[cfg(feature = "plugins")]
    Plugins(u64, Vec<popamp_plugins::installed::Installed>, Vec<Box<dyn popamp_audio::dsp::Effect>>),
    /// A plug-in stopped after failing: its id, and the last failure.
    #[cfg(feature = "plugins")]
    PluginStopped(String, String),
    /// A visualizer's window has to close: the plug-in's id, and why.
    #[cfg(feature = "visualizers")]
    VisualizerStopped(String, String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AddMode {
    ReplaceAndPlay,
    Append,
    Insert(usize),
}

/// Things only the windowing layer can do.
#[derive(Debug)]
pub enum UiReq {
    Dialog(DialogKind, Request),
    /// Open (or raise) one of Popamp's own dialog windows.
    Panel(PanelKind),
    Menu(Vec<MenuItem>),
    /// Open the visual plug-in with this id.
    #[cfg(feature = "visualizers")]
    Visualizer(String),
    /// Window visibility/size/scale changed; re-layout and redraw.
    Layout,
    Minimize,
    Quit,
    Raise,
    SkinChanged,
    AlwaysOnTop,
}

#[derive(Debug, Default)]
pub struct Status {
    pub playing_id: Option<u64>,
    pub info: Option<StreamInfo>,
    pub stream_title: Option<String>,
    pub stream_name: Option<String>,
}

pub struct Model {
    pub paths: Paths,
    pub config: Config,
    pub playlist: Playlist,
    pub engine: Engine,
    pub format: Arc<Format>,
    pub skin: Skin,
    pub skin_path: Option<PathBuf>,
    /// When the skin's files last changed, to reload it after edits.
    skin_changed_at: Option<std::time::SystemTime>,
    pub status: Status,
    pub stop_after_current: bool,
    /// Temporary marquee text (volume, seek, errors) and when it expires.
    pub transient: Option<(String, Instant)>,
    /// A newer Popamp the site told of.
    pub update: Option<crate::update::Release>,
    pub eq_presets: Vec<Preset>,
    pub eq_auto: Vec<Preset>,
    /// Favourite files and streams, kept in `bookmarks.m3u8`.
    pub bookmarks: Vec<Item>,
    /// What was played, kept in `history.txt`.
    pub history: History,
    /// The ReplayGain values Popamp measured, kept in `replaygain.txt`.
    pub scanned: Arc<Scanned>,
    /// Where the ReplayGain scan is; its window shows it.
    pub scan: crate::scan::ScanState,
    /// The media library, kept in `library.txt`, and what its window shows.
    pub library: crate::library::LibraryPane,
    pub vis: Vis,
    pub ui: Vec<UiReq>,
    /// The windows are hidden; the tray icon is there to bring them back.
    pub in_tray: bool,
    pub quit: bool,
    pub(crate) events: Events,
    meta_tx: crossbeam_channel::Sender<(u64, String)>,
    pub(crate) sources: Arc<Sources>,
    /// The decoder plug-ins that are on; `sources` has them too.
    pub(crate) decoders: Decoders,
    #[cfg(feature = "plugins")]
    pub plugins: crate::plugins::Plugins,
    error_skips: usize,
    pub last_dir: Option<PathBuf>,
    /// Increments when anything visible in the playlist changes.
    pub pl_dirty: u64,
    /// Goes up whenever Popamp changes a file's tags, so covers are read again.
    pub tags_generation: u64,
    /// What a bar spans: the main window's monitor less the desktop's
    /// panels.
    pub screen: Option<crate::bar::Screen>,
    /// The window system has no place for a bar (Wayland): the main
    /// window is always a window.
    pub no_bar: bool,
}

/// How a bookmark shows in menus and lists: its title, else its location.
fn bookmark_label(b: &Item) -> String {
    b.title.clone().filter(|t| !t.is_empty()).unwrap_or_else(|| b.location.clone())
}

/// Local files get their tags read; other entries only when they came
/// without a title (listings already carry one) and their source has tags.
fn wants_meta(e: &Entry) -> bool {
    !e.is_url() || e.file_title.is_none()
}

fn spawn_meta_workers(events: Events, sources: Arc<Sources>) -> crossbeam_channel::Sender<(u64, String)> {
    let (tx, rx) = crossbeam_channel::unbounded::<(u64, String)>();
    for i in 0..2 {
        let rx = rx.clone();
        let (events, sources) = (events.clone(), sources.clone());
        let _ = std::thread::Builder::new().name(format!("popamp-meta{i}")).spawn(move || {
            for (id, path) in rx {
                let Some(res) = sources.of(&path).describe(&path) else { continue };
                if !events.send(AppEvent::Meta(id, res)) {
                    return;
                }
            }
        });
    }
    tx
}

/// Expands folders, playlists and channels into playlist items; each
/// location goes to the source that takes it.
fn expand(sources: &Sources, locations: Vec<String>, cx: &Context) -> Vec<Item> {
    let mut out = Vec::new();
    for loc in locations {
        match sources.of(&loc).expand(&loc, cx) {
            Ok(Some(items)) => out.extend(items),
            Ok(None) => out.push(Item { location: loc, title: None, length_ms: None }),
            Err(e) => log::warn!("cannot expand {loc}: {e}"),
        }
    }
    out
}

impl Model {
    pub fn new(paths: Paths, config: Config, proxy: EventLoopProxy<AppEvent>) -> Model {
        let engine_proxy = proxy.clone();
        // With plug-ins, tracks given at startup wait for them to load;
        // headless models have no event loop to load them through.
        let sources = Arc::new(if cfg!(feature = "plugins") { Sources::waiting() } else { Sources::default() });
        let engine = Engine::spawn(config.settings.output_device.clone(), sources.clone(), move |ev| {
            let _ = engine_proxy.send_event(AppEvent::Engine(ev));
        });
        let mut m = Self::with_engine(paths, config, engine, sources, Events(Some(proxy)));
        if m.settings().ml_scan_at_start {
            m.library_rescan();
        }
        m.check_for_update();
        m
    }

    /// A model with no audio device and no event loop, for drawing the
    /// windows offline (`--render`).
    pub fn headless(paths: Paths, config: Config) -> Model {
        Self::with_engine(paths, config, Engine::headless(), Arc::default(), Events(None))
    }

    /// A headless model with default settings and its own empty settings
    /// directory, for unit tests.
    #[cfg(test)]
    pub fn for_tests() -> Model {
        static N: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
        let n = N.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!("popamp-test-{}-{n}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        Self::headless(Paths::resolve(Some(dir.clone())), Config::load(dir.join("popamp.ini")))
    }
}

/// A test model's settings folder goes with it; only folders made by
/// `for_tests` are touched.
#[cfg(test)]
impl Drop for Model {
    fn drop(&mut self) {
        let dir = &self.paths.config_dir;
        let made_for_tests = dir.parent() == Some(std::env::temp_dir().as_path())
            && dir.file_name().is_some_and(|n| n.to_string_lossy().starts_with("popamp-test-"));
        if made_for_tests {
            let _ = std::fs::remove_dir_all(dir);
        }
    }
}

impl Model {
    fn with_engine(paths: Paths, config: Config, engine: Engine, sources: Arc<Sources>, events: Events) -> Model {
        let meta_tx = spawn_meta_workers(events.clone(), sources.clone());
        let eq_presets = eqf::read_file(&paths.eq_library()).unwrap_or_else(|_| eqf::builtin_presets());
        let eq_auto = eqf::read_file(&paths.eq_auto_library()).unwrap_or_default();
        let bookmarks = plformats::read_file(&paths.bookmarks_file()).unwrap_or_default();
        let history = History::load(&paths.history_file());
        let scanned = Arc::new(Scanned::load(&paths.replaygain_file()));
        let format = Format::cached(&config.settings.titlefmt);
        let library = crate::library::LibraryPane::new(popamp_core::library::Library::load(&paths.library_file()));
        let mut m = Model {
            library,
            paths,
            config,
            playlist: Playlist::new(),
            engine,
            format,
            skin: Skin::builtin_vector(),
            skin_path: None,
            skin_changed_at: None,
            status: Status::default(),
            stop_after_current: false,
            screen: None,
            no_bar: false,
            transient: None,
            update: None,
            eq_presets,
            eq_auto,
            bookmarks,
            history,
            scanned,
            scan: Default::default(),
            vis: Vis::new(),
            ui: Vec::new(),
            in_tray: false,
            quit: false,
            events,
            meta_tx,
            sources,
            decoders: Decoders::default(),
            #[cfg(feature = "plugins")]
            plugins: Default::default(),
            error_skips: 0,
            last_dir: None,
            pl_dirty: 0,
            tags_generation: 0,
        };
        let skin = m.settings().skin.clone();
        if !skin.is_empty() {
            m.set_skin(Some(PathBuf::from(skin)));
        }
        m.push_engine_settings();
        #[cfg(feature = "plugins")]
        m.load_plugins();
        m
    }

    pub fn settings(&self) -> &Settings {
        &self.config.settings
    }

    pub fn settings_mut(&mut self) -> &mut Settings {
        &mut self.config.settings
    }

    pub fn events(&self) -> &Events {
        &self.events
    }

    /// What sources need from the settings.
    pub(crate) fn source_context(&self) -> Context {
        Context {
            proxy: self.settings().proxy.clone(),
            soundfont: self.settings().soundfont.clone(),
            decoders: self.decoders.clone(),
            scanned: self.scanned.clone(),
        }
    }

    pub fn push_engine_settings(&mut self) {
        let s = self.settings().clone();
        self.engine.send(EngineCmd::Volume(s.volume as u8));
        self.engine.send(EngineCmd::Balance(s.balance as i8));
        self.engine.send(EngineCmd::Config {
            device: s.output_device.clone(),
            buffer_ms: s.buffer_ms,
            fade_ms: s.fade_ms,
            crossfade_ms: s.crossfade_ms,
            cx: self.source_context(),
        });
        self.engine.send(EngineCmd::ReplayGain(ReplayGainSettings {
            enabled: s.replaygain,
            album: s.replaygain_mode == 1,
            preamp_db: s.replaygain_preamp,
            non_rg_db: s.replaygain_non_rg_gain,
        }));
        self.push_eq();
    }

    pub fn eq_settings(&self) -> EqSettings {
        let s = self.settings();
        EqSettings {
            enabled: s.use_eq,
            iso: s.eq_frequencies == 1,
            limiter: s.eq_limiter,
            bands_db: s.eq_bands().map(eqf::value_to_db),
            preamp_db: eqf::value_to_db(s.preamp as u8),
        }
    }

    pub fn push_eq(&mut self) {
        let eq = self.eq_settings();
        self.engine.send(EngineCmd::Eq(eq));
    }

    pub fn state(&self) -> PlayState {
        self.engine.shared().state()
    }

    pub fn flash(&mut self, text: impl Into<String>) {
        self.transient = Some((text.into(), Instant::now() + Duration::from_millis(1500)));
    }

    // --- playlist ------------------------------------------------------------

    pub fn add(&mut self, locations: Vec<String>, mode: AddMode) {
        // Skin archives are installed, not queued.
        let (skins, locations): (Vec<String>, Vec<String>) =
            locations.into_iter().partition(|l| !is_url(l) && popamp_skin::is_skin_archive(Path::new(l)));
        if let Some(skin) = skins.last() {
            self.install_skin(Path::new(skin));
        }
        if locations.is_empty() {
            return;
        }
        if let Some(first) = locations.iter().find(|l| !is_url(l)) {
            let p = PathBuf::from(first);
            self.last_dir = if p.is_dir() { Some(p) } else { p.parent().map(Path::to_path_buf) };
        }
        let events = self.events.clone();
        let (sources, cx) = (self.sources.clone(), self.source_context());
        let _ = std::thread::Builder::new().name("popamp-expand".into()).spawn(move || {
            let items = expand(&sources, locations, &cx);
            events.send(AppEvent::Expanded { items, mode });
        });
    }

    /// Like `add`, but expands the locations and reads their tags on this
    /// thread. For headless rendering, which has no event loop to wait on.
    pub fn add_now(&mut self, locations: Vec<String>, mode: AddMode) {
        if locations.is_empty() {
            return;
        }
        let items = expand(&self.sources, locations, &self.source_context());
        self.on_expanded(items, mode);
        let local: Vec<(u64, String)> =
            self.playlist.entries().iter().filter(|e| !e.is_url()).map(|e| (e.id, e.location.clone())).collect();
        for (id, path) in local {
            if let Some(res) = self.sources.of_now(&path).describe(&path) {
                self.on_meta(id, res);
            }
        }
    }

    pub fn on_expanded(&mut self, items: Vec<Item>, mode: AddMode) {
        let entries: Vec<Entry> = items.into_iter().map(Item::into_entry).collect();
        if entries.is_empty() {
            self.flash("NOTHING TO ADD");
            return;
        }
        for e in &entries {
            if wants_meta(e) {
                let _ = self.meta_tx.send((e.id, e.location.clone()));
            }
        }
        let first_id = entries[0].id;
        match mode {
            AddMode::ReplaceAndPlay => {
                self.engine.send(EngineCmd::Stop);
                self.playlist.clear();
                self.playlist.extend(entries);
            }
            AddMode::Append => self.playlist.extend(entries),
            AddMode::Insert(at) => self.playlist.insert(at, entries),
        }
        self.pl_dirty += 1;
        if mode == AddMode::ReplaceAndPlay {
            if let Some(i) = self.playlist.index_of_id(first_id) {
                self.play_index(i);
            }
        } else {
            self.update_next();
        }
    }

    pub fn on_meta(&mut self, id: u64, meta: Result<TrackMeta, String>) {
        let fmt = self.format.clone();
        if let Some(e) = self.playlist.find_mut_by_id(id) {
            match meta {
                Ok(m) => {
                    e.length_ms = m.length_ms.or(e.length_ms);
                    e.meta = Some(Arc::new(m));
                    e.refresh_display(&fmt);
                }
                Err(err) => log::debug!("no tags for {}: {err}", e.location),
            }
            self.pl_dirty += 1;
        }
    }

    pub fn set_title_format(&mut self, fmt: &str) {
        self.settings_mut().titlefmt = fmt.to_string();
        self.format = Format::cached(fmt);
        let f = self.format.clone();
        self.playlist.refresh_all_displays(&f);
        self.pl_dirty += 1;
    }

    pub fn save_playlist_file(&self, path: &Path) -> std::io::Result<()> {
        plformats::write_file(path, &plformats::items_from_entries(self.playlist.entries()))
    }

    pub fn load_session(&mut self) {
        let path = self.paths.playlist_file();
        if let Ok(items) = plformats::read_file(&path) {
            let entries: Vec<Entry> = items.into_iter().map(Item::into_entry).collect();
            for e in &entries {
                if wants_meta(e) {
                    let _ = self.meta_tx.send((e.id, e.location.clone()));
                }
            }
            self.playlist.extend(entries);
            let pos = self.settings().plpos.max(0) as usize;
            if pos < self.playlist.len() {
                self.playlist.set_current(Some(pos));
                self.playlist.cursor = pos;
            }
            self.pl_dirty += 1;
        }
    }

    pub fn save_session(&mut self) {
        self.settings_mut().plpos = self.playlist.current().unwrap_or(0) as i32;
        if let Err(e) = self.save_playlist_file(&self.paths.playlist_file()) {
            log::warn!("cannot save playlist: {e}");
        }
        if let Err(e) = self.config.save() {
            log::warn!("cannot save settings to {}: {e}", self.config.path().display());
        }
    }

    // --- playback -----------------------------------------------------------

    pub fn play_index(&mut self, idx: usize) {
        let Some(e) = self.playlist.entry(idx) else { return };
        let (id, loc) = (e.id, e.location.clone());
        self.playlist.set_current(Some(idx));
        self.playlist.cursor = idx;
        self.status.stream_title = None;
        self.status.stream_name = None;
        if self.sources.of_now(&loc).slow() {
            self.flash("CONNECTING…");
        }
        self.engine.send(EngineCmd::Open { id, location: loc, start_ms: 0 });
        self.pl_dirty += 1;
    }

    /// Tells the engine what to continue with when the current track ends.
    pub fn update_next(&mut self) {
        let s = self.settings();
        let (shuffle, repeat, adv) = (s.shuffle, s.repeat, s.pladv);
        if self.state() == PlayState::Stopped {
            return;
        }
        let next = if adv && !self.stop_after_current {
            self.playlist.next_index(shuffle, repeat).and_then(|i| self.playlist.entry(i)).map(|e| (e.id, e.location.clone()))
        } else {
            None
        };
        self.engine.send(EngineCmd::SetNext(next));
    }

    pub fn play(&mut self) {
        match self.state() {
            PlayState::Paused => self.engine.send(EngineCmd::Pause(false)),
            _ => {
                let idx = self.playlist.current().or(if self.playlist.is_empty() { None } else { Some(0) });
                match idx {
                    Some(i) => self.play_index(i),
                    None => self.request_dialog(DialogKind::OpenFiles { play: true }),
                }
            }
        }
    }

    pub fn pause_toggle(&mut self) {
        match self.state() {
            PlayState::Playing => self.engine.send(EngineCmd::Pause(true)),
            PlayState::Paused => self.engine.send(EngineCmd::Pause(false)),
            PlayState::Stopped => {}
        }
    }

    pub fn next(&mut self) {
        let s = self.settings();
        let (shuffle, repeat) = (s.shuffle, s.repeat);
        // Manual "next" always wraps, like the original.
        if let Some(i) = self.playlist.next_index(shuffle, true).or(if repeat { Some(0) } else { None }) {
            self.go_to(i);
        }
    }

    pub fn prev(&mut self) {
        let s = self.settings();
        let shuffle = s.shuffle;
        if let Some(i) = self.playlist.prev_index(shuffle, true) {
            self.go_to(i);
        }
    }

    /// Plays `i` if something is playing, otherwise just selects it.
    fn go_to(&mut self, i: usize) {
        if self.state() == PlayState::Stopped {
            self.playlist.set_current(Some(i));
            self.playlist.cursor = i;
            self.pl_dirty += 1;
        } else {
            self.play_index(i);
        }
    }

    pub fn seek_relative(&mut self, delta_ms: i64) {
        if self.state() == PlayState::Stopped {
            return;
        }
        let pos = self.engine.position_ms() as i64;
        let len = self.engine.shared().length_ms().map(|l| l as i64).unwrap_or(i64::MAX);
        self.engine.send(EngineCmd::Seek((pos + delta_ms).clamp(0, len.saturating_sub(500).max(0)) as u64));
    }

    pub fn seek_to(&mut self, ms: u64) {
        if self.state() != PlayState::Stopped {
            self.engine.send(EngineCmd::Seek(ms));
        }
    }

    pub fn set_volume(&mut self, v: i32) {
        let v = v.clamp(0, 255);
        self.settings_mut().volume = v;
        self.engine.send(EngineCmd::Volume(v as u8));
        self.flash(format!("VOLUME: {}%", (v * 100 + 127) / 255));
    }

    pub fn set_balance(&mut self, b: i32) {
        let b = b.clamp(-127, 127);
        self.settings_mut().balance = b;
        self.engine.send(EngineCmd::Balance(b as i8));
        let pct = (b.abs() * 100 + 63) / 127;
        self.flash(match b {
            0 => "BALANCE: CENTER".to_string(),
            b if b < 0 => format!("BALANCE: {pct}% LEFT"),
            _ => format!("BALANCE: {pct}% RIGHT"),
        });
    }

    pub fn on_engine(&mut self, ev: EngineEvent) {
        match ev {
            EngineEvent::Started { id, info, meta, stream } => self.track_started(id, info, meta, stream.and_then(|s| s.name)),
            EngineEvent::TrackChanged { id, info, meta } => {
                if let Some(i) = self.playlist.index_of_id(id) {
                    self.playlist.set_current(Some(i));
                    self.playlist.cursor = i;
                }
                self.track_started(id, info, meta, None)
            }
            EngineEvent::Finished { .. } => {
                self.stop_after_current = false;
                self.status.playing_id = None;
                // At the end of the list the cursor returns to the first entry.
                if !self.playlist.is_empty() && self.settings().pladv {
                    let last = self.playlist.current() == Some(self.playlist.len() - 1);
                    if last && !self.settings().repeat && !self.settings().shuffle {
                        self.playlist.set_current(Some(0));
                    }
                }
                self.pl_dirty += 1;
            }
            EngineEvent::Stopped => {
                self.status.playing_id = None;
            }
            EngineEvent::Error { id, message } => {
                self.flash(format!("ERROR: {message}"));
                self.status.playing_id = None;
                let len = self.playlist.len();
                if self.settings().pladv && self.error_skips < len && len > 1 {
                    self.error_skips += 1;
                    if let Some(i) = self.playlist.index_of_id(id) {
                        self.playlist.set_current(Some(i));
                        let (shuffle, repeat) = (self.settings().shuffle, self.settings().repeat);
                        if let Some(n) = self.playlist.next_index(shuffle, repeat) {
                            self.play_index(n);
                        }
                    }
                }
            }
            EngineEvent::StreamTitle { title, .. } => {
                self.status.stream_title = Some(title);
            }
        }
    }

    fn track_started(&mut self, id: u64, info: StreamInfo, meta: TrackMeta, stream_name: Option<String>) {
        self.error_skips = 0;
        self.status.playing_id = Some(id);
        self.status.info = Some(info.clone());
        if stream_name.is_some() {
            self.status.stream_name = stream_name;
        }
        let fmt = self.format.clone();
        let autoload = self.settings().autoload_eq;
        let mut file_name = None;
        if let Some(e) = self.playlist.find_mut_by_id(id) {
            if e.meta.is_none() || e.length_ms.is_none() {
                e.length_ms = info.length_ms.or(e.length_ms);
                if !e.is_url() || meta.title.is_some() {
                    e.meta = Some(Arc::new(meta));
                }
                e.refresh_display(&fmt);
            }
            file_name = Some(e.location.rsplit('/').next().unwrap_or("").to_string());
            let played = Played { at: history::now(), location: e.location.clone(), title: e.display.clone() };
            self.history.record(&self.paths.history_file(), played);
        }
        if autoload {
            if let Some(name) = file_name {
                if let Some(p) = self.eq_auto.iter().find(|p| p.name.eq_ignore_ascii_case(&name)).cloned() {
                    self.apply_preset(&p);
                }
            }
        }
        self.pl_dirty += 1;
        self.update_next();
    }

    pub fn current_title(&self) -> String {
        let Some(e) = self.playlist.current_entry() else { return String::new() };
        let title = match (&self.status.stream_title, self.status.playing_id == Some(e.id)) {
            (Some(t), true) => t.clone(),
            _ => e.display.clone(),
        };
        let idx = self.playlist.current().unwrap_or(0) + 1;
        match e.length_ms {
            Some(l) => format!("{idx}. {title} ({})", format_duration_ms(l)),
            None => format!("{idx}. {title}"),
        }
    }

    // --- equalizer ----------------------------------------------------------

    pub fn set_eq_band(&mut self, band: usize, value: i32) {
        let mut bands = self.settings().eq_bands();
        bands[band] = value.clamp(0, 63) as u8;
        self.settings_mut().set_eq_bands(&bands);
        self.push_eq();
        self.flash(format!("EQ: {:+.1} DB", eqf::value_to_db(bands[band])));
    }

    pub fn set_preamp(&mut self, value: i32) {
        self.settings_mut().preamp = value.clamp(0, 63);
        self.push_eq();
        self.flash(format!("PREAMP: {:+.1} DB", eqf::value_to_db(value.clamp(0, 63) as u8)));
    }

    pub fn apply_preset(&mut self, p: &Preset) {
        self.settings_mut().set_eq_bands(&p.bands);
        self.settings_mut().preamp = p.preamp as i32;
        self.push_eq();
    }

    pub fn current_preset(&self, name: &str) -> Preset {
        Preset { name: name.to_string(), bands: self.settings().eq_bands(), preamp: self.settings().preamp as u8 }
    }

    /// Plays a track from the history: from the playlist when it is there,
    /// else added to its end.
    pub fn play_again(&mut self, played: &Played) {
        let at = self.playlist.entries().iter().position(|e| e.location == played.location);
        let at = at.unwrap_or_else(|| {
            let item = Item { location: played.location.clone(), title: Some(played.title.clone()), length_ms: None };
            self.on_expanded(vec![item], AddMode::Append);
            self.playlist.len() - 1
        });
        self.play_index(at);
    }

    // --- bookmarks ---------------------------------------------------------------

    fn add_bookmarks(&mut self, entries: &[Entry]) {
        if entries.is_empty() {
            return;
        }
        self.bookmarks.extend(plformats::items_from_entries(entries));
        self.save_bookmarks();
        self.flash(if entries.len() == 1 { "BOOKMARK ADDED".to_string() } else { format!("{} BOOKMARKS ADDED", entries.len()) });
    }

    pub fn remove_bookmark(&mut self, i: usize) {
        if i < self.bookmarks.len() {
            self.bookmarks.remove(i);
            self.save_bookmarks();
        }
    }

    fn save_bookmarks(&self) {
        if let Err(e) = plformats::write_file(&self.paths.bookmarks_file(), &self.bookmarks) {
            log::warn!("cannot save bookmarks: {e}");
        }
    }

    /// "Bookmarks" in the main menu: managing them, then one item each.
    pub fn bookmarks_menu(&self) -> Vec<MenuItem> {
        use Command as C;
        let mut v = vec![
            MenuItem::cmd("Add current as bookmark", "Alt+I", C::AddBookmark),
            MenuItem::cmd("Remove bookmark…", "Ctrl+Alt+I", C::RemoveBookmark),
        ];
        if !self.bookmarks.is_empty() {
            v.push(MenuItem::sep());
            v.extend(self.bookmarks.iter().enumerate().map(|(i, b)| MenuItem::action(&bookmark_label(b), "", MenuAction::PlayBookmark(i))));
        }
        v
    }

    fn save_eq_libraries(&self) {
        if let Err(e) = eqf::write_file(&self.paths.eq_library(), &self.eq_presets) {
            log::warn!("cannot save EQ presets: {e}");
        }
        if let Err(e) = eqf::write_file(&self.paths.eq_auto_library(), &self.eq_auto) {
            log::warn!("cannot save EQ auto presets: {e}");
        }
    }

    // --- skins ---------------------------------------------------------------

    pub fn set_skin(&mut self, path: Option<PathBuf>) {
        match &path {
            None => self.skin = Skin::builtin_vector(),
            Some(p) => match self.load_skin(p) {
                Ok(s) => self.skin = s,
                Err(e) => {
                    self.flash(format!("SKIN ERROR: {e}"));
                    return;
                }
            },
        }
        self.settings_mut().skin = path.as_ref().map(|p| p.to_string_lossy().into_owned()).unwrap_or_default();
        self.skin_changed_at = path.as_deref().and_then(popamp_skin::changed_at);
        self.skin_path = path;
        self.ui.push(UiReq::SkinChanged);
    }

    /// Loads the skin again when its files have changed since, so a skin
    /// author sees each edit; a broken edit keeps the skin on screen.
    pub fn reload_skin_if_changed(&mut self) {
        let Some(path) = self.skin_path.clone() else { return };
        let now = popamp_skin::changed_at(&path);
        if now == self.skin_changed_at {
            return;
        }
        self.skin_changed_at = now;
        self.reload_skin(&path);
    }

    /// Loads the skin with the colour set the user picked.
    pub fn load_skin(&self, path: &std::path::Path) -> Result<Skin, String> {
        Skin::load(path, Some(&self.settings().skin_colors))
    }

    fn reload_skin(&mut self, path: &std::path::Path) {
        match self.load_skin(path) {
            Ok(s) => {
                self.skin = s;
                self.ui.push(UiReq::SkinChanged);
            }
            Err(e) => {
                log::warn!("{e}");
                self.flash(format!("SKIN ERROR: {e}"));
            }
        }
    }

    // --- dialogs ---------------------------------------------------------------

    fn start_dir(&self) -> Option<PathBuf> {
        self.last_dir.clone().or_else(|| {
            std::env::home_dir().map(|h| {
                let music = h.join("Music");
                if music.is_dir() {
                    music
                } else {
                    h
                }
            })
        })
    }

    pub fn request_dialog(&mut self, kind: DialogKind) {
        let own = popamp_core::audio_extensions().chain(["m3u", "m3u8", "pls", "xspf", "cue"]).map(|e| format!("*.{e}"));
        let media_filter: Vec<String> = own.chain(self.decoders.extensions().iter().map(|e| format!("*.{e}"))).collect();
        let start = self.start_dir();
        let req = match &kind {
            DialogKind::OpenFiles { .. } => Request::Files { title: "Open file(s)".into(), multiple: true, filter: media_filter, start },
            DialogKind::OpenFolder { .. } => Request::Folder { title: "Open folder".into(), start },
            DialogKind::LibraryFolder => Request::Folder { title: "Add a folder to the media library".into(), start },
            DialogKind::OpenUrl { .. } => Request::Entry {
                title: "Open URL".into(),
                text: "Enter a URL to open (a stream or a playlist):".into(),
                initial: String::new(),
            },
            DialogKind::LoadPlaylist => Request::Files {
                title: "Load playlist".into(),
                multiple: false,
                filter: ["*.m3u", "*.m3u8", "*.pls", "*.xspf"].map(String::from).to_vec(),
                start,
            },
            DialogKind::SavePlaylist => Request::Save { title: "Save playlist".into(), name: "playlist.m3u8".into(), start },
            DialogKind::SaveHtml => Request::Save { title: "Generate HTML playlist".into(), name: "playlist.html".into(), start },
            DialogKind::JumpToTime => {
                let len = self.engine.shared().length_ms().map(format_duration_ms).unwrap_or_else(|| "?".into());
                Request::Entry {
                    title: "Jump to time".into(),
                    text: format!("Jump to (m:ss), track length {len}:"),
                    initial: String::new(),
                }
            }
            DialogKind::EditEntry { id } => {
                let loc =
                    self.playlist.index_of_id(*id).and_then(|i| self.playlist.entry(i)).map(|e| e.location.clone()).unwrap_or_default();
                Request::Entry { title: "Edit playlist entry".into(), text: "Location:".into(), initial: loc }
            }
            DialogKind::RemoveBookmark => Request::List {
                title: "Remove bookmark".into(),
                column: "Bookmark".into(),
                rows: self.bookmarks.iter().map(bookmark_label).collect(),
            },
            DialogKind::EqLoadPreset { auto } | DialogKind::EqDeletePreset { auto } => {
                let list = if *auto { &self.eq_auto } else { &self.eq_presets };
                Request::List { title: "EQ presets".into(), column: "Preset".into(), rows: list.iter().map(|p| p.name.clone()).collect() }
            }
            DialogKind::EqSavePreset { auto } => {
                let initial = if *auto {
                    self.playlist.current_entry().map(|e| e.location.rsplit('/').next().unwrap_or("").to_string()).unwrap_or_default()
                } else {
                    String::new()
                };
                Request::Entry { title: "Save EQ preset".into(), text: "Preset name:".into(), initial }
            }
            DialogKind::EqImport => {
                Request::Files { title: "Load EQ preset file".into(), multiple: false, filter: vec!["*.eqf".into(), "*.q1".into()], start }
            }
            DialogKind::EqExport => Request::Save { title: "Save EQ preset file".into(), name: "preset.eqf".into(), start },
            DialogKind::InstallSkin => Request::Files {
                title: "Install skin".into(),
                multiple: false,
                filter: popamp_skin::ARCHIVE_EXTENSIONS.iter().chain(&["zip"]).map(|e| format!("*.{e}")).collect(),
                start,
            },
            // A message carries its own text: see `C::About`.
            DialogKind::Info => return,
            DialogKind::PickCover { dir } => Request::Files {
                title: "Choose a cover picture".into(),
                multiple: false,
                filter: ["jpg", "jpeg", "png", "gif", "bmp"].iter().map(|e| format!("*.{e}")).collect(),
                start: dir.clone(),
            },
            DialogKind::ConfirmPhysicalRemove { paths } => Request::Question {
                title: "Delete files".into(),
                text: format!(
                    "Permanently delete {} file(s) from disk?\n\n{}",
                    paths.len(),
                    paths.iter().take(10).cloned().collect::<Vec<_>>().join("\n")
                ),
            },
        };
        self.ui.push(UiReq::Dialog(kind, req));
    }

    /// Name/value rows describing the selected (or else the playing) entry.
    /// The entry File info is about: the first selected one, else the
    /// current one.
    pub fn info_entry(&self) -> Option<&Entry> {
        let idx = self.playlist.selected_indices().first().copied().or(self.playlist.current());
        idx.and_then(|i| self.playlist.entry(i))
    }

    /// Popamp changed the tags of the file at `location`: its entries read
    /// them again, and covers are read again.
    pub fn tags_changed(&mut self, location: &str) {
        self.tags_generation += 1;
        let fmt = self.format.clone();
        let ids: Vec<u64> = self.playlist.entries().iter().filter(|e| e.location == location).map(|e| e.id).collect();
        for id in ids {
            if let Some(e) = self.playlist.find_mut_by_id(id) {
                e.meta = None;
                e.refresh_display(&fmt);
                let _ = self.meta_tx.send((e.id, e.location.clone()));
            }
        }
        self.pl_dirty += 1;
    }

    pub fn file_info_fields(&self) -> Vec<(String, String)> {
        let Some(e) = self.info_entry() else { return vec![("".into(), "No file selected.".into())] };
        let mut rows = vec![("Location".to_string(), e.location.clone())];
        match (&e.meta, &e.file_title) {
            (Some(m), _) => rows.extend(meta_rows(m)),
            (None, Some(t)) => rows.push(("Title".into(), t.clone())),
            (None, None) => {}
        }
        let mut push = |k: &str, v: String| rows.push((k.to_string(), v));
        if self.status.playing_id == Some(e.id) {
            if let Some(n) = &self.status.stream_name {
                push("Station", n.clone());
            }
            if let Some(t) = &self.status.stream_title {
                push("Now playing", t.clone());
            }
        }
        rows
    }

    pub fn on_dialog(&mut self, r: DialogResult) {
        let first = r.values.first().cloned();
        match r.kind {
            DialogKind::OpenFiles { play } | DialogKind::OpenFolder { play } | DialogKind::OpenUrl { play } => {
                let vals: Vec<String> = r.values.into_iter().map(|v| v.trim().to_string()).filter(|v| !v.is_empty()).collect();
                self.add(vals, if play { AddMode::ReplaceAndPlay } else { AddMode::Append });
            }
            DialogKind::LoadPlaylist => {
                if let Some(f) = first {
                    self.add(vec![f], AddMode::ReplaceAndPlay);
                }
            }
            DialogKind::SavePlaylist => {
                if let Some(f) = first {
                    let mut p = PathBuf::from(f);
                    if plformats::PlaylistFormat::from_path(&p).is_none() {
                        p.set_extension("m3u8");
                    }
                    if let Err(e) = self.save_playlist_file(&p) {
                        self.flash(format!("SAVE FAILED: {e}"));
                    }
                }
            }
            DialogKind::SaveHtml => {
                if let Some(f) = first {
                    let mut p = PathBuf::from(f);
                    if p.extension().is_none() {
                        p.set_extension("html");
                    }
                    let html = plformats::html(&plformats::items_from_entries(self.playlist.entries()));
                    if let Err(e) = std::fs::write(&p, html) {
                        self.flash(format!("SAVE FAILED: {e}"));
                    }
                }
            }
            DialogKind::JumpToTime => {
                if let Some(ms) = first.as_deref().and_then(crate::dialogs::parse_time) {
                    self.seek_to(ms);
                }
            }
            DialogKind::EditEntry { id } => {
                if let Some(loc) = first.filter(|l| !l.trim().is_empty()) {
                    let fmt = self.format.clone();
                    if let Some(e) = self.playlist.find_mut_by_id(id) {
                        e.location = loc.trim().to_string();
                        e.meta = None;
                        e.file_title = None;
                        e.refresh_display(&fmt);
                        if wants_meta(e) {
                            let _ = self.meta_tx.send((e.id, e.location.clone()));
                        }
                    }
                    self.pl_dirty += 1;
                }
            }
            DialogKind::EqLoadPreset { auto } => {
                let list = if auto { &self.eq_auto } else { &self.eq_presets };
                if let Some(p) = first.and_then(|s| s.trim().parse::<usize>().ok()).and_then(|i| list.get(i)).cloned() {
                    self.apply_preset(&p);
                }
            }
            DialogKind::RemoveBookmark => {
                if let Some(i) = first.and_then(|s| s.trim().parse::<usize>().ok()) {
                    self.remove_bookmark(i);
                }
            }
            DialogKind::EqDeletePreset { auto } => {
                if let Some(i) = first.and_then(|s| s.trim().parse::<usize>().ok()) {
                    let list = if auto { &mut self.eq_auto } else { &mut self.eq_presets };
                    if i < list.len() {
                        list.remove(i);
                        self.save_eq_libraries();
                    }
                }
            }
            DialogKind::EqSavePreset { auto } => {
                if let Some(name) = first.map(|s| s.trim().to_string()).filter(|s| !s.is_empty()) {
                    let preset = self.current_preset(&name);
                    let list = if auto { &mut self.eq_auto } else { &mut self.eq_presets };
                    match list.iter_mut().find(|p| p.name.eq_ignore_ascii_case(&name)) {
                        Some(p) => *p = preset,
                        None => list.push(preset),
                    }
                    self.save_eq_libraries();
                }
            }
            DialogKind::EqImport => {
                if let Some(f) = first {
                    match eqf::read_file(Path::new(&f)) {
                        Ok(presets) if !presets.is_empty() => self.apply_preset(&presets[0]),
                        Ok(_) => self.flash("EMPTY EQ FILE"),
                        Err(e) => self.flash(format!("EQ FILE ERROR: {e}")),
                    }
                }
            }
            DialogKind::EqExport => {
                if let Some(f) = first {
                    let mut p = PathBuf::from(f);
                    if p.extension().is_none() {
                        p.set_extension("eqf");
                    }
                    let name = p.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
                    if let Err(e) = eqf::write_file(&p, &[self.current_preset(&name)]) {
                        self.flash(format!("SAVE FAILED: {e}"));
                    }
                }
            }
            DialogKind::InstallSkin => {
                if let Some(f) = first {
                    self.install_skin(Path::new(&f));
                }
            }
            DialogKind::LibraryFolder => {
                if let Some(f) = first.map(|f| f.trim().to_string()).filter(|f| !f.is_empty()) {
                    self.library_add_folder(&f);
                }
            }
            DialogKind::Info => {}
            // The File info window takes the picture (app.rs).
            DialogKind::PickCover { .. } => {}
            DialogKind::ConfirmPhysicalRemove { paths } => {
                if first.as_deref() == Some("yes") {
                    for p in &paths {
                        if let Err(e) = std::fs::remove_file(p) {
                            log::warn!("cannot delete {p}: {e}");
                        }
                    }
                    self.playlist.remove_selected();
                    self.pl_dirty += 1;
                }
            }
        }
    }

    pub fn install_skin(&mut self, src: &Path) {
        let dir = self.paths.skins_dir();
        let Some(name) = src.file_name() else { return };
        let dest = dir.join(name);
        let res = std::fs::create_dir_all(&dir).and_then(|_| if src != dest { std::fs::copy(src, &dest).map(|_| ()) } else { Ok(()) });
        match res {
            Ok(()) => self.set_skin(Some(dest)),
            Err(e) => self.flash(format!("INSTALL FAILED: {e}")),
        }
    }

    // --- commands ---------------------------------------------------------------

    pub fn exec(&mut self, cmd: Command) {
        use Command as C;
        match cmd {
            C::Exit => {
                self.quit = true;
                self.ui.push(UiReq::Quit);
            }
            C::Play => self.play(),
            C::Pause => self.pause_toggle(),
            C::PlayPause => match self.state() {
                PlayState::Stopped => self.play(),
                _ => self.pause_toggle(),
            },
            C::Stop => self.engine.send(EngineCmd::Stop),
            C::StopFadeout => self.engine.send(EngineCmd::StopFade),
            C::StopAfterCurrent => {
                self.stop_after_current = !self.stop_after_current;
                self.flash(if self.stop_after_current { "STOP AFTER CURRENT: ON" } else { "STOP AFTER CURRENT: OFF" });
                self.update_next();
            }
            C::Next => self.next(),
            C::Previous => self.prev(),
            C::Back5s | C::Rewind5s => self.seek_relative(-5000),
            C::Fwd5s | C::Forward5s => self.seek_relative(5000),
            C::StartOfList => {
                if !self.playlist.is_empty() {
                    self.go_to(0)
                }
            }
            C::EndOfList => {
                if !self.playlist.is_empty() {
                    self.go_to(self.playlist.len() - 1)
                }
            }
            C::TenTracksBack | C::TenTracksForward => {
                if !self.playlist.is_empty() {
                    let cur = self.playlist.current().unwrap_or(0) as i64;
                    let d = if cmd == C::TenTracksBack { -10 } else { 10 };
                    self.go_to((cur + d).clamp(0, self.playlist.len() as i64 - 1) as usize);
                }
            }
            C::VolumeUp => self.set_volume(self.settings().volume + 5),
            C::VolumeDown => self.set_volume(self.settings().volume - 5),
            C::BalanceLeft => self.set_balance(self.settings().balance - 8),
            C::BalanceRight => self.set_balance(self.settings().balance + 8),
            C::ToggleRepeat => {
                let v = !self.settings().repeat;
                self.settings_mut().repeat = v;
                self.update_next();
            }
            C::ToggleShuffle => {
                let v = !self.settings().shuffle;
                self.settings_mut().shuffle = v;
                self.update_next();
            }
            C::ToggleManualAdvance => {
                let v = !self.settings().pladv;
                self.settings_mut().pladv = v;
                self.flash(if v { "MANUAL ADVANCE: OFF" } else { "MANUAL ADVANCE: ON" });
                self.update_next();
            }
            C::OpenFile => self.request_dialog(DialogKind::OpenFiles { play: true }),
            C::OpenFolder => self.request_dialog(DialogKind::OpenFolder { play: true }),
            C::OpenUrl => self.request_dialog(DialogKind::OpenUrl { play: true }),
            C::PlAddFiles => self.request_dialog(DialogKind::OpenFiles { play: false }),
            C::PlAddFolder => self.request_dialog(DialogKind::OpenFolder { play: false }),
            C::PlAddUrl => self.request_dialog(DialogKind::OpenUrl { play: false }),
            C::PlaylistOpen => self.request_dialog(DialogKind::LoadPlaylist),
            C::PlaylistSave => self.request_dialog(DialogKind::SavePlaylist),
            C::JumpToTime => self.request_dialog(DialogKind::JumpToTime),
            C::JumpToFile => self.ui.push(UiReq::Panel(PanelKind::JumpToFile)),
            C::FileInfo => self.ui.push(UiReq::Panel(PanelKind::FileInfo)),
            C::PlGenerateHtml => self.request_dialog(DialogKind::SaveHtml),
            C::EqLoadDefault => {
                let p = self.settings().eq_default();
                self.apply_preset(&p);
            }
            C::EqSaveDefault => {
                let p = self.current_preset("Default");
                self.settings_mut().set_eq_default(&p);
            }
            C::AddBookmark => {
                let current = self.playlist.current_entry().into_iter().cloned().collect::<Vec<_>>();
                self.add_bookmarks(&current);
            }
            C::BookmarkSelected => {
                let selected: Vec<Entry> = self.playlist.entries().iter().filter(|e| e.selected).cloned().collect();
                self.add_bookmarks(&selected);
            }
            C::RemoveBookmark => self.request_dialog(DialogKind::RemoveBookmark),
            C::EditPlaylistEntry => {
                let idx = self.playlist.selected_indices().first().copied().or(self.playlist.current());
                if let Some(id) = idx.and_then(|i| self.playlist.entry(i)).map(|e| e.id) {
                    self.request_dialog(DialogKind::EditEntry { id });
                }
            }
            C::SkinBrowser => self.ui.push(UiReq::Panel(PanelKind::SkinBrowser)),
            C::SkinMuseum => self.ui.push(UiReq::Panel(PanelKind::SkinMuseum)),
            C::Gallery => self.ui.push(UiReq::Panel(PanelKind::Gallery)),
            C::AlbumArt => self.ui.push(UiReq::Panel(PanelKind::AlbumArt)),
            C::History => self.ui.push(UiReq::Panel(PanelKind::History)),
            C::ScanReplayGain => self.scan_replaygain(),
            C::ToggleLibraryWindow => {
                let v = !self.settings().ml_open;
                self.settings_mut().ml_open = v;
                self.ui.push(UiReq::Layout);
            }
            C::LibraryPlay => self.library_play(AddMode::ReplaceAndPlay),
            C::LibraryEnqueue => self.library_play(AddMode::Append),
            C::LibraryOpen => self.exec(self.library_open(false)),
            C::LibraryOpenOther => self.exec(self.library_open(true)),
            C::LibraryAddFolder => self.request_dialog(DialogKind::LibraryFolder),
            C::LibraryRescan => self.library_rescan(),
            C::LibraryRemoveFolder => self.library_remove_folder(),
            #[cfg(feature = "plugins")]
            C::Plugins => self.ui.push(UiReq::Panel(PanelKind::Plugins)),
            #[cfg(not(feature = "plugins"))]
            C::Plugins => {}
            C::EqPresetLoad => self.request_dialog(DialogKind::EqLoadPreset { auto: false }),
            C::EqPresetSave => self.request_dialog(DialogKind::EqSavePreset { auto: false }),
            C::EqPresetDelete => self.request_dialog(DialogKind::EqDeletePreset { auto: false }),
            C::EqImportEqf => self.request_dialog(DialogKind::EqImport),
            C::EqExportEqf => self.request_dialog(DialogKind::EqExport),
            C::EqReset => {
                self.settings_mut().set_eq_bands(&[31; 10]);
                self.settings_mut().preamp = 31;
                self.push_eq();
            }
            C::ToggleEq => {
                let v = !self.settings().use_eq;
                self.settings_mut().use_eq = v;
                self.push_eq();
            }
            C::ToggleEqAutoload => {
                let v = !self.settings().autoload_eq;
                self.settings_mut().autoload_eq = v;
            }
            C::ClearPlaylist => {
                self.playlist.clear();
                self.pl_dirty += 1;
                self.update_next();
            }
            C::PlRemoveSelected => {
                self.playlist.remove_selected();
                self.pl_dirty += 1;
                self.update_next();
            }
            C::PlCropSelected => {
                self.playlist.crop_selected();
                self.pl_dirty += 1;
                self.update_next();
            }
            C::RemoveMissing => {
                self.playlist.remove_missing();
                self.pl_dirty += 1;
            }
            C::PhysicallyRemove => {
                let paths: Vec<String> = self
                    .playlist
                    .selected_indices()
                    .iter()
                    .filter_map(|&i| self.playlist.entry(i))
                    .filter(|e| !e.is_url())
                    .map(|e| e.location.clone())
                    .collect();
                if !paths.is_empty() {
                    self.request_dialog(DialogKind::ConfirmPhysicalRemove { paths });
                }
            }
            C::SelectAll => {
                self.playlist.select_all(true);
                self.pl_dirty += 1;
            }
            C::SelectNone => {
                self.playlist.select_all(false);
                self.pl_dirty += 1;
            }
            C::InvertSelection => {
                self.playlist.invert_selection();
                self.pl_dirty += 1;
            }
            C::SortByTitle | C::SortByFileName | C::SortByPath => {
                self.playlist.sort(match cmd {
                    C::SortByTitle => SortKey::Title,
                    C::SortByFileName => SortKey::FileName,
                    _ => SortKey::Path,
                });
                self.pl_dirty += 1;
                self.update_next();
            }
            C::ReverseList => {
                self.playlist.reverse();
                self.pl_dirty += 1;
                self.update_next();
            }
            C::RandomizeList => {
                self.playlist.randomize();
                self.pl_dirty += 1;
                self.update_next();
            }
            C::MoveSelectionUp | C::MoveSelectionDown => {
                self.playlist.move_selected(if cmd == C::MoveSelectionUp { -1 } else { 1 });
                self.pl_dirty += 1;
                self.update_next();
            }
            C::PlaySelected => {
                let idx = self.playlist.selected_indices().first().copied().unwrap_or(self.playlist.cursor);
                self.play_index(idx);
            }
            C::TimeElapsed => self.settings_mut().timeleftmode = false,
            C::TimeRemaining => self.settings_mut().timeleftmode = true,
            C::ToggleTimeMode => {
                let v = !self.settings().timeleftmode;
                self.settings_mut().timeleftmode = v;
            }
            C::ToggleAutoscroll => {
                let v = !self.settings().autoscrollname;
                self.settings_mut().autoscrollname = v;
            }
            C::CycleVisMode => {
                let v = (self.settings().sa + 1) % 3;
                self.settings_mut().sa = v;
            }
            C::ToggleAlwaysOnTop => {
                let v = !self.settings().aot;
                self.settings_mut().aot = v;
                self.ui.push(UiReq::AlwaysOnTop);
            }
            C::ToggleDoubleSize => {
                let v = !self.settings().dsize;
                self.settings_mut().dsize = v;
                self.ui.push(UiReq::Layout);
            }
            C::ToggleEasyMove => {
                let v = !self.settings().easymove;
                self.settings_mut().easymove = v;
            }
            C::ToggleWindowshade => {
                let v = !self.settings().windowshade;
                self.settings_mut().windowshade = v;
                self.ui.push(UiReq::Layout);
            }
            C::ToggleEqShade => {
                let v = !self.settings().eq_ws;
                self.settings_mut().eq_ws = v;
                self.ui.push(UiReq::Layout);
            }
            C::TogglePlShade => {
                let v = !self.settings().pe_ws;
                self.settings_mut().pe_ws = v;
                self.ui.push(UiReq::Layout);
            }
            C::ToggleEqWindow => {
                let v = !self.settings().eq_open;
                self.settings_mut().eq_open = v;
                self.ui.push(UiReq::Layout);
            }
            C::TogglePlaylistWindow => {
                let v = !self.settings().pe_open;
                self.settings_mut().pe_open = v;
                self.ui.push(UiReq::Layout);
            }
            C::ToggleMainWindow => self.raise(),
            C::Minimize if self.settings().minimize_to_tray => {
                self.in_tray = true;
                self.ui.push(UiReq::Layout);
            }
            C::Minimize => self.ui.push(UiReq::Minimize),
            C::Preferences => self.ui.push(UiReq::Panel(PanelKind::Preferences)),
            C::SelectVisPlugin => {
                self.ui.push(UiReq::Menu(self.options_menu()));
            }
            C::ToggleVis => self.exec(C::CycleVisMode),
            C::About => {
                let text = format!(
                    "{}\n\nA Winamp-style audio player for Linux, written in Rust.\nDecoding by Symphonia. Classic (.wsz) and vector (.psz) skins.\n\nConfig: {}",
                    crate::edition::title(),
                    self.config.path().display()
                );
                self.ui.push(UiReq::Dialog(DialogKind::Info, Request::Info { title: format!("About {}", crate::edition::NAME), text }));
            }
        }
    }

    pub fn on_menu(&mut self, action: MenuAction) {
        match action {
            MenuAction::Cmd(c) => self.exec(c),
            MenuAction::Skin(p) => self.set_skin(p),
            MenuAction::VisMode(m) => self.settings_mut().sa = m,
            #[cfg(feature = "visualizers")]
            MenuAction::Visualizer(id) => self.ui.push(UiReq::Visualizer(id)),
            MenuAction::SaStyle(s) => self.settings_mut().sa_style = s,
            MenuAction::OscStyle(s) => self.settings_mut().osc_style = s,
            MenuAction::SaPeaks => {
                let v = !self.settings().sa_peaks;
                self.settings_mut().sa_peaks = v;
            }
            MenuAction::SaThick => {
                let v = !self.settings().sa_thick;
                self.settings_mut().sa_thick = v;
            }
            MenuAction::EqBands(iso) => {
                self.settings_mut().eq_frequencies = iso as i32;
                self.push_eq();
            }
            MenuAction::EqLimiter => {
                let v = !self.settings().eq_limiter;
                self.settings_mut().eq_limiter = v;
                self.push_eq();
            }
            MenuAction::Zoom(pct) => {
                let s = self.settings_mut();
                s.dsize = pct > 100;
                if s.dsize {
                    s.zoom = pct;
                }
                self.ui.push(UiReq::Layout);
            }
            MenuAction::Scaling(mode) => {
                self.settings_mut().wsz_scaling = mode;
                self.ui.push(UiReq::Layout);
            }
            MenuAction::UpdateDownload => self.update_download(),
            MenuAction::UpdateSkip => self.update_skip(),
            MenuAction::SkinColors(set) => {
                self.settings_mut().skin_colors = set;
                // The pictures are read with their colours, so read them again.
                if let Some(path) = self.skin_path.clone() {
                    self.reload_skin(&path);
                }
            }
            MenuAction::MainAsBar(bar) => {
                self.settings_mut().main_bar = bar;
                // A bar is another kind of window to the desktop.
                self.ui.push(UiReq::SkinChanged);
            }
            MenuAction::BarEdge(edge) => {
                self.settings_mut().bar_edge = Some(edge);
                self.ui.push(UiReq::Layout);
            }
            MenuAction::BarZoom(pct) => {
                self.settings_mut().bar_zoom = pct;
                self.ui.push(UiReq::Layout);
            }
            MenuAction::ReplayGain(mode) => {
                let s = self.settings_mut();
                match mode {
                    None => s.replaygain = false,
                    Some(m) => {
                        s.replaygain = true;
                        s.replaygain_mode = m;
                    }
                }
                self.push_engine_settings();
            }
            MenuAction::TitleFormat(f) => self.set_title_format(&f),
            MenuAction::EqPreset(i, auto) => {
                let list = if auto { &self.eq_auto } else { &self.eq_presets };
                if let Some(p) = list.get(i).cloned() {
                    self.apply_preset(&p);
                }
            }
            MenuAction::EqDialog(kind) => self.request_dialog(kind),
            MenuAction::PlaySelected => self.exec(Command::PlaySelected),
            MenuAction::PlayBookmark(i) => {
                if let Some(b) = self.bookmarks.get(i) {
                    self.add(vec![b.location.clone()], AddMode::ReplaceAndPlay);
                }
            }
        }
    }

    // --- menus -------------------------------------------------------------------

    pub fn main_menu(&self) -> Vec<MenuItem> {
        use Command as C;
        let s = self.settings();
        let st = self.state();
        let mut items: Vec<MenuItem> = self.update_menu().into_iter().collect();
        items.extend([
            MenuItem::cmd("About Popamp…", "Ctrl+F1", C::About),
            MenuItem::sep(),
            MenuItem::sub(
                "Play",
                vec![
                    MenuItem::cmd("File…", "L", C::OpenFile),
                    MenuItem::cmd("URL…", "Ctrl+L", C::OpenUrl),
                    MenuItem::cmd("Folder…", "Shift+L", C::OpenFolder),
                ],
            ),
            MenuItem::cmd("View file info…", "Alt+3", C::FileInfo),
            MenuItem::cmd("Album art", "Alt+A", C::AlbumArt),
            MenuItem::cmd("History…", "Ctrl+H", C::History),
            MenuItem::sub("Bookmarks", self.bookmarks_menu()),
            MenuItem::sep(),
            MenuItem::cmd("Main Window", "Alt+W", C::ToggleMainWindow),
            MenuItem::cmd("Playlist Editor", "Alt+E", C::TogglePlaylistWindow).checked(s.pe_open),
            MenuItem::cmd("Equalizer", "Alt+G", C::ToggleEqWindow).checked(s.eq_open),
            MenuItem::cmd("Media Library", "Alt+L", C::ToggleLibraryWindow).checked(s.ml_open),
        ]);
        if self.skin.color_sets.len() > 1 {
            let set = |(i, name): (usize, &String)| {
                MenuItem::action(name, "", MenuAction::SkinColors(name.clone())).checked(i == self.skin.color_set)
            };
            items.push(MenuItem::sub("Colours", self.skin.color_sets.iter().enumerate().map(set).collect()));
        }
        if let Some(edge) = crate::bar::chosen_edge(self) {
            let mut mode = Vec::new();
            // A skin with a window too lets the bar be turned off.
            if self.skin.layout.window(WindowKey::Main).is_some_and(|w| w.bar.is_some()) {
                let on = |label: &str, bar: bool| MenuItem::action(label, "", MenuAction::MainAsBar(bar)).checked(s.main_bar == bar);
                mode.extend([on("On", true), on("Off", false), MenuItem::sep()]);
            }
            let at = |label: &str, e: BarEdge| MenuItem::action(label, "", MenuAction::BarEdge(e)).checked(edge == e);
            mode.push(MenuItem::sub("Position", vec![at("Top of the screen", BarEdge::Top), at("Bottom of the screen", BarEdge::Bottom)]));
            mode.push(MenuItem::sub("Size", zooms(s.bar_zoom, MenuAction::BarZoom)));
            items.push(MenuItem::sub("Toolbar mode", mode));
        }
        items.extend([
            MenuItem::sep(),
            MenuItem::sub("Options", self.options_menu()),
            MenuItem::sub(
                "Playback",
                vec![
                    MenuItem::cmd("Previous", "Z", C::Previous),
                    MenuItem::cmd("Play", "X", C::Play),
                    MenuItem::cmd("Pause", "C", C::Pause).checked(st == PlayState::Paused),
                    MenuItem::cmd("Stop", "V", C::Stop),
                    MenuItem::cmd("Next", "B", C::Next),
                    MenuItem::sep(),
                    MenuItem::cmd("Stop w/ fadeout", "Shift+V", C::StopFadeout),
                    MenuItem::cmd("Stop after current", "Ctrl+V", C::StopAfterCurrent).checked(self.stop_after_current),
                    MenuItem::cmd("Back 5 seconds", "Left", C::Back5s),
                    MenuItem::cmd("Fwd 5 seconds", "Right", C::Fwd5s),
                    MenuItem::cmd("Start of list", "Ctrl+Z", C::StartOfList),
                    MenuItem::cmd("End of list", "Ctrl+B", C::EndOfList),
                    MenuItem::cmd("10 tracks back", "Num 1", C::TenTracksBack),
                    MenuItem::cmd("10 tracks fwd", "Num 3", C::TenTracksForward),
                    MenuItem::sep(),
                    MenuItem::cmd("Jump to time…", "Ctrl+J", C::JumpToTime),
                    MenuItem::cmd("Jump to file…", "J", C::JumpToFile),
                ],
            ),
            MenuItem::sub("Visualization", self.vis_menu()),
            MenuItem::sub("Skins", self.skins_menu()),
            MenuItem::sep(),
            MenuItem::cmd("Exit", "Alt+F4", C::Exit),
        ]);
        items
    }

    /// The window sizes and how classic skins are scaled to them.
    fn size_menu(&self) -> Vec<MenuItem> {
        use popamp_core::config::Scaling;
        let s = self.settings();
        let mut items = zooms(if s.dsize { s.zoom } else { 100 }, MenuAction::Zoom);
        items.push(MenuItem::sep());
        let modes = [(Scaling::Sharp, "Sharp scaling"), (Scaling::Integer, "Whole multiples only"), (Scaling::Nearest, "Nearest pixel")];
        items.extend(modes.map(|(m, label)| MenuItem::action(label, "", MenuAction::Scaling(m)).checked(s.wsz_scaling == m)));
        items
    }

    pub fn options_menu(&self) -> Vec<MenuItem> {
        use Command as C;
        let s = self.settings();
        let fmt = |label: &str, f: &str| MenuItem::action(label, "", MenuAction::TitleFormat(f.to_string())).checked(s.titlefmt == f);
        vec![
            MenuItem::cmd("Preferences…", "Ctrl+P", C::Preferences),
            MenuItem::cmd("Skin browser…", "Alt+S", C::SkinBrowser),
            #[cfg(feature = "plugins")]
            MenuItem::cmd("Plug-ins…", "", C::Plugins),
            MenuItem::cmd("Gallery (popamp.app)…", "", C::Gallery),
            MenuItem::sep(),
            MenuItem::cmd("Time elapsed", "Ctrl+T", C::TimeElapsed).checked(!s.timeleftmode),
            MenuItem::cmd("Time remaining", "Ctrl+T", C::TimeRemaining).checked(s.timeleftmode),
            MenuItem::sep(),
            MenuItem::cmd("Always On Top", "Ctrl+A", C::ToggleAlwaysOnTop).checked(s.aot),
            MenuItem::cmd("Double Size", "Ctrl+D", C::ToggleDoubleSize).checked(s.dsize),
            MenuItem::sub("Size", self.size_menu()),
            MenuItem::cmd("EasyMove", "Ctrl+E", C::ToggleEasyMove).checked(s.easymove),
            MenuItem::cmd("Windowshade", "Ctrl+W", C::ToggleWindowshade).checked(s.windowshade),
            MenuItem::sep(),
            MenuItem::cmd("Repeat", "R", C::ToggleRepeat).checked(s.repeat),
            MenuItem::cmd("Shuffle", "S", C::ToggleShuffle).checked(s.shuffle),
            MenuItem::cmd("Manual playlist advance", "Shift+R", C::ToggleManualAdvance).checked(!s.pladv),
            MenuItem::sep(),
            MenuItem::sub(
                "ReplayGain",
                vec![
                    MenuItem::action("Off", "", MenuAction::ReplayGain(None)).checked(!s.replaygain),
                    MenuItem::action("Track gain", "", MenuAction::ReplayGain(Some(0))).checked(s.replaygain && s.replaygain_mode == 0),
                    MenuItem::action("Album gain", "", MenuAction::ReplayGain(Some(1))).checked(s.replaygain && s.replaygain_mode == 1),
                ],
            ),
            MenuItem::sub(
                "Title format",
                vec![
                    fmt("Artist - Title", popamp_core::config::DEFAULT_TITLE_FORMAT),
                    fmt("Artist - Album - ## - Title", "[%artist% - ][%album% - ][$num(%track%,2) - ]$if2(%title%,$filepart(%filename%))"),
                    fmt("Title", "$if2(%title%,$filepart(%filename%))"),
                    fmt("File name", "$filepart(%filename%)"),
                ],
            ),
            MenuItem::sub(
                "Equalizer",
                vec![
                    MenuItem::action("Winamp bands (60Hz-16kHz)", "", MenuAction::EqBands(false)).checked(s.eq_frequencies == 0),
                    MenuItem::action("ISO bands (31Hz-16kHz)", "", MenuAction::EqBands(true)).checked(s.eq_frequencies == 1),
                    MenuItem::sep(),
                    MenuItem::action("Limiter (prevents clipping)", "", MenuAction::EqLimiter).checked(s.eq_limiter),
                ],
            ),
        ]
    }

    pub fn vis_menu(&self) -> Vec<MenuItem> {
        let s = self.settings();
        #[cfg_attr(not(feature = "visualizers"), allow(unused_mut))]
        let mut items = vec![
            MenuItem::action("Spectrum analyzer", "", MenuAction::VisMode(1)).checked(s.sa == 1),
            MenuItem::action("Oscilloscope", "", MenuAction::VisMode(2)).checked(s.sa == 2),
            MenuItem::action("Off", "", MenuAction::VisMode(0)).checked(s.sa == 0),
            MenuItem::sep(),
            MenuItem::action("Normal style", "", MenuAction::SaStyle(0)).checked(s.sa_style == 0),
            MenuItem::action("Fire style", "", MenuAction::SaStyle(1)).checked(s.sa_style == 1),
            MenuItem::action("Line style", "", MenuAction::SaStyle(2)).checked(s.sa_style == 2),
            MenuItem::action("Show peaks", "", MenuAction::SaPeaks).checked(s.sa_peaks),
            MenuItem::action("Thick bands", "", MenuAction::SaThick).checked(s.sa_thick),
            MenuItem::sep(),
            MenuItem::action("Scope: dots", "", MenuAction::OscStyle(0)).checked(s.osc_style == 0),
            MenuItem::action("Scope: lines", "", MenuAction::OscStyle(1)).checked(s.osc_style == 1),
            MenuItem::action("Scope: solid", "", MenuAction::OscStyle(2)).checked(s.osc_style == 2),
            MenuItem::action("Scope: afterglow", "", MenuAction::OscStyle(3)).checked(s.osc_style == 3),
        ];
        #[cfg(feature = "visualizers")]
        items.extend(self.visualizer_items());
        items
    }

    pub fn skins_menu(&self) -> Vec<MenuItem> {
        let mut items = vec![
            MenuItem::cmd("Skin browser…", "Alt+S", Command::SkinBrowser),
            MenuItem::cmd("Gallery (popamp.app)…", "", Command::Gallery),
            MenuItem::cmd("Skin Museum (online)…", "", Command::SkinMuseum),
            MenuItem::sep(),
        ];
        items.push(MenuItem::action("Popamp Default", "", MenuAction::Skin(None)).checked(self.skin_path.is_none()));
        for p in popamp_skin::list_skins(&self.paths.skins_dirs()).into_iter().take(40) {
            let name = p.file_stem().unwrap_or_default().to_string_lossy().into_owned();
            let checked = self.skin_path.as_deref() == Some(p.as_path());
            items.push(MenuItem::action(&name, "", MenuAction::Skin(Some(p))).checked(checked));
        }
        items
    }

    pub fn eq_presets_menu(&self) -> Vec<MenuItem> {
        let presets = |auto: bool| -> Vec<MenuItem> {
            let list = if auto { &self.eq_auto } else { &self.eq_presets };
            let mut v: Vec<MenuItem> =
                list.iter().enumerate().take(40).map(|(i, p)| MenuItem::action(&p.name, "", MenuAction::EqPreset(i, auto))).collect();
            if v.is_empty() {
                v.push(MenuItem::label("(none)"));
            }
            v
        };
        vec![
            MenuItem::sub("Load", {
                let mut v = presets(false);
                v.push(MenuItem::sep());
                v.push(MenuItem::sub("Auto-load presets", presets(true)));
                v.push(MenuItem::cmd("Default", "", Command::EqLoadDefault));
                v.push(MenuItem::cmd("From EQF…", "", Command::EqImportEqf));
                v
            }),
            MenuItem::sub(
                "Save",
                vec![
                    MenuItem::cmd("Preset…", "", Command::EqPresetSave),
                    MenuItem::action("Auto-load preset…", "", MenuAction::EqDialog(DialogKind::EqSavePreset { auto: true })),
                    MenuItem::cmd("Default", "", Command::EqSaveDefault),
                    MenuItem::sep(),
                    MenuItem::cmd("To EQF…", "", Command::EqExportEqf),
                ],
            ),
            MenuItem::sub(
                "Delete",
                vec![
                    MenuItem::cmd("Preset…", "", Command::EqPresetDelete),
                    MenuItem::action("Auto-load preset…", "", MenuAction::EqDialog(DialogKind::EqDeletePreset { auto: true })),
                ],
            ),
            MenuItem::sep(),
            MenuItem::cmd("Reset to flat", "", Command::EqReset),
        ]
    }

    pub fn playlist_row_menu(&self) -> Vec<MenuItem> {
        use Command as C;
        vec![
            MenuItem::action("Play item", "Enter", MenuAction::PlaySelected),
            MenuItem::sep(),
            MenuItem::cmd("Remove item(s)", "Del", C::PlRemoveSelected),
            MenuItem::cmd("Crop item(s)", "Ctrl+Del", C::PlCropSelected),
            MenuItem::sep(),
            MenuItem::cmd("View file info…", "Alt+3", C::FileInfo),
            MenuItem::cmd("Playlist entry…", "Ctrl+E", C::EditPlaylistEntry),
            MenuItem::cmd("Bookmark item(s)", "Alt+I", C::BookmarkSelected),
            MenuItem::cmd("Scan ReplayGain", "", C::ScanReplayGain),
            MenuItem::sep(),
            MenuItem::sub("Sort", self.sort_menu()),
        ]
    }

    /// The playlist's MISC > Misc options menu.
    pub fn playlist_misc_menu(&self) -> Vec<MenuItem> {
        vec![
            MenuItem::cmd("Generate HTML playlist…", "", Command::PlGenerateHtml),
            MenuItem::sep(),
            MenuItem::sub("Options", self.options_menu()),
        ]
    }

    pub fn sort_menu(&self) -> Vec<MenuItem> {
        use Command as C;
        vec![
            MenuItem::cmd("Sort list by title", "Ctrl+Shift+1", C::SortByTitle),
            MenuItem::cmd("Sort list by filename", "Ctrl+Shift+2", C::SortByFileName),
            MenuItem::cmd("Sort list by path and filename", "Ctrl+Shift+3", C::SortByPath),
            MenuItem::sep(),
            MenuItem::cmd("Reverse list", "Ctrl+R", C::ReverseList),
            MenuItem::cmd("Randomize list", "Ctrl+Shift+R", C::RandomizeList),
        ]
    }

    // --- remote control --------------------------------------------------------

    pub fn on_remote(&mut self, remotes: Vec<Remote>) -> String {
        let mut reply = "ok".to_string();
        for r in remotes {
            match r {
                Remote::Open(files) => self.add(files, AddMode::ReplaceAndPlay),
                Remote::Add(files) => self.add(files, AddMode::Append),
                Remote::Cmd(c) => self.exec(c),
                Remote::SetVolumePercent(p) => self.set_volume((p as i32 * 255 + 50) / 100),
                Remote::VolumeStep(d) => self.set_volume(self.settings().volume + d * 255 / 100),
                Remote::SetPan(p) => self.set_balance(p),
                Remote::PanStep(d) => self.set_balance(self.settings().balance + d * 127 / 100),
                Remote::Shuffle(v) => {
                    let v = v.unwrap_or(!self.settings().shuffle);
                    self.settings_mut().shuffle = v;
                    self.update_next();
                }
                Remote::Repeat(v) => {
                    let v = v.unwrap_or(!self.settings().repeat);
                    self.settings_mut().repeat = v;
                    self.update_next();
                }
                Remote::SeekMs(ms) => self.seek_to(ms),
                Remote::Clear => self.exec(Command::ClearPlaylist),
                Remote::Show => self.raise(),
                Remote::Quit => self.exec(Command::Exit),
                Remote::Status => reply = self.status_json(),
            }
        }
        reply
    }

    /// Brings the windows up, out of the tray if they are there.
    fn raise(&mut self) {
        if std::mem::take(&mut self.in_tray) {
            self.ui.push(UiReq::Layout);
        }
        self.ui.push(UiReq::Raise);
    }

    pub fn media_snapshot(&self) -> crate::media::Snapshot {
        let s = self.settings();
        let status = match self.state() {
            PlayState::Playing => "Playing",
            PlayState::Paused => "Paused",
            PlayState::Stopped => "Stopped",
        };
        let track = self.playlist.current_entry().map(|e| {
            let playing = self.status.playing_id == Some(e.id);
            let meta = e.meta.as_deref();
            let title = match (&self.status.stream_title, playing) {
                (Some(t), true) => t.clone(),
                _ => meta.and_then(|m| m.title.clone()).unwrap_or_else(|| e.display.clone()),
            };
            crate::media::Track {
                id: e.id,
                title,
                artist: meta.and_then(|m| m.artist.clone()),
                album: meta.and_then(|m| m.album.clone()),
                url: e.location.clone(),
                length_ms: e.length_ms,
            }
        });
        crate::media::Snapshot {
            status,
            repeat: s.repeat,
            shuffle: s.shuffle,
            volume: s.volume as f64 / 255.0,
            track,
            notify: s.notify_tracks,
            tray: s.tray_icon || self.in_tray,
        }
    }

    pub fn status_json(&self) -> String {
        fn esc(s: &str) -> String {
            let mut o = String::new();
            for c in s.chars() {
                match c {
                    '"' => o.push_str("\\\""),
                    '\\' => o.push_str("\\\\"),
                    c if (c as u32) < 0x20 => o.push_str(&format!("\\u{:04x}", c as u32)),
                    c => o.push(c),
                }
            }
            o
        }
        let sh = self.engine.shared();
        let state = match sh.state() {
            PlayState::Playing => "playing",
            PlayState::Paused => "paused",
            PlayState::Stopped => "stopped",
        };
        let s = self.settings();
        let cur = self.playlist.current_entry();
        format!(
            "{{\"state\":\"{state}\",\"position_ms\":{},\"length_ms\":{},\"volume\":{},\"balance\":{},\"shuffle\":{},\"repeat\":{},\"index\":{},\"count\":{},\"title\":\"{}\",\"location\":\"{}\"}}",
            self.engine.position_ms(),
            sh.length_ms().map(|l| l.to_string()).unwrap_or_else(|| "null".into()),
            s.volume,
            s.balance,
            s.shuffle,
            s.repeat,
            self.playlist.current().map(|i| i.to_string()).unwrap_or_else(|| "null".into()),
            self.playlist.len(),
            esc(&cur.map(|e| e.display.clone()).unwrap_or_default()),
            esc(&cur.map(|e| e.location.clone()).unwrap_or_default()),
        )
    }
}

/// A menu of the sizes, ×1 to ×3, with `current` checked.
fn zooms(current: i32, action: fn(i32) -> MenuAction) -> Vec<MenuItem> {
    popamp_core::config::ZOOMS
        .iter()
        .map(|&pct| MenuItem::action(&format!("×{}", pct as f32 / 100.0), "", action(pct)).checked(pct == current))
        .collect()
}

/// The tags and format of a track as File info lists them.
pub fn meta_rows(m: &TrackMeta) -> Vec<(String, String)> {
    let mut rows = Vec::new();
    let mut push = |k: &str, v: String| rows.push((k.to_string(), v));
    let fields = [
        ("Title", &m.title),
        ("Artist", &m.artist),
        ("Album", &m.album),
        ("Album artist", &m.album_artist),
        ("Year", &m.year),
        ("Track", &m.track),
        ("Disc", &m.disc),
        ("Genre", &m.genre),
        ("Composer", &m.composer),
        ("Comment", &m.comment),
        ("Codec", &m.codec),
    ];
    for (k, v) in fields {
        if let Some(v) = v {
            push(k, v.clone());
        }
    }
    if let Some(l) = m.length_ms {
        push("Length", format_duration_ms(l));
    }
    if let (Some(r), Some(c)) = (m.sample_rate, m.channels) {
        push("Format", format!("{r} Hz, {c} channel(s){}", m.bitrate_kbps.map(|b| format!(", {b} kbps")).unwrap_or_default()));
    }
    if let Some(g) = m.replaygain_track_gain {
        push("ReplayGain", format!("{g:+.2} dB (track)"));
    }
    rows
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A model whose playlist holds `n` local entries titled "Song 1"...
    fn with_songs(n: usize) -> Model {
        let mut m = Model::for_tests();
        let items =
            (1..=n).map(|i| Item { location: format!("/music/{i}.mp3"), title: Some(format!("Song {i}")), length_ms: None }).collect();
        m.on_expanded(items, AddMode::Append);
        m
    }

    fn titles(m: &Model) -> Vec<String> {
        m.bookmarks.iter().map(|b| b.title.clone().unwrap_or_default()).collect()
    }

    #[test]
    fn the_options_menu_offers_plugins_only_in_a_build_with_them() {
        let m = Model::for_tests();
        let offered = m.options_menu().iter().any(|i| i.label == "Plug-ins…");
        assert_eq!(offered, cfg!(feature = "plugins"));
    }

    #[test]
    fn the_options_menu_opens_the_preferences() {
        let mut m = Model::for_tests();
        let item = m.options_menu().into_iter().find(|i| i.label == "Preferences…").expect("in the menu");
        m.on_menu(item.action.expect("does something"));
        assert!(m.ui.iter().any(|r| matches!(r, UiReq::Panel(PanelKind::Preferences))));
    }

    #[test]
    fn adding_expands_each_location_through_its_source() {
        let dir = std::env::temp_dir().join(format!("popamp-add-{}", std::process::id()));
        std::fs::create_dir_all(dir.join("album")).unwrap();
        for f in ["2.mp3", "1.mp3"] {
            std::fs::write(dir.join("album").join(f), b"").unwrap();
        }
        std::fs::write(dir.join("list.m3u"), "x.ogg\n").unwrap();
        let mut m = Model::for_tests();
        let at = |f: &str| f.split('/').fold(dir.clone(), |p, part| p.join(part)).to_string_lossy().into_owned();
        m.add_now(vec![at("album"), at("list.m3u"), "http://radio:8000/stream".into()], AddMode::Append);
        let got: Vec<_> = m.playlist.entries().iter().map(|e| e.location.clone()).collect();
        std::fs::remove_dir_all(&dir).unwrap();
        assert_eq!(got, [at("album/1.mp3"), at("album/2.mp3"), at("x.ogg"), "http://radio:8000/stream".into()]);
    }

    #[test]
    fn a_test_model_takes_its_settings_folder_with_it() {
        let m = Model::for_tests();
        let dir = m.paths.config_dir.clone();
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("bookmarks.m3u8"), "#EXTM3U\n").unwrap();
        drop(m);
        assert!(!dir.exists(), "{dir:?} left behind");
    }

    #[test]
    fn an_opened_vector_skin_archive_is_installed_not_queued() {
        let mut m = Model::for_tests();
        let src = m.paths.config_dir.join("Mine.psz");
        std::fs::create_dir_all(&m.paths.config_dir).unwrap();
        let mut w = zip::ZipWriter::new(std::fs::File::create(&src).unwrap());
        w.start_file("skin.toml", zip::write::SimpleFileOptions::default()).unwrap();
        std::io::Write::write_all(&mut w, b"[meta]\nname = \"Mine\"\nformat_version = 1\n").unwrap();
        w.finish().unwrap();
        m.add(vec![src.to_string_lossy().into_owned()], AddMode::Append);
        assert_eq!(m.skin.name, "Mine");
        assert!(m.paths.skins_dir().join("Mine.psz").is_file());
        assert_eq!(m.playlist.len(), 0);
    }

    #[test]
    fn a_skin_with_colour_sets_offers_them_and_remembers_the_pick() {
        let mut m = Model::for_tests();
        let colours = |m: &Model| m.main_menu().into_iter().find(|i| i.label == "Colours");
        assert!(colours(&m).is_none(), "the default skin has one look");
        let dir = m.paths.skins_dir().join("tinted");
        std::fs::create_dir_all(&dir).unwrap();
        let toml = "[meta]\nname = \"Tinted\"\nformat_version = 1\n[colors.day]\nback = \"#ffffff\"\n\
            [colors.night]\nback = \"#000020\"\n[playlist]\nnormal_bg = \"back\"\n";
        std::fs::write(dir.join("skin.toml"), toml).unwrap();
        m.set_skin(Some(dir.clone()));
        let sets = colours(&m).unwrap().sub;
        assert_eq!(sets.iter().map(|i| (i.label.as_str(), i.checked)).collect::<Vec<_>>(), [("day", true), ("night", false)]);
        m.on_menu(sets[1].action.clone().unwrap());
        assert_eq!(m.skin.pl.normal_bg, 0x000020);
        assert!(matches!(m.ui.last(), Some(UiReq::SkinChanged)));
        assert!(colours(&m).unwrap().sub[1].checked);
        m.set_skin(None);
        m.set_skin(Some(dir));
        assert_eq!(m.skin.pl.normal_bg, 0x000020, "night again after another skin");
    }

    #[test]
    fn an_edited_skin_is_loaded_again() {
        let mut m = Model::for_tests();
        let dir = m.paths.skins_dir().join("vector");
        std::fs::create_dir_all(&dir).unwrap();
        let write = |name: &str| std::fs::write(dir.join("skin.toml"), format!("[meta]\nname = \"{name}\"\nformat_version = 1\n")).unwrap();
        write("First");
        m.set_skin(Some(dir.clone()));
        assert_eq!(m.skin.name, "First");
        m.reload_skin_if_changed();
        assert_eq!(m.ui.len(), 1, "nothing changed: no reload");
        std::thread::sleep(std::time::Duration::from_millis(20));
        write("Second");
        m.reload_skin_if_changed();
        assert_eq!(m.skin.name, "Second");
        std::thread::sleep(std::time::Duration::from_millis(20));
        std::fs::write(dir.join("skin.toml"), "[meta]\nname = 1\n").unwrap();
        m.reload_skin_if_changed();
        assert_eq!(m.skin.name, "Second", "a broken edit keeps the skin");
        assert!(m.transient.as_ref().is_some_and(|t| t.0.starts_with("SKIN ERROR: ")));
    }

    #[test]
    fn minimizing_to_the_tray_shows_the_icon_until_the_windows_come_back() {
        let mut m = Model::for_tests();
        m.settings_mut().tray_icon = false;
        m.exec(Command::Minimize);
        assert!(matches!(m.ui.pop(), Some(UiReq::Minimize)), "to the taskbar");
        assert!(!m.media_snapshot().tray);

        m.settings_mut().minimize_to_tray = true;
        m.exec(Command::Minimize);
        assert!(m.in_tray && matches!(m.ui.pop(), Some(UiReq::Layout)), "the windows hide");
        assert!(m.media_snapshot().tray, "the icon comes for them");

        m.on_remote(vec![Remote::Show]);
        assert!(!m.in_tray && !m.media_snapshot().tray, "back up, the icon goes");
        assert!(matches!(m.ui[..], [UiReq::Layout, UiReq::Raise]));

        m.settings_mut().tray_icon = true;
        m.exec(Command::Minimize);
        m.exec(Command::ToggleMainWindow);
        assert!(m.media_snapshot().tray, "an icon asked for stays");
    }

    /// The entry `label` of the main menu's Toolbar mode.
    fn toolbar(m: &Model, label: &str) -> Option<MenuItem> {
        let mode = m.main_menu().into_iter().find(|i| i.label == "Toolbar mode")?;
        mode.sub.into_iter().find(|i| i.label == label)
    }

    #[test]
    fn a_bar_skin_offers_the_edge_in_the_main_menu() {
        let mut m = Model::for_tests();
        assert!(m.main_menu().iter().all(|i| i.label != "Toolbar mode"), "no bar, no choice");
        m.skin.layout.windows[0].placement = popamp_skin::layout::Placement::Bar(BarEdge::Bottom);
        assert!(toolbar(&m, "On").is_none() && toolbar(&m, "Size").is_some(), "a bar it is, with a size");
        let items = toolbar(&m, "Position").unwrap().sub;
        assert!(items[1].checked && !items[0].checked, "the skin's edge");
        m.on_menu(items[0].action.clone().unwrap());
        assert_eq!(crate::bar::edge(&m), Some(BarEdge::Top));
        assert!(matches!(m.ui.last(), Some(UiReq::Layout)));
    }

    #[test]
    fn a_skin_with_both_lets_the_user_pick_window_or_bar() {
        use popamp_skin::layout::{Bar, Placement, View};
        let mut m = Model::for_tests();
        m.settings_mut().main_bar = true;
        assert!(toolbar(&m, "On").is_none(), "the skin has no bar view");
        assert_eq!(crate::bar::edge(&m), None, "and stays a window whatever the setting");
        m.settings_mut().main_bar = false;
        let main = &mut m.skin.layout.windows[0];
        main.bar = Some(Bar { edge: BarEdge::Top, mode: main.normal.clone() });
        let (on, off) = (toolbar(&m, "On").unwrap(), toolbar(&m, "Off").unwrap());
        assert!(off.checked && !on.checked, "a window at first");
        let top = toolbar(&m, "Position").unwrap().sub[0].clone();
        assert!(top.checked && toolbar(&m, "Size").is_some(), "where and how big it will be, chosen before it is on");
        m.on_menu(on.action.unwrap());
        assert!(matches!(m.ui.last(), Some(UiReq::SkinChanged)), "the window is made again as a bar");
        assert_eq!(crate::bind::view(&m, WindowKey::Main), View::Bar);
        assert_eq!(crate::bind::placement(&m, WindowKey::Main), Placement::Bar(BarEdge::Top));
        assert_eq!(crate::bind::view(&m, WindowKey::Eq), View::Normal);
        assert!(toolbar(&m, "On").unwrap().checked);
        m.settings_mut().windowshade = true;
        assert_eq!(crate::bind::view(&m, WindowKey::Main), View::Bar, "a bar has no shade mode");
    }

    #[test]
    fn a_bar_has_its_own_size_in_the_main_menu() {
        let mut m = Model::for_tests();
        m.skin.layout.windows[0].placement = popamp_skin::layout::Placement::Bar(BarEdge::Bottom);
        let sizes = toolbar(&m, "Size").unwrap().sub;
        let checked: Vec<_> = sizes.iter().filter(|i| i.checked).map(|i| i.label.as_str()).collect();
        assert_eq!(checked, ["×2"]);
        let x15 = sizes.iter().find(|i| i.label == "×1.5").unwrap();
        m.on_menu(x15.action.clone().unwrap());
        assert_eq!(m.settings().bar_zoom, 150);
        assert!(!m.settings().dsize, "the windows keep their size");
        assert!(matches!(m.ui.last(), Some(UiReq::Layout)));
    }

    #[test]
    fn size_menu_sets_double_size_and_its_zoom() {
        let mut m = Model::for_tests();
        m.on_menu(MenuAction::Zoom(150));
        assert!(m.settings().dsize && m.settings().zoom == 150);
        m.exec(Command::ToggleDoubleSize);
        assert!(!m.settings().dsize);
        m.exec(Command::ToggleDoubleSize);
        assert_eq!(crate::scale::Scale::of(m.settings()).px(100), 150, "Ctrl+D returns to the chosen size");
        m.on_menu(MenuAction::Zoom(100));
        assert!(!m.settings().dsize && m.settings().zoom == 150);
    }

    #[test]
    fn bookmarks_current_and_selected_and_persist() {
        let mut m = with_songs(3);
        m.exec(Command::AddBookmark); // nothing playing: nothing to add
        assert!(m.bookmarks.is_empty());
        m.playlist.set_current(Some(1));
        m.exec(Command::AddBookmark);
        m.playlist.select_only(0);
        m.playlist.select_range(0, 2);
        m.exec(Command::BookmarkSelected);
        assert_eq!(titles(&m), ["Song 2", "Song 1", "Song 2", "Song 3"]);
        assert_eq!(m.bookmarks[0].location, "/music/2.mp3");
        // Saved: a new model on the same settings directory sees them.
        let again = Model::headless(m.paths.clone(), Config::load(m.paths.ini_file()));
        assert_eq!(titles(&again), titles(&m));
    }

    #[test]
    fn eq_default_preset_saves_and_loads() {
        let mut m = Model::for_tests();
        m.set_eq_band(0, 10);
        m.exec(Command::EqSaveDefault);
        m.exec(Command::EqReset);
        assert_eq!(m.settings().eq_bands()[0], 31);
        m.exec(Command::EqLoadDefault);
        assert_eq!(m.settings().eq_bands()[0], 10);
    }

    #[test]
    fn generates_an_html_playlist() {
        let mut m = with_songs(2);
        m.exec(Command::PlGenerateHtml);
        assert!(matches!(m.ui.last(), Some(UiReq::Dialog(DialogKind::SaveHtml, _))));
        let path = m.paths.config_dir.join("list");
        std::fs::create_dir_all(&m.paths.config_dir).unwrap();
        m.on_dialog(DialogResult { kind: DialogKind::SaveHtml, values: vec![path.to_string_lossy().into_owned()] });
        let html = std::fs::read_to_string(path.with_extension("html")).unwrap();
        assert!(html.contains("<li>Song 2 <span>(?)</span></li>"));
    }

    #[test]
    fn removing_and_playing_bookmarks() {
        let mut m = with_songs(2);
        m.playlist.select_range(0, 1);
        m.exec(Command::BookmarkSelected);
        m.remove_bookmark(0);
        m.remove_bookmark(7); // out of range: ignored
        assert_eq!(titles(&m), ["Song 2"]);
        let menu = m.bookmarks_menu();
        assert!(menu.iter().any(|i| i.label == "Song 2"));
    }
}
