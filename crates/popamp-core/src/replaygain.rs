//! ReplayGain values Popamp measured itself, kept in `replaygain.txt`.
//!
//! Popamp never writes them into the tracks: the scanner's results live
//! here, one line per track, `location<TAB>size<TAB>mtime<TAB>track gain
//! <TAB>track peak<TAB>album gain<TAB>album peak` (the album pair may be
//! empty). A track whose file changed size or time since is measured anew.

use crate::meta::TrackMeta;
use crate::text::{escape_field, unescape_field};
use std::collections::BTreeMap;
use std::path::Path;

/// The loudness ReplayGain 2.0 brings tracks to, in LUFS.
pub const REFERENCE_LUFS: f64 = -18.0;

/// A gain and the peak it must not push past full scale.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Level {
    pub gain_db: f32,
    /// The highest sample, 1.0 being full scale.
    pub peak: f32,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Gains {
    pub track: Level,
    /// None for a track that is not part of a scanned album.
    pub album: Option<Level>,
}

impl Gains {
    /// Puts these values in place of the ones the tags gave: a scan is
    /// what the user asked for last.
    pub fn apply(&self, meta: &mut TrackMeta) {
        meta.replaygain_track_gain = Some(self.track.gain_db);
        meta.replaygain_track_peak = Some(self.track.peak);
        meta.replaygain_album_gain = self.album.map(|a| a.gain_db);
        meta.replaygain_album_peak = self.album.map(|a| a.peak);
    }
}

/// What a file looked like when it was measured.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Stamp {
    pub size: u64,
    /// Seconds since the Unix epoch.
    pub modified: u64,
}

impl Stamp {
    /// None for a file that cannot be read.
    pub fn of(path: &Path) -> Option<Stamp> {
        let m = std::fs::metadata(path).ok()?;
        let modified = m.modified().ok()?.duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs());
        Some(Stamp { size: m.len(), modified })
    }
}

/// The measured tracks, by location.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Scanned {
    tracks: BTreeMap<String, (Stamp, Gains)>,
}

impl Scanned {
    /// The tracks in `text`; broken lines are skipped.
    pub fn parse(text: &str) -> Scanned {
        Scanned { tracks: text.lines().filter_map(parse_line).collect() }
    }

    /// A missing file is no scans.
    pub fn load(path: &Path) -> Scanned {
        Scanned::parse(&std::fs::read_to_string(path).unwrap_or_default())
    }

    /// Writes the whole store; a crash midway leaves the old file.
    pub fn save(&self, path: &Path) -> std::io::Result<()> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let tmp = path.with_extension("txt.tmp");
        std::fs::write(&tmp, self.to_text())?;
        std::fs::rename(&tmp, path)
    }

    fn to_text(&self) -> String {
        self.tracks.iter().map(|(location, (stamp, gains))| line(location, stamp, gains)).collect()
    }

    pub fn insert(&mut self, location: String, stamp: Stamp, gains: Gains) {
        self.tracks.insert(location, (stamp, gains));
    }

    /// The values for `location`, unless its file has changed since.
    pub fn get(&self, location: &str, now: Stamp) -> Option<&Gains> {
        self.tracks.get(location).filter(|(stamp, _)| *stamp == now).map(|(_, gains)| gains)
    }

    /// Applies the values for `location`, whose audio is in `file`.
    pub fn apply(&self, location: &str, file: &Path, meta: &mut TrackMeta) {
        if self.tracks.is_empty() {
            return;
        }
        if let Some(gains) = Stamp::of(file).and_then(|now| self.get(location, now)) {
            gains.apply(meta);
        }
    }

    pub fn len(&self) -> usize {
        self.tracks.len()
    }

    pub fn is_empty(&self) -> bool {
        self.tracks.is_empty()
    }
}

fn line(location: &str, stamp: &Stamp, gains: &Gains) -> String {
    let album = gains.album.map_or(String::from("\t"), |a| format!("{}\t{}", a.gain_db, a.peak));
    format!("{}\t{}\t{}\t{}\t{}\t{album}\n", escape_field(location), stamp.size, stamp.modified, gains.track.gain_db, gains.track.peak)
}

fn parse_line(line: &str) -> Option<(String, (Stamp, Gains))> {
    let f: Vec<&str> = line.split('\t').collect();
    let [location, size, modified, gain, peak, album_gain, album_peak] = f[..] else { return None };
    let num = |s: &str| s.parse::<f32>().ok().filter(|v| v.is_finite());
    let stamp = Stamp { size: size.parse().ok()?, modified: modified.parse().ok()? };
    let track = Level { gain_db: num(gain)?, peak: num(peak)? };
    let album = match (album_gain, album_peak) {
        ("", "") => None,
        (g, p) => Some(Level { gain_db: num(g)?, peak: num(p)? }),
    };
    let location = Some(unescape_field(location)).filter(|l| !l.is_empty())?;
    Some((location, (stamp, Gains { track, album })))
}

#[cfg(test)]
mod tests {
    use super::*;

    const STAMP: Stamp = Stamp { size: 1234, modified: 1_794_000_000 };

    fn gains(album: bool) -> Gains {
        let track = Level { gain_db: -7.25, peak: 0.98 };
        Gains { track, album: album.then_some(Level { gain_db: -6.5, peak: 1.0 }) }
    }

    #[test]
    fn scans_survive_a_restart() {
        let dir = std::env::temp_dir().join(format!("popamp-rg-{}", std::process::id()));
        let path = dir.join("replaygain.txt");
        let mut s = Scanned::default();
        s.insert("/music/a\tb.flac".into(), STAMP, gains(true));
        s.insert("album.cue#2".into(), STAMP, gains(false));
        s.save(&path).unwrap();
        let again = Scanned::load(&path);
        let _ = std::fs::remove_dir_all(&dir);
        assert_eq!(again, s);
        assert_eq!(again.get("album.cue#2", STAMP), Some(&gains(false)));
    }

    #[test]
    fn a_changed_file_has_no_values() {
        let mut s = Scanned::default();
        s.insert("/a.mp3".into(), STAMP, gains(false));
        assert_eq!(s.get("/a.mp3", Stamp { modified: STAMP.modified + 1, ..STAMP }), None);
        assert_eq!(s.get("/a.mp3", Stamp { size: 1, ..STAMP }), None);
        assert_eq!(s.get("/b.mp3", STAMP), None);
    }

    #[test]
    fn broken_lines_are_skipped() {
        let good = line("/ok.mp3", &STAMP, &gains(true));
        let text = format!("junk\n\t1\t2\t-1\t1\t\t\n/x.mp3\t1\t2\tNaN\t1\t\t\n/y.mp3\t1\t2\t-1\t1\t-2\t\n{good}");
        let s = Scanned::parse(&text);
        assert_eq!((s.len(), s.get("/ok.mp3", STAMP)), (1, Some(&gains(true))));
    }

    #[test]
    fn scanned_values_replace_the_tags() {
        let mut meta = TrackMeta { replaygain_album_gain: Some(3.0), replaygain_track_gain: Some(2.0), ..Default::default() };
        gains(false).apply(&mut meta);
        let got = (meta.replaygain_track_gain, meta.replaygain_track_peak, meta.replaygain_album_gain, meta.replaygain_album_peak);
        assert_eq!(got, (Some(-7.25), Some(0.98), None, None));
    }

    #[test]
    fn values_apply_while_the_file_is_unchanged() {
        let dir = std::env::temp_dir().join(format!("popamp-rg-apply-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("a.mp3");
        std::fs::write(&file, b"abc").unwrap();
        let mut s = Scanned::default();
        s.insert("/a.mp3".into(), Stamp::of(&file).unwrap(), gains(false));
        let mut meta = TrackMeta::default();
        s.apply("/a.mp3", &file, &mut meta);
        assert_eq!(meta.replaygain_track_gain, Some(-7.25));
        std::fs::write(&file, b"abcd").unwrap();
        let mut meta = TrackMeta::default();
        s.apply("/a.mp3", &file, &mut meta);
        let _ = std::fs::remove_dir_all(&dir);
        assert_eq!(meta.replaygain_track_gain, None, "the file changed");
    }
}
