//! Sources: who takes a location, lists it, describes it and says how to
//! play it. Files and HTTP are built in; plug-ins (with `--features plugins`) are
//! sources too, asked first. A source never handles audio: it answers
//! with a [`Playback`] and the engine plays.
//!
//! Every call but `handles` may be slow (a network or a program) and runs
//! on a worker thread, never on the engine or UI thread.

use crate::formats::Decoders;
use crate::pipe::Pipe;
use parking_lot::{Condvar, Mutex};
use popamp_core::cue;
use popamp_core::meta::TrackMeta;
use popamp_core::playlist::is_url;
use popamp_core::plformats::{self, is_playlist, Item};
use popamp_core::replaygain::Scanned;
use popamp_core::text;
use std::num::NonZeroU32;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

/// What a call needs from the player's settings.
#[derive(Debug, Clone)]
pub struct Context {
    /// HTTP proxy (`[user:pass@]host:port`), empty for none.
    pub proxy: String,
    /// The SoundFont that plays MIDI, empty for the system's.
    pub soundfont: String,
    /// The decoder plug-ins that are on.
    pub decoders: Decoders,
    /// The ReplayGain values Popamp measured; they win over the tags.
    pub scanned: Arc<Scanned>,
    /// How far ahead a played file is read.
    pub read_ahead_mb: NonZeroU32,
}

impl Default for Context {
    fn default() -> Context {
        Context {
            proxy: String::new(),
            soundfont: String::new(),
            decoders: Decoders::default(),
            scanned: Arc::default(),
            read_ahead_mb: popamp_core::config::DEFAULT_READ_AHEAD_MB,
        }
    }
}

/// How the engine plays a track.
#[derive(Debug, Clone, PartialEq)]
pub enum Playback {
    File(PathBuf),
    /// From `start_ms` to `end_ms` (or the end) of a local file, with
    /// the tags of that part rather than the file's.
    Part {
        path: PathBuf,
        start_ms: u64,
        end_ms: Option<u64>,
        meta: Box<TrackMeta>,
    },
    Http(String),
    Pipe(Box<Pipe>),
}

pub trait Source: Send + Sync {
    /// A short name, `[a-z0-9-]`, as a plug-in's id.
    fn id(&self) -> &str;
    /// True if this source takes `location`; quick: no network, no program.
    fn handles(&self, location: &str) -> bool;
    /// The tracks of a list (a folder, a playlist, a channel), or `None`
    /// when `location` is one track.
    fn expand(&self, location: &str, cx: &Context) -> Result<Option<Vec<Item>>, String>;
    /// Tags of a track, or `None` when this source has none to give.
    fn describe(&self, location: &str) -> Option<Result<TrackMeta, String>>;
    fn open(&self, location: &str, cx: &Context) -> Result<Playback, String>;
    /// True when opening takes long enough to say "connecting".
    fn slow(&self) -> bool {
        false
    }
}

/// The sources in the order they are asked: plug-ins, then HTTP, CUE
/// sheets and files, which play the formats of the decoder plug-ins too.
/// Shared by every thread that asks; plug-ins can be swapped while it runs.
pub struct Sources {
    /// `None` until the plug-ins are known: they load on a thread of their
    /// own while the player starts.
    all: Mutex<Option<Vec<Arc<dyn Source>>>>,
    loaded: Condvar,
}

/// How long a worker waits for the plug-ins before it does without them.
const PLUGINS_WAIT: Duration = Duration::from_secs(30);

impl Sources {
    /// Sources whose plug-ins are still loading: [`Sources::of`] waits
    /// for them, so a track given at startup goes to its plug-in.
    pub fn waiting() -> Sources {
        Sources { all: Mutex::new(None), loaded: Condvar::new() }
    }

    /// Replaces the plug-ins: the sources asked before the built-in ones,
    /// and the decoders the built-in ones play files with.
    pub fn set_plugins(&self, plugins: Vec<Arc<dyn Source>>, decoders: Decoders) {
        *self.all.lock() = Some(Self::chain(plugins, decoders));
        self.loaded.notify_all();
    }

    fn chain(plugins: Vec<Arc<dyn Source>>, decoders: Decoders) -> Vec<Arc<dyn Source>> {
        let built_in: [Arc<dyn Source>; 3] = [Arc::new(HttpSource), Arc::new(CueSource(decoders.clone())), Arc::new(FileSource(decoders))];
        plugins.into_iter().chain(built_in).collect()
    }

    /// The source that takes `location`; anything not a URL is a file.
    /// Waits while the plug-ins load, so call it on a worker thread.
    pub fn of(&self, location: &str) -> Arc<dyn Source> {
        let mut all = self.all.lock();
        if all.is_none() && self.loaded.wait_while_for(&mut all, |p| p.is_none(), PLUGINS_WAIT).timed_out() {
            log::warn!("plug-ins did not load in {PLUGINS_WAIT:?}; going on without them");
            *all = Some(Self::chain(Vec::new(), Decoders::default()));
        }
        Self::pick(all.as_deref().unwrap_or_default(), location)
    }

    /// Like [`Sources::of`], without waiting for the plug-ins: for the UI
    /// thread, which only asks how a location behaves.
    pub fn of_now(&self, location: &str) -> Arc<dyn Source> {
        Self::pick(self.all.lock().as_deref().unwrap_or_default(), location)
    }

    fn pick(all: &[Arc<dyn Source>], location: &str) -> Arc<dyn Source> {
        all.iter().find(|s| s.handles(location)).cloned().unwrap_or_else(|| Arc::new(FileSource::default()))
    }
}

impl Default for Sources {
    fn default() -> Self {
        Sources { all: Mutex::new(Some(Self::chain(Vec::new(), Decoders::default()))), loaded: Condvar::new() }
    }
}

/// Streams and playlists over HTTP(S).
pub struct HttpSource;

impl Source for HttpSource {
    fn id(&self) -> &str {
        "http"
    }

    fn handles(&self, location: &str) -> bool {
        is_url(location)
    }

    fn expand(&self, location: &str, cx: &Context) -> Result<Option<Vec<Item>>, String> {
        if !is_playlist(location) {
            return Ok(None);
        }
        let text = crate::stream::fetch_text(location, &cx.proxy)?;
        let fmt = plformats::PlaylistFormat::from_path(Path::new(location.split('?').next().unwrap_or(location)))
            .or_else(|| plformats::sniff(&text))
            .unwrap_or(plformats::PlaylistFormat::M3u);
        Ok(Some(plformats::parse(&text, fmt, None)))
    }

    fn describe(&self, _location: &str) -> Option<Result<TrackMeta, String>> {
        None
    }

    fn open(&self, location: &str, _cx: &Context) -> Result<Playback, String> {
        Ok(Playback::Http(location.to_string()))
    }
}

/// CUE sheets: a sheet lists its tracks, and each plays a part of a file.
#[derive(Default)]
pub struct CueSource(pub Decoders);

impl CueSource {
    fn sheet(&self, path: &str) -> Result<cue::Sheet, String> {
        cue::read(Path::new(path), &self.0.extensions()).map_err(|e| format!("{path}: {e}"))
    }

    fn track(&self, location: &str) -> Result<(cue::Sheet, cue::Track), String> {
        let (sheet_path, number) = cue::split_location(location).ok_or("not a track of a CUE sheet")?;
        let sheet = self.sheet(sheet_path)?;
        let track = sheet.track(number).cloned().ok_or_else(|| format!("{sheet_path} has no track {number}"))?;
        Ok((sheet, track))
    }

    /// The track's length; the last one of a file runs to the file's end.
    fn length_ms(track: &cue::Track, file_length_ms: Option<u64>) -> Option<u64> {
        track.end_ms.or(file_length_ms).map(|end| end.saturating_sub(track.start_ms))
    }

    fn file_length_ms(&self, track: &cue::Track) -> Option<u64> {
        crate::formats::read_meta(&track.file, &self.0).ok().and_then(|m| m.length_ms)
    }

    /// The file's tags and stream, with what the sheet says of the track.
    fn meta(&self, sheet: &cue::Sheet, track: &cue::Track) -> TrackMeta {
        let mut meta = crate::formats::read_meta(&track.file, &self.0).unwrap_or_default();
        meta.length_ms = Self::length_ms(track, meta.length_ms);
        meta.title = track.title.clone().or(meta.title);
        meta.artist = track.performer.clone().or(sheet.performer.clone()).or(meta.artist);
        meta.album = sheet.title.clone().or(meta.album);
        meta.album_artist = sheet.performer.clone().or(meta.album_artist);
        meta.track = Some(track.number.to_string());
        // The file's own track gain is the whole file's.
        meta.replaygain_track_gain = None;
        meta.replaygain_track_peak = None;
        meta
    }

    /// The tracks of the sheet at `location`; with `only`, those of that one file.
    fn items(&self, location: &str, only: Option<&Path>) -> Result<Vec<Item>, String> {
        let sheet = self.sheet(location)?;
        let items = sheet.tracks.iter().filter(|t| only.is_none_or(|f| t.file == f)).map(|t| {
            let performer = t.performer.as_ref().or(sheet.performer.as_ref());
            let title = match (performer, &t.title) {
                (Some(p), Some(t)) => Some(format!("{p} - {t}")),
                (None, t) => t.clone(),
                (Some(_), None) => None,
            };
            let file_length = if t.end_ms.is_none() { self.file_length_ms(t) } else { None };
            Item { location: cue::track_location(location, t.number), title, length_ms: Self::length_ms(t, file_length) }
        });
        Ok(items.collect())
    }
}

impl Source for CueSource {
    fn id(&self) -> &str {
        "cue"
    }

    fn handles(&self, location: &str) -> bool {
        !is_url(location) && (cue::is_cue(location) || cue::split_location(location).is_some())
    }

    fn expand(&self, location: &str, _cx: &Context) -> Result<Option<Vec<Item>>, String> {
        if !cue::is_cue(location) {
            return Ok(None);
        }
        self.items(location, None).map(Some)
    }

    fn describe(&self, location: &str) -> Option<Result<TrackMeta, String>> {
        Some(self.track(location).map(|(sheet, track)| self.meta(&sheet, &track)))
    }

    fn open(&self, location: &str, _cx: &Context) -> Result<Playback, String> {
        let (sheet, track) = self.track(location)?;
        let meta = Box::new(self.meta(&sheet, &track));
        Ok(Playback::Part { path: track.file, start_ms: track.start_ms, end_ms: track.end_ms, meta })
    }
}

/// Local files, folders and playlist files.
#[derive(Default)]
pub struct FileSource(pub Decoders);

impl Source for FileSource {
    fn id(&self) -> &str {
        "file"
    }

    fn handles(&self, location: &str) -> bool {
        !is_url(location)
    }

    fn expand(&self, location: &str, _cx: &Context) -> Result<Option<Vec<Item>>, String> {
        let path = Path::new(location);
        if path.is_dir() {
            let mut files = Vec::new();
            walk(path, &mut files, 0, &self.0);
            files.sort_by(|a, b| text::natural_cmp(a, b));
            return Ok(Some(files.into_iter().flat_map(|f| self.listed(f)).collect()));
        }
        if is_playlist(location) {
            return plformats::read_file(path).map(Some).map_err(|e| e.to_string());
        }
        // An album file given alone, as a folder lists it: by the sheet that cuts it.
        let sheet = sheets_over_files(sheets_beside(path), vec![location.to_string()], &self.0).into_iter().find(|f| cue::is_cue(f));
        match sheet {
            Some(sheet) => CueSource(self.0.clone()).items(&sheet, Some(path)).map(Some),
            None => Ok(self.inside(location)),
        }
    }

    fn describe(&self, location: &str) -> Option<Result<TrackMeta, String>> {
        Some(crate::formats::read_meta(Path::new(location), &self.0))
    }

    fn open(&self, location: &str, _cx: &Context) -> Result<Playback, String> {
        Ok(Playback::File(PathBuf::from(location)))
    }
}

impl FileSource {
    /// What a folder lists for one of its entries: a sheet's tracks, an
    /// album file's tracks by the sheet inside it, or the file.
    fn listed(&self, f: String) -> Vec<Item> {
        if cue::is_cue(&f) {
            return CueSource(self.0.clone()).items(&f, None).unwrap_or_else(|e| {
                log::warn!("cannot read {e}");
                Vec::new()
            });
        }
        self.inside(&f).unwrap_or_else(|| vec![Item { location: f, title: None, length_ms: None }])
    }

    /// The tracks of the sheet inside an album file, if it has one.
    fn inside(&self, file: &str) -> Option<Vec<Item>> {
        if !cue::may_carry_sheet(file) {
            return None;
        }
        CueSource(self.0.clone()).items(file, None).ok().filter(|items| !items.is_empty())
    }
}

fn walk(dir: &Path, out: &mut Vec<String>, depth: usize, decoders: &Decoders) {
    if depth > 32 {
        return;
    }
    let Ok(rd) = std::fs::read_dir(dir) else { return };
    let (mut files, mut sheets) = (Vec::new(), Vec::new());
    for e in rd.flatten() {
        let p = e.path();
        let Ok(ft) = e.file_type() else { continue };
        if ft.is_dir() || (ft.is_symlink() && p.is_dir()) {
            walk(&p, out, depth + 1, decoders);
        } else {
            let s = p.to_string_lossy().into_owned();
            if cue::is_cue(&s) {
                sheets.push(s);
            } else if decoders.plays(&s) {
                files.push(s);
            }
        }
    }
    out.extend(sheets_over_files(sheets, files, decoders));
}

/// The CUE sheets in the folder of `file`.
fn sheets_beside(file: &Path) -> Vec<String> {
    let Some(rd) = file.parent().and_then(|dir| std::fs::read_dir(dir).ok()) else { return Vec::new() };
    rd.flatten().map(|e| e.path().to_string_lossy().into_owned()).filter(|s| cue::is_cue(s)).collect()
}

/// A folder's tracks: each audio file a sheet cuts into tracks is listed
/// by that sheet, the rest as they are. Of two sheets of one file (a rip
/// often has one for the WAV and one for the FLAC) the first is taken.
fn sheets_over_files(mut sheets: Vec<String>, mut files: Vec<String>, decoders: &Decoders) -> Vec<String> {
    sheets.sort_by(|a, b| text::natural_cmp(a, b));
    let mut taken = Vec::new();
    for sheet in sheets {
        let Ok(read) = CueSource(decoders.clone()).sheet(&sheet) else { continue };
        let cut: Vec<String> = read.tracks.iter().map(|t| t.file.to_string_lossy().into_owned()).collect();
        let before = files.len();
        files.retain(|f| !cut.contains(f));
        if files.len() < before {
            taken.push(sheet);
        }
    }
    taken.extend(files);
    taken
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A source that takes `test:` locations.
    struct Test;

    impl Source for Test {
        fn id(&self) -> &str {
            "test"
        }
        fn handles(&self, location: &str) -> bool {
            location.starts_with("test:")
        }
        fn expand(&self, _: &str, _: &Context) -> Result<Option<Vec<Item>>, String> {
            Ok(None)
        }
        fn describe(&self, _: &str) -> Option<Result<TrackMeta, String>> {
            None
        }
        fn open(&self, _: &str, _: &Context) -> Result<Playback, String> {
            Err("test".into())
        }
    }

    #[test]
    fn each_location_goes_to_its_source_and_added_ones_are_asked_first() {
        let sources = Sources::default();
        assert_eq!(sources.of("test:1").id(), "file");
        sources.set_plugins(vec![Arc::new(Test)], Decoders::default());
        for (loc, id) in [
            ("test:1", "test"),
            ("http://radio:8000/stream", "http"),
            ("https://example.com/list.m3u", "http"),
            ("/music/a.mp3", "file"),
            ("relative/b.flac", "file"),
            ("/music/live.cue", "cue"),
            ("/music/live.cue#2", "cue"),
        ] {
            assert_eq!(sources.of(loc).id(), id, "{loc}");
        }
        sources.set_plugins(Vec::new(), Decoders::default());
        assert_eq!(sources.of("test:1").id(), "file");
    }

    #[test]
    fn a_track_opened_while_plugins_load_waits_for_them() {
        let sources = Arc::new(Sources::waiting());
        assert_eq!(sources.of_now("test:1").id(), "file", "the UI does not wait");
        let asker = {
            let sources = sources.clone();
            std::thread::spawn(move || sources.of("test:1").id().to_string())
        };
        std::thread::sleep(std::time::Duration::from_millis(100));
        sources.set_plugins(vec![Arc::new(Test)], Decoders::default());
        assert_eq!(asker.join().unwrap(), "test");
    }

    #[test]
    fn a_folder_expands_to_its_audio_files_in_natural_order() {
        let dir = std::env::temp_dir().join(format!("popamp-src-{}", std::process::id()));
        std::fs::create_dir_all(dir.join("sub")).unwrap();
        for f in ["10.mp3", "9.mp3", "notes.txt", "sub/1.flac"] {
            std::fs::write(dir.join(f), b"").unwrap();
        }
        let items = FileSource::default().expand(&dir.to_string_lossy(), &Context::default()).unwrap().unwrap();
        let names: Vec<_> = items.iter().map(|i| Path::new(&i.location).strip_prefix(&dir).unwrap().to_path_buf()).collect();
        assert_eq!(names, [Path::new("9.mp3"), Path::new("10.mp3"), &Path::new("sub").join("1.flac")]);
        assert_eq!(FileSource::default().expand(&dir.join("9.mp3").to_string_lossy(), &Context::default()), Ok(None));
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn a_cue_sheet_lists_its_tracks_and_each_plays_its_part() {
        let dir = std::env::temp_dir().join(format!("popamp-cue-src-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let fixture = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/silence.wav");
        std::fs::copy(&fixture, dir.join("album.wav")).unwrap();
        let file_ms = crate::read_meta(&dir.join("album.wav"), &Decoders::default()).unwrap().length_ms.unwrap();
        let sheet = "PERFORMER \"Band\"\nTITLE \"Album\"\nFILE \"album.wav\" WAVE\n  TRACK 01 AUDIO\n    TITLE \"One\"\n    INDEX 01 00:00:00\n  TRACK 02 AUDIO\n    TITLE \"Two\"\n    INDEX 01 00:00:15\n";
        let cue = dir.join("album.cue");
        std::fs::write(&cue, sheet).unwrap();
        let cue = cue.to_string_lossy().into_owned();
        let items = CueSource::default().expand(&cue, &Context::default()).unwrap().unwrap();
        let second = format!("{cue}#2");
        assert_eq!(items.iter().map(|i| i.location.as_str()).collect::<Vec<_>>(), [format!("{cue}#1").as_str(), second.as_str()]);
        assert_eq!((items[0].title.as_deref(), items[0].length_ms), (Some("Band - One"), Some(200)));
        assert_eq!(items[1].length_ms, Some(file_ms - 200), "the last track runs to the end");
        let meta = CueSource::default().describe(&second).unwrap().unwrap();
        assert_eq!(
            (meta.title.as_deref(), meta.artist.as_deref(), meta.album.as_deref(), meta.track.as_deref()),
            (Some("Two"), Some("Band"), Some("Album"), Some("2"))
        );
        assert_eq!(
            CueSource::default().open(&second, &Context::default()),
            Ok(Playback::Part { path: dir.join("album.wav"), start_ms: 200, end_ms: None, meta: Box::new(meta) })
        );
        assert!(CueSource::default().open(&format!("{cue}#9"), &Context::default()).is_err());
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn a_folder_lists_an_album_file_by_its_cue_sheet_once() {
        let dir = std::env::temp_dir().join(format!("popamp-cue-dir-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let fixture = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/silence.wav");
        std::fs::copy(&fixture, dir.join("Альбом.wav")).unwrap();
        std::fs::write(dir.join("single.mp3"), b"").unwrap();
        // Two sheets of one rip, as EAC leaves them: one names the WAV it was ripped to.
        let sheet = |file: &str| {
            format!("FILE \"{file}\" WAVE\n  TRACK 01 AUDIO\n    TITLE \"Тучи\"\n    INDEX 01 00:00:00\n  TRACK 02 AUDIO\n    INDEX 01 00:00:15\n")
        };
        for (name, file) in [("Альбом.cue", "Альбом.flac"), ("Альбом (WAV).cue", "Альбом.wav")] {
            std::fs::write(dir.join(name), encoding_rs::WINDOWS_1251.encode(&sheet(file)).0).unwrap();
        }
        let items = FileSource::default().expand(&dir.to_string_lossy(), &Context::default()).unwrap().unwrap();
        let names: Vec<_> = items
            .iter()
            .map(|i| i.location.strip_prefix(&*dir.to_string_lossy()).unwrap().trim_start_matches(['/', '\\']).to_string())
            .collect();
        assert_eq!(names, ["single.mp3", "Альбом (WAV).cue#1", "Альбом (WAV).cue#2"]);
        assert_eq!(items[1].title.as_deref(), Some("Тучи"));
        let album = FileSource::default().expand(&dir.join("Альбом.wav").to_string_lossy(), &Context::default()).unwrap();
        assert_eq!(album.map(|a| a.len()), Some(2), "the album file given alone is listed by its sheet too");
        assert_eq!(FileSource::default().expand(&dir.join("single.mp3").to_string_lossy(), &Context::default()), Ok(None));
        std::fs::remove_dir_all(dir).unwrap();
    }

    /// A decoder plug-in for `.ape` that is never asked to play.
    struct Ape(Vec<String>);

    impl crate::formats::FileDecoder for Ape {
        fn extensions(&self) -> &[String] {
            &self.0
        }
        fn open(&self, _: &Path) -> Result<(Box<dyn crate::decoder::AudioSource>, TrackMeta), String> {
            Err("not here".into())
        }
    }

    #[test]
    fn a_sheet_of_a_wav_finds_the_album_in_a_format_of_a_plugin() {
        let dir = std::env::temp_dir().join(format!("popamp-cue-ape-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("album.ape"), b"").unwrap();
        let sheet = "FILE \"album.wav\" WAVE\n  TRACK 01 AUDIO\n    INDEX 01 00:00:00\n  TRACK 02 AUDIO\n    INDEX 01 00:00:15\n";
        std::fs::write(dir.join("album.cue"), sheet).unwrap();
        let files = FileSource(Decoders::new(vec![Arc::new(Ape(vec!["ape".into()]))]));
        let items = files.expand(&dir.to_string_lossy(), &Context::default()).unwrap().unwrap();
        let names: Vec<_> = items
            .iter()
            .map(|i| i.location.strip_prefix(&*dir.to_string_lossy()).unwrap().trim_start_matches(['/', '\\']).to_string())
            .collect();
        assert_eq!(names, ["album.cue#1", "album.cue#2"]);
        let alone = files.expand(&dir.join("album.ape").to_string_lossy(), &Context::default()).unwrap();
        assert_eq!(alone.map(|a| a.len()), Some(2));
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn an_album_file_with_its_sheet_inside_lists_its_tracks() {
        let dir = std::env::temp_dir().join(format!("popamp-cue-inside-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        // A FLAC file's head with one block: comments with the sheet.
        let tag = "CUESHEET=FILE \"a.wav\" WAVE\n  TRACK 01 AUDIO\n    TITLE \"One\"\n    INDEX 01 00:00:00\n  TRACK 02 AUDIO\n    INDEX 01 00:00:15\n";
        let mut comments = 0u32.to_le_bytes().to_vec();
        comments.extend_from_slice(&1u32.to_le_bytes());
        comments.extend_from_slice(&(tag.len() as u32).to_le_bytes());
        comments.extend_from_slice(tag.as_bytes());
        let mut flac = b"fLaC\x84".to_vec();
        flac.extend_from_slice(&(comments.len() as u32).to_be_bytes()[1..]);
        flac.extend_from_slice(&comments);
        std::fs::write(dir.join("album.flac"), flac).unwrap();
        std::fs::write(dir.join("plain.flac"), b"").unwrap();
        let items = FileSource::default().expand(&dir.to_string_lossy(), &Context::default()).unwrap().unwrap();
        let names: Vec<_> = items
            .iter()
            .map(|i| i.location.strip_prefix(&*dir.to_string_lossy()).unwrap().trim_start_matches(['/', '\\']).to_string())
            .collect();
        assert_eq!(names, ["album.flac#1", "album.flac#2", "plain.flac"]);
        assert_eq!(items[0].title.as_deref(), Some("One"));
        let album = dir.join("album.flac").to_string_lossy().into_owned();
        assert_eq!(FileSource::default().expand(&album, &Context::default()).unwrap().map(|a| a.len()), Some(2));
        assert_eq!(FileSource::default().expand(&dir.join("plain.flac").to_string_lossy(), &Context::default()), Ok(None));
        let second = format!("{album}#2");
        assert!(CueSource::default().handles(&second));
        let Ok(Playback::Part { path, start_ms, .. }) = CueSource::default().open(&second, &Context::default()) else { panic!() };
        assert_eq!((path, start_ms), (dir.join("album.flac"), 200));
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn a_stream_that_is_not_a_playlist_is_one_track() {
        assert_eq!(HttpSource.expand("http://radio:8000/stream", &Context::default()), Ok(None));
        assert_eq!(HttpSource.open("http://radio:8000/stream", &Context::default()), Ok(Playback::Http("http://radio:8000/stream".into())));
    }
}
