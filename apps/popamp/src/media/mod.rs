//! The desktop's media controls: media keys, desktop widgets, playerctl.
//! The UI thread sends [`Snapshot`]s through [`Controls`]; each system's
//! service turns them into its own notifications and sends method calls
//! back as remote commands. MPRIS2 over D-Bus on Linux, the System Media
//! Transport Controls on Windows, Now Playing on macOS (not yet).

// The services' shared helpers go unused on macOS until Now Playing is there.
#![cfg_attr(target_os = "macos", allow(dead_code))]

use crate::cli::Remote;
use crate::ipc::Request;
use crate::model::AppEvent;
use popamp_audio::cover::{self, Cover};
use popamp_audio::Shared;
use popamp_core::Command;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use winit::event_loop::EventLoopProxy;

#[cfg(linux_like)]
mod mpris;
#[cfg(linux_like)]
mod notify;
#[cfg(linux_like)]
mod tray;
#[cfg(windows)]
#[path = "tray_windows.rs"]
mod tray;
#[cfg(target_os = "macos")]
#[path = "tray_macos.rs"]
mod tray;
#[cfg(linux_like)]
use mpris as service;
#[cfg(target_os = "macos")]
mod nowplaying;
#[cfg(target_os = "macos")]
use nowplaying as service;
#[cfg(windows)]
mod smtc;
#[cfg(windows)]
use smtc as service;

/// Sends `remotes` to the UI thread, as a remote control would.
pub(crate) fn send(proxy: &EventLoopProxy<AppEvent>, remotes: Vec<Remote>) {
    let (tx, _rx) = crossbeam_channel::bounded(1);
    let _ = proxy.send_event(AppEvent::Remote(Request { remotes, reply: tx }));
}

/// What a Pause button asks of a player that is `status`: nothing unless
/// it plays, since pausing a paused player resumes it.
fn pause(status: &str) -> Option<Command> {
    (status == "Playing").then_some(Command::Pause)
}

/// The cover of the track on show as a file the desktop can read: an
/// image beside the track, or the picture from its tags written out to a
/// file of ours, kept until the next track. The files a Popamp left when
/// it ended go when the next one starts.
struct Covers {
    written: Option<PathBuf>,
}

impl Default for Covers {
    fn default() -> Self {
        sweep(&std::env::temp_dir());
        Self { written: None }
    }
}

/// Removes the cover files in `dir` of Popamps no longer running: one
/// that exits ends its threads before their [`Covers`] are dropped.
fn sweep(dir: &Path) {
    let Ok(entries) = std::fs::read_dir(dir) else { return };
    for entry in entries.flatten() {
        let name = entry.file_name();
        let pid = name.to_str().and_then(|n| n.strip_prefix("popamp-cover-")?.split('-').next()?.parse::<u32>().ok());
        if pid.is_some_and(|pid| pid != std::process::id() && !running(pid)) {
            let _ = std::fs::remove_file(entry.path());
        }
    }
}

/// Whether a process `pid` is running.
#[cfg(windows)]
fn running(pid: u32) -> bool {
    use windows_sys::Win32::Foundation::{CloseHandle, GetLastError, ERROR_ACCESS_DENIED, STILL_ACTIVE};
    use windows_sys::Win32::System::Threading::{GetExitCodeProcess, OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION};
    // SAFETY: the handle is checked, read and closed once.
    unsafe {
        let process = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid);
        if process.is_null() {
            return GetLastError() == ERROR_ACCESS_DENIED;
        }
        let mut code = 0;
        let alive = GetExitCodeProcess(process, &mut code) == 0 || code == STILL_ACTIVE as u32;
        CloseHandle(process);
        alive
    }
}

#[cfg(linux_like)]
fn running(pid: u32) -> bool {
    Path::new(&format!("/proc/{pid}")).exists()
}

/// macOS has no `/proc`; `ps` knows.
#[cfg(target_os = "macos")]
fn running(pid: u32) -> bool {
    std::process::Command::new("ps").args(["-p", &pid.to_string()]).output().is_ok_and(|o| o.status.success())
}

impl Covers {
    /// The cover file of `track`, if it has a cover.
    fn of(&mut self, track: &Track) -> Option<PathBuf> {
        self.forget();
        if popamp_core::playlist::is_url(&track.url) {
            return None;
        }
        match cover::find(Path::new(&track.url))? {
            Cover::File(path) => Some(path),
            Cover::Tagged(picture) => {
                let path = std::env::temp_dir().join(format!("popamp-cover-{}-{}.{}", std::process::id(), track.id, extension(&picture)));
                std::fs::write(&path, &picture.data).inspect_err(|e| log::debug!("{}: {e}", path.display())).ok()?;
                self.written = Some(path.clone());
                Some(path)
            }
        }
    }

    fn forget(&mut self) {
        if let Some(path) = self.written.take() {
            let _ = std::fs::remove_file(path);
        }
    }
}

impl Drop for Covers {
    fn drop(&mut self) {
        self.forget();
    }
}

/// The file name extension for `picture`'s media type.
fn extension(picture: &cover::Picture) -> &'static str {
    match picture.media_type.as_str() {
        "image/png" => "png",
        "image/gif" => "gif",
        "image/bmp" => "bmp",
        "image/webp" => "webp",
        _ => "jpg",
    }
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Track {
    pub id: u64,
    pub title: String,
    pub artist: Option<String>,
    pub album: Option<String>,
    pub url: String,
    pub length_ms: Option<u64>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Snapshot {
    pub status: &'static str,
    pub repeat: bool,
    pub shuffle: bool,
    pub volume: f64,
    pub track: Option<Track>,
    /// Whether the user wants a notification for each new track.
    pub notify: bool,
    /// Whether the user wants an icon in the system tray.
    pub tray: bool,
}

impl Default for Snapshot {
    fn default() -> Self {
        Self { status: "Stopped", repeat: false, shuffle: false, volume: 1.0, track: None, notify: false, tray: false }
    }
}

/// Picks the tracks worth a notification: each new track, or new stream
/// title, once, when it plays. Pausing and resuming is no news.
#[derive(Default)]
#[cfg(linux_like)] // Windows shows its own media flyout; macOS, later, Now Playing.
struct News {
    told: Option<(u64, String)>,
}

#[cfg(linux_like)]
impl News {
    fn of<'a>(&mut self, snap: &'a Snapshot) -> Option<&'a Track> {
        let track = snap.track.as_ref().filter(|_| snap.notify && snap.status == "Playing")?;
        let key = (track.id, track.title.clone());
        (self.told.as_ref() != Some(&key)).then(|| {
            self.told = Some(key);
            track
        })
    }
}

/// The line under the title: who and from what.
fn byline(track: &Track) -> String {
    [&track.artist, &track.album].into_iter().flatten().filter(|s| !s.trim().is_empty()).cloned().collect::<Vec<_>>().join(" — ")
}

/// What the tray icon's tooltip says under the name.
fn now_playing(snap: &Snapshot) -> String {
    let Some(track) = &snap.track else { return tr!("nowPlaying.stopped").into() };
    let line = match byline(track) {
        by if by.is_empty() => track.title.clone(),
        by => format!("{}\n{by}", track.title),
    };
    match snap.status {
        "Playing" => line,
        "Paused" => format!("{line}\n({})", tr!("nowPlaying.paused")),
        status => format!("{line}\n({status})"),
    }
}

/// An entry of the tray icon's menu.
#[derive(Debug, Clone, PartialEq)]
struct Entry {
    label: &'static str,
    /// Some for an entry with a check mark.
    checked: Option<bool>,
    remote: Remote,
}

/// The tray icon's menu; `None` is a separator.
fn tray_menu(snap: &Snapshot) -> Vec<Option<Entry>> {
    let button = |label, command| Some(Entry { label, checked: None, remote: Remote::Cmd(command) });
    let check = |label, on: bool, remote| Some(Entry { label, checked: Some(on), remote });
    vec![
        Some(Entry { label: tr!("menu.tray.showPopamp"), checked: None, remote: Remote::Show }),
        None,
        button(tr!("menu.tray.previous"), Command::Previous),
        button(tr!("menu.tray.play"), Command::Play),
        button(tr!("menu.tray.pause"), Command::Pause),
        button(tr!("menu.tray.stop"), Command::Stop),
        button(tr!("menu.tray.next"), Command::Next),
        None,
        check(tr!("menu.tray.shuffle"), snap.shuffle, Remote::Shuffle(Some(!snap.shuffle))),
        check(tr!("menu.tray.repeat"), snap.repeat, Remote::Repeat(Some(!snap.repeat))),
        None,
        Some(Entry { label: tr!("menu.tray.exit"), checked: None, remote: Remote::Quit }),
    ]
}

/// Sends remote commands to the UI thread.
type Sender = Arc<dyn Fn(Vec<Remote>) + Send + Sync>;

/// Feeds the tray icon on a thread of its own: it may wait on the desktop.
fn spawn_tray(send: Sender) -> Option<crossbeam_channel::Sender<Snapshot>> {
    let (tx, rx) = crossbeam_channel::unbounded::<Snapshot>();
    let run = move || {
        let mut tray = tray::Tray::new(send);
        for snap in rx {
            tray.update(&snap);
        }
    };
    std::thread::Builder::new().name("popamp-tray".into()).spawn(run).map_err(|e| log::warn!("no tray icon: {e}")).ok()?;
    Some(tx)
}

/// Where the snapshots go: the system's media controls and the tray icon.
pub struct Controls {
    to: Vec<crossbeam_channel::Sender<Snapshot>>,
    last: Option<Snapshot>,
}

impl Controls {
    /// Publishes the snapshot if it changed.
    pub fn update(&mut self, snap: Snapshot) {
        if self.last.as_ref() != Some(&snap) {
            self.last = Some(snap.clone());
            for to in &self.to {
                let _ = to.send(snap.clone());
            }
        }
    }
}

/// Starts the system's media service, if it has one running, and the tray.
pub fn spawn(proxy: EventLoopProxy<AppEvent>, shared: Arc<Shared>, instance: &str) -> Controls {
    let service = service::spawn(proxy.clone(), shared, instance);
    let tray = spawn_tray(Arc::new(move |remotes| send(&proxy, remotes)));
    Controls { to: service.into_iter().chain(tray).collect(), last: None }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn track(id: u64, url: &str) -> Track {
        Track { id, url: url.into(), ..Track::default() }
    }

    #[test]
    fn a_tagged_cover_is_written_out_until_the_next_track() {
        let dir = std::env::temp_dir().join(format!("popamp-covers-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        // ID3v2.3 with a 3-byte PNG "cover", then MPEG-1 Layer III frames.
        let mut mp3 = b"ID3\x03\x00\x00\x00\x00\x00\x1a".to_vec();
        mp3.extend_from_slice(b"APIC\x00\x00\x00\x10\x00\x00\x00image/png\x00\x03\x00png");
        for _ in 0..4 {
            mp3.extend_from_slice(&[0xFF, 0xFB, 0x90, 0x00]);
            mp3.extend_from_slice(&[0; 413]);
        }
        let tagged = dir.join("a.mp3");
        std::fs::write(&tagged, mp3).unwrap();
        let mut covers = Covers::default();
        let written = covers.of(&track(7, tagged.to_str().unwrap())).expect("a cover");
        assert_eq!(written.extension().unwrap(), "png");
        assert_eq!(std::fs::read(&written).unwrap(), b"png");
        assert_eq!(covers.of(&track(8, "http://radio.example/stream")), None);
        assert!(!written.exists(), "gone with its track");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn covers_of_popamps_gone_are_swept() {
        let dir = std::env::temp_dir().join(format!("popamp-sweep-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        // A pid no process has: they are multiples of 4 on Windows.
        let gone = dir.join("popamp-cover-4294967293-3.jpg");
        let ours = dir.join(format!("popamp-cover-{}-1.png", std::process::id()));
        let other = dir.join("popamp-cover-notes.txt");
        for f in [&gone, &ours, &other] {
            std::fs::write(f, b"x").unwrap();
        }
        sweep(&dir);
        assert!(!gone.exists());
        assert!(ours.exists() && other.exists());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[cfg(linux_like)]
    fn playing(id: u64, title: &str) -> Snapshot {
        Snapshot {
            status: "Playing",
            notify: true,
            track: Some(Track { id, title: title.into(), ..track(id, "/a.mp3") }),
            ..Snapshot::default()
        }
    }

    #[test]
    #[cfg(linux_like)]
    fn each_new_track_is_news_once() {
        let mut news = News::default();
        let title = |s: &Snapshot, news: &mut News| news.of(s).map(|t| t.title.clone());
        assert_eq!(title(&playing(1, "One"), &mut news).as_deref(), Some("One"));
        assert_eq!(title(&playing(1, "One"), &mut news), None, "told already");
        assert_eq!(title(&Snapshot { status: "Paused", ..playing(1, "One") }, &mut news), None);
        assert_eq!(title(&playing(1, "One"), &mut news), None, "resumed is not new");
        assert_eq!(title(&playing(1, "Next song on the radio"), &mut news).as_deref(), Some("Next song on the radio"));
        assert_eq!(title(&Snapshot { status: "Paused", ..playing(2, "Two") }, &mut news), None, "not while paused");
        assert_eq!(title(&Snapshot { notify: false, ..playing(2, "Two") }, &mut news), None, "not when turned off");
        assert_eq!(title(&playing(2, "Two"), &mut news).as_deref(), Some("Two"));
    }

    #[test]
    fn the_tooltip_says_what_plays() {
        let track = Track { title: "Song".into(), artist: Some("Band".into()), ..Track::default() };
        let snap = |status| Snapshot { status, track: Some(track.clone()), ..Snapshot::default() };
        assert_eq!(now_playing(&Snapshot::default()), "Stopped");
        assert_eq!(now_playing(&snap("Playing")), "Song\nBand");
        assert_eq!(now_playing(&snap("Paused")), "Song\nBand\n(Paused)");
        let bare = Snapshot { track: Some(Track { title: "Stream".into(), ..Track::default() }), status: "Playing", ..Snapshot::default() };
        assert_eq!(now_playing(&bare), "Stream");
    }

    #[test]
    fn the_tray_menu_toggles_from_the_current_modes() {
        let menu = tray_menu(&Snapshot { shuffle: true, ..Snapshot::default() });
        let find = |label| menu.iter().flatten().find(|e| e.label == label).cloned().unwrap();
        assert_eq!(find("Shuffle"), Entry { label: "Shuffle", checked: Some(true), remote: Remote::Shuffle(Some(false)) });
        assert_eq!(find("Repeat"), Entry { label: "Repeat", checked: Some(false), remote: Remote::Repeat(Some(true)) });
        assert_eq!(find("Next").remote, Remote::Cmd(Command::Next));
        assert_eq!(
            (menu.first().cloned().flatten().unwrap().remote, menu.last().cloned().flatten().unwrap().remote),
            (Remote::Show, Remote::Quit)
        );
    }

    #[test]
    fn the_byline_has_what_is_known() {
        let t = |artist: Option<&str>, album: Option<&str>| Track {
            artist: artist.map(Into::into),
            album: album.map(Into::into),
            ..Track::default()
        };
        assert_eq!(byline(&t(Some("Artist"), Some("Album"))), "Artist — Album");
        assert_eq!(byline(&t(None, Some("Album"))), "Album");
        assert_eq!(byline(&t(Some("Artist"), Some(" "))), "Artist");
        assert_eq!(byline(&t(None, None)), "");
    }

    #[test]
    fn pause_only_pauses() {
        assert_eq!(pause("Playing"), Some(Command::Pause));
        assert_eq!(pause("Paused"), None);
        assert_eq!(pause("Stopped"), None);
    }
}
