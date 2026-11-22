//! The library's own playlists: tracks put there by hand, in the order
//! they were put. Each is an M3U8 file in Popamp's `playlists` folder,
//! named as the list is.

use crate::library::Library;
use crate::plformats::{self, Item};
use std::collections::HashMap;
use std::io;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct PlainList {
    pub name: String,
    /// The tracks' paths, each once.
    pub tracks: Vec<String>,
}

/// What a list may be called: a file name, so no slashes or the other
/// characters a file name cannot have on some systems. `None` when nothing
/// is left of `name`.
pub fn clean_name(name: &str) -> Option<String> {
    let bad = |c: char| c.is_control() || "/\\:*?\"<>|".contains(c);
    let name: String = name.chars().map(|c| if bad(c) { '_' } else { c }).collect();
    let name = name.trim().trim_matches('.').trim();
    (!name.is_empty()).then(|| name.to_string())
}

/// Every list in `dir`, by name; none when there is no `dir`.
pub fn load(dir: &Path) -> Vec<PlainList> {
    let Ok(entries) = std::fs::read_dir(dir) else { return Vec::new() };
    let mut lists: Vec<PlainList> = entries
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|x| x == "m3u8"))
        .filter_map(|p| {
            let name = p.file_stem()?.to_string_lossy().into_owned();
            let items = plformats::read_file(&p).map_err(|e| log::warn!("cannot read {}: {e}", p.display())).ok()?;
            let mut list = PlainList { name, tracks: Vec::new() };
            list.add(items.into_iter().map(|i| i.location));
            Some(list)
        })
        .collect();
    lists.sort_by_key(|l| l.name.to_lowercase());
    lists
}

impl PlainList {
    /// The file the list is kept in.
    pub fn file(&self, dir: &Path) -> PathBuf {
        dir.join(format!("{}.m3u8", self.name))
    }

    pub fn save(&self, dir: &Path) -> io::Result<()> {
        std::fs::create_dir_all(dir)?;
        let items: Vec<Item> = self.tracks.iter().map(|t| Item { location: t.clone(), title: None, length_ms: None }).collect();
        plformats::write_file(&self.file(dir), &items)
    }

    /// Puts `paths` at the end, but none that is there already. Returns
    /// how many went in.
    pub fn add(&mut self, paths: impl IntoIterator<Item = String>) -> usize {
        let before = self.tracks.len();
        for p in paths {
            if !self.tracks.contains(&p) {
                self.tracks.push(p);
            }
        }
        self.tracks.len() - before
    }

    pub fn remove(&mut self, paths: &[String]) {
        self.tracks.retain(|t| !paths.contains(t));
    }

    /// The list's tracks that are in `lib`, as places in `lib.tracks`, in
    /// the list's order; with `query`, only those that have its words.
    pub fn tracks(&self, lib: &Library, query: &str) -> Vec<usize> {
        let at: HashMap<&str, usize> = lib.tracks.iter().enumerate().map(|(i, t)| (t.path.as_str(), i)).collect();
        let words: Vec<String> = query.split_whitespace().map(str::to_lowercase).collect();
        self.tracks.iter().filter_map(|p| at.get(p.as_str()).copied()).filter(|&i| words.is_empty() || lib.tracks[i].has(&words)).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::library::Track;

    fn dir(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("popamp-plain-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        d
    }

    #[test]
    fn a_name_is_one_a_file_can_have() {
        assert_eq!(clean_name("  Road / trip?  ").as_deref(), Some("Road _ trip_"));
        assert_eq!(clean_name("..."), None);
        assert_eq!(clean_name("   "), None);
    }

    #[test]
    fn lists_are_kept_as_files_and_read_back_by_name() {
        let d = dir("keep");
        assert!(load(&d).is_empty(), "no folder, no lists");
        // Whole paths on every system, as the library has them.
        let at = |name: &str| d.join("music").join(name).to_string_lossy().into_owned();
        let mut b = PlainList { name: "beta".into(), ..PlainList::default() };
        assert_eq!(b.add([at("2.mp3"), at("1.mp3"), at("2.mp3")]), 2, "each track once");
        b.save(&d).unwrap();
        PlainList { name: "Alpha".into(), ..PlainList::default() }.save(&d).unwrap();
        let lists = load(&d);
        assert_eq!(lists.iter().map(|l| l.name.as_str()).collect::<Vec<_>>(), ["Alpha", "beta"]);
        assert_eq!(lists[1].tracks, [at("2.mp3"), at("1.mp3")], "in the order they were put");
        std::fs::remove_dir_all(&d).unwrap();
    }

    #[test]
    fn a_list_shows_its_tracks_the_library_has_in_its_order() {
        let track = |path: &str, title: &str| Track { path: path.into(), title: title.into(), ..Track::default() };
        let lib = Library { tracks: vec![track("/m/1.mp3", "One"), track("/m/2.mp3", "Two")], ..Library::default() };
        let mut list = PlainList::default();
        list.add(["/m/2.mp3".to_string(), "/gone.mp3".into(), "/m/1.mp3".into()]);
        assert_eq!(list.tracks(&lib, ""), [1, 0]);
        assert_eq!(list.tracks(&lib, "one"), [0]);
        list.remove(&["/m/2.mp3".into()]);
        assert_eq!(list.tracks(&lib, ""), [0]);
    }
}
