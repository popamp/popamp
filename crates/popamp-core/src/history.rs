//! What was played, newest last, kept in `history.txt`.
//!
//! Each play is one appended line, `secs<TAB>location<TAB>title`, so a
//! crash loses at most the line being written. Loading keeps the newest
//! [`LIMIT`] plays and rewrites the file once it holds twice that many.

use std::collections::VecDeque;
use std::path::Path;

use crate::text::{escape_field as escape, unescape_field as unescape};

/// How many plays are kept.
pub const LIMIT: usize = 5000;

/// One track started.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Played {
    /// Seconds since the Unix epoch.
    pub at: u64,
    pub location: String,
    /// The title the playlist showed then.
    pub title: String,
}

impl Played {
    fn line(&self) -> String {
        format!("{}\t{}\t{}\n", self.at, escape(&self.location), escape(&self.title))
    }

    fn parse(line: &str) -> Option<Played> {
        let mut parts = line.splitn(3, '\t');
        let at = parts.next()?.parse().ok()?;
        let location = unescape(parts.next().filter(|l| !l.is_empty())?);
        let title = unescape(parts.next()?);
        Some(Played { at, location, title })
    }
}

#[derive(Debug, Default)]
pub struct History {
    plays: VecDeque<Played>,
}

impl History {
    /// The plays in `text`; broken lines are skipped.
    pub fn parse(text: &str) -> History {
        let mut h = History::default();
        text.lines().filter_map(Played::parse).for_each(|p| h.push(p));
        h
    }

    /// Reads `path`, compacting it when it has grown past twice the limit.
    /// A missing file is an empty history.
    pub fn load(path: &Path) -> History {
        let text = std::fs::read_to_string(path).unwrap_or_default();
        let h = History::parse(&text);
        if text.lines().count() > 2 * LIMIT {
            let all: String = h.plays.iter().map(Played::line).collect();
            if let Err(e) = std::fs::write(path, all) {
                log::warn!("cannot compact {}: {e}", path.display());
            }
        }
        h
    }

    /// Adds a play and appends it to `path`.
    pub fn record(&mut self, path: &Path, played: Played) {
        if let Err(e) = crate::files::append(path, &played.line()) {
            log::warn!("cannot write {}: {e}", path.display());
        }
        self.push(played);
    }

    fn push(&mut self, played: Played) {
        if self.plays.len() == LIMIT {
            self.plays.pop_front();
        }
        self.plays.push_back(played);
    }

    /// Newest first.
    pub fn newest_first(&self) -> impl Iterator<Item = &Played> {
        self.plays.iter().rev()
    }

    pub fn len(&self) -> usize {
        self.plays.len()
    }

    pub fn is_empty(&self) -> bool {
        self.plays.is_empty()
    }
}

/// Seconds since the Unix epoch, now.
pub fn now() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn played(at: u64, location: &str, title: &str) -> Played {
        Played { at, location: location.into(), title: title.into() }
    }

    fn file(name: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("popamp-history-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        dir.join("history.txt")
    }

    #[test]
    fn plays_survive_a_restart_newest_first() {
        let path = file("restart");
        let mut h = History::load(&path);
        h.record(&path, played(100, "/music/a.flac", "A - One"));
        h.record(&path, played(200, "http://radio.example/live", "Radio"));
        let again = History::load(&path);
        let _ = std::fs::remove_dir_all(path.parent().unwrap());
        let titles: Vec<_> = again.newest_first().map(|p| (p.at, p.title.as_str())).collect();
        assert_eq!(titles, [(200, "Radio"), (100, "A - One")]);
    }

    #[test]
    fn odd_characters_keep_to_their_line() {
        let odd = played(5, "/music/tab\there\\x.mp3", "line\nbreak\r and \\t");
        let h = History::parse(&odd.line());
        assert_eq!(h.newest_first().next(), Some(&odd));
    }

    #[test]
    fn broken_lines_are_skipped() {
        let h = History::parse("garbage\n12\t\tno location\n7\t/a.mp3\tA\nx\t/b.mp3\tB\n8\t/c.mp3\n");
        let locations: Vec<_> = h.newest_first().map(|p| p.location.as_str()).collect();
        assert_eq!(locations, ["/a.mp3"]);
    }

    #[test]
    fn only_the_newest_plays_are_kept_and_the_file_is_compacted() {
        let path = file("limit");
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        let lines: String = (0..2 * LIMIT as u64 + 1).map(|i| played(i, &format!("/{i}.mp3"), "").line()).collect();
        std::fs::write(&path, lines).unwrap();
        let h = History::load(&path);
        let kept = std::fs::read_to_string(&path).unwrap().lines().count();
        let _ = std::fs::remove_dir_all(path.parent().unwrap());
        assert_eq!((h.len(), kept), (LIMIT, LIMIT));
        assert_eq!(h.newest_first().next().map(|p| p.at), Some(2 * LIMIT as u64));
    }
}
