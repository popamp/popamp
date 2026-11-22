//! How often each track was listened to and how the user rated it, kept
//! in `stats.txt`.
//!
//! The file is lines of tab-separated fields. `S<TAB>plays<TAB>last<TAB>
//! rating<TAB>missing<TAB>path` is a track's whole record, written when
//! the file is rewritten. Between rewrites each play and each rating is
//! one appended line, `P<TAB>secs<TAB>path` and `R<TAB>stars<TAB>path`, so
//! a crash loses at most the line being written. Audio files are never
//! touched.

use std::collections::HashMap;
use std::path::Path;

use crate::text::{escape_field as escape, unescape_field as unescape};

/// How long the record of a file gone missing waits for it, in seconds.
pub const KEEP_MISSING: u64 = 30 * 86_400;

/// Appended lines allowed beyond the records before loading rewrites the
/// file.
const SLACK: usize = 1000;

/// Stars from 1 to 5; none means the user has not rated the track.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Default)]
pub struct Rating(u8);

impl Rating {
    pub const NONE: Rating = Rating(0);
    pub const MAX: u8 = 5;

    /// None when `stars` is more than [`Rating::MAX`].
    pub fn new(stars: u8) -> Option<Rating> {
        (stars <= Self::MAX).then_some(Rating(stars))
    }

    pub fn stars(self) -> u8 {
        self.0
    }
}

/// What is known of one track.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct TrackStats {
    /// Times it was listened to long enough to count.
    pub plays: u32,
    /// When it last counted, in seconds since the Unix epoch; 0 = never.
    pub last_played: u64,
    pub rating: Rating,
    /// When a scan last found its file gone, in seconds since the Unix
    /// epoch; the record waits for the file to come back (an unplugged
    /// drive) or to be found under another name.
    pub missing_since: Option<u64>,
}

impl TrackStats {
    /// Whether the record holds nothing worth keeping.
    fn is_empty(&self) -> bool {
        self.plays == 0 && self.rating == Rating::NONE
    }
}

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Stats {
    tracks: HashMap<String, TrackStats>,
    /// Lines appended since the file was last written whole.
    appended: usize,
}

impl Stats {
    /// The stats in `text`; broken lines are skipped.
    pub fn parse(text: &str) -> Stats {
        let mut s = Stats::default();
        for line in text.lines() {
            s.apply(line);
        }
        s
    }

    fn apply(&mut self, line: &str) {
        let fields: Vec<&str> = line.split('\t').collect();
        let (kind, path) = match fields.as_slice() {
            [kind, .., path] if !path.is_empty() => (*kind, unescape(path)),
            _ => return,
        };
        let changed = match (kind, &fields[1..fields.len() - 1]) {
            ("S", record) => {
                if let Some(r) = parse_record(record) {
                    self.tracks.insert(path, r);
                }
                return;
            }
            ("P", [at]) => at.parse().ok().map(|at| self.count_play(path, at)),
            ("R", [stars]) => stars.parse().ok().and_then(Rating::new).map(|r| self.set_rating(path, r)),
            _ => None,
        };
        if changed.is_some() {
            self.appended += 1;
        }
    }

    fn count_play(&mut self, path: String, at: u64) {
        let t = self.tracks.entry(path).or_default();
        t.plays += 1;
        t.last_played = t.last_played.max(at);
    }

    fn set_rating(&mut self, path: String, rating: Rating) {
        self.tracks.entry(path).or_default().rating = rating;
    }

    /// Reads `file`, rewriting it once the appended lines outgrow the
    /// records. A missing file is no stats.
    pub fn load(file: &Path) -> Stats {
        let mut s = Stats::parse(&std::fs::read_to_string(file).unwrap_or_default());
        if s.appended > s.tracks.len() + SLACK {
            if let Err(e) = s.save(file) {
                log::warn!("cannot compact {}: {e}", file.display());
            }
        }
        s
    }

    /// Writes every record; a crash midway leaves the old file.
    pub fn save(&mut self, file: &Path) -> std::io::Result<()> {
        crate::files::replace(file, &self.to_text())?;
        self.appended = 0;
        Ok(())
    }

    fn to_text(&self) -> String {
        let mut paths: Vec<&String> = self.tracks.iter().filter(|(_, t)| !t.is_empty()).map(|(p, _)| p).collect();
        paths.sort();
        paths
            .into_iter()
            .map(|p| {
                let t = &self.tracks[p];
                let missing = t.missing_since.map(|m| m.to_string()).unwrap_or_default();
                format!("S\t{}\t{}\t{}\t{missing}\t{}\n", t.plays, t.last_played, t.rating.stars(), escape(p))
            })
            .collect()
    }

    /// What is known of the track at `path`; nothing for a track never
    /// counted or rated.
    pub fn get(&self, path: &str) -> TrackStats {
        self.tracks.get(path).copied().unwrap_or_default()
    }

    /// Counts a play of the track at `path` and appends it to `file`.
    pub fn record_play(&mut self, file: &Path, path: &str, at: u64) {
        self.append(file, format!("P\t{at}\t{}\n", escape(path)));
        self.count_play(path.to_string(), at);
    }

    /// Rates the track at `path` and appends that to `file`.
    pub fn rate(&mut self, file: &Path, path: &str, rating: Rating) {
        self.append(file, format!("R\t{}\t{}\n", rating.stars(), escape(path)));
        self.set_rating(path.to_string(), rating);
    }

    /// Brings the records up to date after a scan: each of `moves`
    /// (old path, new path) takes its record along, a file that is there
    /// again is no longer missing, one gone is marked missing from `now`,
    /// and one missing for longer than `keep` seconds is forgotten.
    pub fn tidy(&mut self, moves: &[(String, String)], exists: impl Fn(&str) -> bool, now: u64, keep: u64) {
        for (old, new) in moves {
            if let Some(from) = self.tracks.remove(old) {
                let to = self.tracks.entry(new.clone()).or_default();
                to.plays += from.plays;
                to.last_played = to.last_played.max(from.last_played);
                if to.rating == Rating::NONE {
                    to.rating = from.rating;
                }
            }
        }
        self.tracks.retain(|path, t| {
            if exists(path) {
                t.missing_since = None;
                return true;
            }
            let since = *t.missing_since.get_or_insert(now);
            now.saturating_sub(since) < keep
        });
    }

    /// How many tracks have plays or a rating.
    pub fn len(&self) -> usize {
        self.tracks.values().filter(|t| !t.is_empty()).count()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// How many records wait for their file.
    pub fn missing(&self) -> usize {
        self.tracks.values().filter(|t| t.missing_since.is_some() && !t.is_empty()).count()
    }

    fn append(&mut self, file: &Path, line: String) {
        if let Err(e) = crate::files::append(file, &line) {
            log::warn!("cannot write {}: {e}", file.display());
        }
        self.appended += 1;
    }
}

/// The fields of an `S` line between its kind and its path.
fn parse_record(fields: &[&str]) -> Option<TrackStats> {
    let [plays, last, rating, missing] = fields else { return None };
    Some(TrackStats {
        plays: plays.parse().ok()?,
        last_played: last.parse().ok()?,
        rating: Rating::new(rating.parse().ok()?)?,
        missing_since: if missing.is_empty() { None } else { Some(missing.parse().ok()?) },
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn file(name: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("popamp-stats-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        dir.join("stats.txt")
    }

    fn stars(n: u8) -> Rating {
        Rating::new(n).unwrap()
    }

    #[test]
    fn plays_and_ratings_survive_a_restart() {
        let path = file("restart");
        let mut s = Stats::load(&path);
        s.record_play(&path, "/music/a.flac", 100);
        s.record_play(&path, "/music/a.flac", 300);
        s.rate(&path, "/music/b\ttab.mp3", stars(4));
        let again = Stats::load(&path);
        let _ = std::fs::remove_dir_all(path.parent().unwrap());
        assert_eq!(again.get("/music/a.flac"), TrackStats { plays: 2, last_played: 300, ..TrackStats::default() });
        assert_eq!(again.get("/music/b\ttab.mp3").rating, stars(4));
        assert_eq!(again.get("/never.mp3"), TrackStats::default());
    }

    #[test]
    fn a_rewritten_file_reads_back_the_same_without_unrated_unplayed_tracks() {
        let mut s = Stats::parse("P\t5\t/a.mp3\nR\t3\t/b.mp3\nR\t2\t/c.mp3\nR\t0\t/c.mp3\n");
        s.tracks.get_mut("/a.mp3").unwrap().missing_since = Some(77);
        let text = s.to_text();
        assert_eq!(text, "S\t1\t5\t0\t77\t/a.mp3\nS\t0\t0\t3\t\t/b.mp3\n");
        assert_eq!(Stats::parse(&text).tracks, s.tracks.into_iter().filter(|(_, t)| !t.is_empty()).collect());
    }

    #[test]
    fn broken_lines_are_skipped() {
        let s = Stats::parse("junk\nP\tx\t/a.mp3\nP\t1\t\nR\t6\t/b.mp3\nS\t1\t2\t3\t\t\nS\t1\t2\t9\t\t/c.mp3\nP\t1\t/ok.mp3\n");
        assert_eq!(s.tracks.keys().collect::<Vec<_>>(), ["/ok.mp3"]);
    }

    #[test]
    fn a_rating_is_zero_to_five_stars() {
        assert_eq!((Rating::new(5).map(Rating::stars), Rating::new(6)), (Some(5), None));
        assert_eq!(Rating::default(), Rating::NONE);
    }

    #[test]
    fn a_record_follows_its_file_and_waits_for_a_missing_one() {
        let mut s = Stats::parse("P\t10\t/old.mp3\nR\t5\t/old.mp3\nP\t20\t/new.mp3\nP\t30\t/unplugged.mp3\nP\t40\t/here.mp3\n");
        let there = ["/new.mp3", "/here.mp3"];
        let day = 86_400;
        s.tidy(&[("/old.mp3".into(), "/new.mp3".into())], |p| there.contains(&p), 1000, KEEP_MISSING);
        assert_eq!(s.get("/new.mp3"), TrackStats { plays: 2, last_played: 20, rating: stars(5), missing_since: None });
        assert_eq!((s.get("/old.mp3"), s.get("/unplugged.mp3").missing_since, s.missing()), (TrackStats::default(), Some(1000), 1));
        s.tidy(&[], |p| there.contains(&p), 1000 + 29 * day, KEEP_MISSING);
        assert_eq!(s.get("/unplugged.mp3").missing_since, Some(1000), "the first time it was missed counts");
        s.tidy(&[], |p| there.contains(&p) || p == "/unplugged.mp3", 1000 + 29 * day, KEEP_MISSING);
        assert_eq!((s.get("/unplugged.mp3").plays, s.missing()), (1, 0), "plugged in again");
        s.tidy(&[], |p| there.contains(&p), 1000 + 30 * day, KEEP_MISSING);
        s.tidy(&[], |p| there.contains(&p), 1000 + 60 * day, KEEP_MISSING);
        assert_eq!(s.get("/unplugged.mp3"), TrackStats::default(), "forgotten after thirty days");
        assert_eq!(s.get("/here.mp3").plays, 1);
    }

    #[test]
    fn cleaning_up_now_forgets_every_missing_record() {
        let mut s = Stats::parse("P\t10\t/gone.mp3\nP\t20\t/here.mp3\n");
        s.tidy(&[], |p| p == "/here.mp3", 50, 0);
        assert_eq!((s.get("/gone.mp3").plays, s.get("/here.mp3").plays), (0, 1));
    }

    #[test]
    fn a_rating_moved_onto_a_rated_track_keeps_the_newer_one() {
        let mut s = Stats::parse("R\t2\t/a.mp3\nR\t4\t/b.mp3\n");
        s.tidy(&[("/a.mp3".into(), "/b.mp3".into())], |_| true, 0, KEEP_MISSING);
        assert_eq!(s.get("/b.mp3").rating, stars(4));
    }

    #[test]
    fn loading_rewrites_a_file_grown_long() {
        let path = file("compact");
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        let lines: String = (0..SLACK as u64 + 2).map(|i| format!("P\t{i}\t/a.mp3\n")).collect();
        std::fs::write(&path, lines).unwrap();
        let s = Stats::load(&path);
        let text = std::fs::read_to_string(&path).unwrap();
        let _ = std::fs::remove_dir_all(path.parent().unwrap());
        assert_eq!(text, format!("S\t{}\t{}\t0\t\t/a.mp3\n", SLACK + 2, SLACK + 1));
        assert_eq!(s.get("/a.mp3").plays as usize, SLACK + 2);
    }
}
