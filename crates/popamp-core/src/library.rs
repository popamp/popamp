//! The media library: the tracks found in the folders the user gave it,
//! kept in `library.txt`.
//!
//! The file is lines of tab-separated fields: `D<TAB>folder` for a folder
//! and `T<TAB>...` for a track. It is written whole; a crash midway leaves
//! the old file. Popamp only reads the audio files, it never changes them.

use crate::meta::TrackMeta;
use crate::replaygain::Stamp;
use crate::text::{escape_field as escape, natural_cmp, unescape_field as unescape};
use std::cmp::Ordering;
use std::path::Path;

/// One audio file of the library, with the tags it had when it was read.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Track {
    pub path: String,
    /// The file's size and change time when its tags were read.
    pub stamp: Stamp,
    /// When the library first saw it, in seconds since the Unix epoch.
    pub added: u64,
    pub title: String,
    pub artist: String,
    pub album: String,
    pub genre: String,
    pub year: String,
    /// The track number; 0 when the file has none.
    pub number: u32,
    pub length_ms: Option<u64>,
}

impl Track {
    /// A track from a file's tags. A file without a title shows its name.
    pub fn new(path: String, stamp: Stamp, added: u64, meta: &TrackMeta) -> Track {
        let field = |f: &Option<String>| f.as_deref().unwrap_or("").trim().to_string();
        let mut title = field(&meta.title);
        if title.is_empty() {
            title = Path::new(&path).file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
        }
        // "3/12" is track 3 of 12.
        let number = field(&meta.track).split('/').next().and_then(|n| n.trim().parse().ok()).unwrap_or(0);
        Track {
            stamp,
            added,
            title,
            artist: field(&meta.artist),
            album: field(&meta.album),
            genre: field(&meta.genre),
            year: field(&meta.year),
            number,
            length_ms: meta.length_ms,
            path,
        }
    }

    fn line(&self) -> String {
        let length = self.length_ms.map(|l| l.to_string()).unwrap_or_default();
        let texts = [&self.path, &self.title, &self.artist, &self.album, &self.genre, &self.year].map(|t| escape(t));
        format!("T\t{}\t{}\t{}\t{}\t{length}\t{}\n", self.stamp.size, self.stamp.modified, self.added, self.number, texts.join("\t"))
    }

    fn parse(fields: &[&str]) -> Option<Track> {
        let [size, modified, added, number, length, path, title, artist, album, genre, year] = fields else { return None };
        Some(Track {
            path: Some(unescape(path)).filter(|p| !p.is_empty())?,
            stamp: Stamp { size: size.parse().ok()?, modified: modified.parse().ok()? },
            added: added.parse().ok()?,
            number: number.parse().ok()?,
            length_ms: length.parse().ok(),
            title: unescape(title),
            artist: unescape(artist),
            album: unescape(album),
            genre: unescape(genre),
            year: unescape(year),
        })
    }

    /// The value of the tag `by` groups tracks on.
    fn tag(&self, by: Group) -> &str {
        match by {
            Group::Artist => &self.artist,
            Group::Album => &self.album,
        }
    }

    /// Whether the track has every word of `words` (lower case) in its
    /// tags or its path. The file type is no part of that: `3` would find
    /// every `.mp3`.
    fn has(&self, words: &[String]) -> bool {
        let path = Path::new(&self.path).with_extension("").to_string_lossy().into_owned();
        let hay = [&self.title, &self.artist, &self.album, &self.genre, &self.year, &path].map(|t| t.to_lowercase());
        words.iter().all(|w| hay.iter().any(|h| h.contains(w.as_str())))
    }
}

/// A tag the library lists tracks by.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Group {
    Artist,
    Album,
}

/// One value of a tag among some tracks.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct GroupRow {
    /// Empty for the tracks without the tag.
    pub name: String,
    pub tracks: usize,
    /// How many albums its tracks are from.
    pub albums: usize,
    /// The first year its tracks have.
    pub year: String,
}

/// Which tracks are wanted.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Filter<'a> {
    /// Only the tracks under this folder.
    pub folder: Option<&'a str>,
    /// Only the tracks of this artist and of this album; an empty value
    /// is the tracks without the tag.
    pub artist: Option<&'a str>,
    pub album: Option<&'a str>,
    /// Only the tracks with every word of this text.
    pub query: &'a str,
}

/// The order of a track list.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Sort {
    /// By artist, then album, track number and title.
    #[default]
    Artist,
    Title,
    Album,
    Length,
    Year,
    Genre,
    /// The newest in the library first.
    Added,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Library {
    /// The folders the library is made of, as the user gave them.
    pub folders: Vec<String>,
    pub tracks: Vec<Track>,
}

/// Whether `path` is in `folder`, at any depth.
pub fn is_under(path: &str, folder: &str) -> bool {
    Path::new(path).starts_with(folder)
}

fn by_album(a: &Track, b: &Track) -> Ordering {
    natural_cmp(&a.album, &b.album).then(a.number.cmp(&b.number)).then_with(|| natural_cmp(&a.title, &b.title))
}

impl Library {
    /// The library in `text`; broken lines are skipped.
    pub fn parse(text: &str) -> Library {
        let mut lib = Library::default();
        for line in text.lines() {
            let fields: Vec<&str> = line.split('\t').collect();
            match fields.split_first() {
                Some((&"D", [folder])) if !folder.is_empty() => lib.folders.push(unescape(folder)),
                Some((&"T", rest)) => lib.tracks.extend(Track::parse(rest)),
                _ => {}
            }
        }
        lib
    }

    pub fn to_text(&self) -> String {
        let folders = self.folders.iter().map(|f| format!("D\t{}\n", escape(f)));
        folders.chain(self.tracks.iter().map(Track::line)).collect()
    }

    /// A missing file is an empty library.
    pub fn load(path: &Path) -> Library {
        Library::parse(&std::fs::read_to_string(path).unwrap_or_default())
    }

    /// Writes the whole library; a crash midway leaves the old file.
    pub fn save(&self, path: &Path) -> std::io::Result<()> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let tmp = path.with_extension("txt.tmp");
        std::fs::write(&tmp, self.to_text())?;
        std::fs::rename(&tmp, path)
    }

    /// Adds a folder; false when the library has it already, itself or as
    /// part of a folder it has.
    pub fn add_folder(&mut self, folder: &str) -> bool {
        if folder.is_empty() || self.folders.iter().any(|f| is_under(folder, f)) {
            return false;
        }
        // Folders inside the new one are part of it from now on.
        self.folders.retain(|f| !is_under(f, folder));
        self.folders.push(folder.to_string());
        true
    }

    /// Takes a folder out, with its tracks.
    pub fn remove_folder(&mut self, folder: &str) {
        self.folders.retain(|f| f != folder);
        self.drop_strays();
    }

    /// Forgets the tracks that are in none of the folders.
    fn drop_strays(&mut self) {
        let folders = &self.folders;
        self.tracks.retain(|t| folders.iter().any(|f| is_under(&t.path, f)));
    }

    /// Takes what a scan of `folder` found in place of what the library
    /// had there.
    pub fn replace_folder(&mut self, folder: &str, found: Vec<Track>) {
        self.tracks.retain(|t| !is_under(&t.path, folder));
        self.tracks.extend(found);
        self.drop_strays();
    }

    /// The tracks `filter` lets through, as places in `tracks`, in `sort`
    /// order (`reverse` turns it around).
    pub fn find(&self, filter: &Filter, sort: Sort, reverse: bool) -> Vec<usize> {
        let words: Vec<String> = filter.query.split_whitespace().map(str::to_lowercase).collect();
        let passes = |t: &Track| {
            filter.folder.is_none_or(|f| is_under(&t.path, f))
                && filter.artist.is_none_or(|a| t.artist == a)
                && filter.album.is_none_or(|a| t.album == a)
                && (words.is_empty() || t.has(&words))
        };
        let mut found: Vec<usize> = (0..self.tracks.len()).filter(|&i| passes(&self.tracks[i])).collect();
        let order = |a: &Track, b: &Track| match sort {
            Sort::Artist => natural_cmp(&a.artist, &b.artist).then_with(|| by_album(a, b)),
            Sort::Title => natural_cmp(&a.title, &b.title),
            Sort::Album => by_album(a, b),
            Sort::Length => a.length_ms.cmp(&b.length_ms),
            Sort::Year => natural_cmp(&a.year, &b.year).then_with(|| by_album(a, b)),
            Sort::Genre => natural_cmp(&a.genre, &b.genre).then_with(|| by_album(a, b)),
            Sort::Added => b.added.cmp(&a.added).then_with(|| natural_cmp(&a.path, &b.path)),
        };
        found.sort_by(|&a, &b| order(&self.tracks[a], &self.tracks[b]).then(a.cmp(&b)));
        if reverse {
            found.reverse();
        }
        found
    }

    /// The values the tag `by` has among the tracks `filter` lets through,
    /// in order. Tracks without the tag count under an empty value, which
    /// comes first.
    pub fn groups(&self, by: Group, filter: &Filter) -> Vec<GroupRow> {
        let mut found: Vec<&Track> = self.find(filter, Sort::Album, false).into_iter().map(|i| &self.tracks[i]).collect();
        found.sort_by(|a, b| natural_cmp(a.tag(by), b.tag(by)).then(a.tag(by).cmp(b.tag(by))));
        let mut out: Vec<GroupRow> = Vec::new();
        let mut last_album: Option<&str> = None;
        for t in found {
            if out.last().is_none_or(|g| g.name != t.tag(by)) {
                out.push(GroupRow { name: t.tag(by).to_string(), ..GroupRow::default() });
                last_album = None;
            }
            let g = out.last_mut().expect("pushed above");
            g.tracks += 1;
            // Within one value the tracks still come album by album.
            if last_album != Some(&t.album) {
                g.albums += 1;
                last_album = Some(&t.album);
            }
            if g.year.is_empty() {
                g.year.clone_from(&t.year);
            }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn track(path: &str, artist: &str, album: &str, number: u32, title: &str) -> Track {
        Track {
            path: path.into(),
            artist: artist.into(),
            album: album.into(),
            number,
            title: title.into(),
            added: number as u64,
            ..Track::default()
        }
    }

    fn sample() -> Library {
        Library {
            folders: vec!["/music".into()],
            tracks: vec![
                track("/music/b/2.mp3", "Beta", "Second", 2, "Two"),
                track("/music/a/10.mp3", "Alpha", "First", 10, "Ten"),
                track("/music/a/9.mp3", "Alpha", "First", 9, "Nine"),
                track("/music/b/1.mp3", "Beta", "Second", 1, "One"),
                track("/music/x.mp3", "", "", 0, "Loose"),
            ],
        }
    }

    fn titles(lib: &Library, found: &[usize]) -> Vec<String> {
        found.iter().map(|&i| lib.tracks[i].title.clone()).collect()
    }

    #[test]
    fn a_saved_library_reads_back_the_same() {
        let mut lib = sample();
        lib.tracks[0].title = "Tab\there\nand a line".into();
        lib.tracks[0].length_ms = Some(123_456);
        lib.tracks[0].stamp = Stamp { size: 7, modified: 9 };
        lib.tracks[0].genre = "Rock".into();
        lib.tracks[0].year = "1996".into();
        assert_eq!(Library::parse(&lib.to_text()), lib);
        assert_eq!(Library::parse("nonsense\nT\t1\t2\nD\t\n"), Library::default(), "broken lines are skipped");
    }

    #[test]
    fn tags_make_a_track_and_the_file_name_stands_in_for_a_title() {
        let meta = TrackMeta { artist: Some(" Kangaroo Moon ".into()), track: Some("3/12".into()), ..TrackMeta::default() };
        let t = Track::new("/music/Keep Them Warm.flac".into(), Stamp::default(), 5, &meta);
        assert_eq!((t.title.as_str(), t.artist.as_str(), t.number, t.added), ("Keep Them Warm", "Kangaroo Moon", 3, 5));
    }

    #[test]
    fn tracks_come_by_artist_album_and_number() {
        let lib = sample();
        let all = lib.find(&Filter::default(), Sort::Artist, false);
        assert_eq!(titles(&lib, &all), ["Loose", "Nine", "Ten", "One", "Two"]);
        assert_eq!(titles(&lib, &lib.find(&Filter::default(), Sort::Title, true)), ["Two", "Ten", "One", "Nine", "Loose"]);
        assert_eq!(titles(&lib, &lib.find(&Filter::default(), Sort::Added, false))[0], "Ten", "the newest first");
    }

    #[test]
    fn a_search_needs_every_word_in_any_case() {
        let lib = sample();
        let find = |query| titles(&lib, &lib.find(&Filter { query, ..Filter::default() }, Sort::Artist, false));
        assert_eq!(find("alpha NINE"), ["Nine"]);
        assert_eq!(find("second"), ["One", "Two"]);
        assert_eq!(find("/a/"), ["Nine", "Ten"], "the path counts too");
        assert!(find("mp3").is_empty(), "but not the file type");
        assert!(find("alpha one").is_empty());
    }

    #[test]
    fn groups_count_their_tracks_and_filter_them() {
        let mut lib = sample();
        lib.tracks[0].year = "1999".into();
        let artists = lib.groups(Group::Artist, &Filter::default());
        let rows: Vec<(&str, usize, usize)> = artists.iter().map(|g| (g.name.as_str(), g.albums, g.tracks)).collect();
        assert_eq!(rows, [("", 1, 1), ("Alpha", 1, 2), ("Beta", 1, 2)]);
        let of = |artist, album| titles(&lib, &lib.find(&Filter { artist, album, ..Filter::default() }, Sort::Artist, false));
        assert_eq!(of(Some("Beta"), None), ["One", "Two"]);
        assert_eq!(of(Some(""), None), ["Loose"]);
        assert_eq!(of(Some("Alpha"), Some("Second")), Vec::<String>::new(), "artist and album both hold");
        let albums = lib.groups(Group::Album, &Filter { artist: Some("Beta"), ..Filter::default() });
        assert_eq!(albums, [GroupRow { name: "Second".into(), tracks: 2, albums: 1, year: "1999".into() }]);
        assert_eq!(lib.groups(Group::Album, &Filter { query: "ten", ..Filter::default() })[0].name, "First");
    }

    #[test]
    fn tracks_sort_by_year_and_genre_too() {
        let mut lib = sample();
        (lib.tracks[3].year, lib.tracks[1].genre) = ("1980".into(), "Ambient".into());
        assert_eq!(titles(&lib, &lib.find(&Filter::default(), Sort::Year, true))[0], "One");
        assert_eq!(titles(&lib, &lib.find(&Filter::default(), Sort::Genre, true))[0], "Ten");
    }

    #[test]
    fn folders_hold_their_tracks() {
        let mut lib = sample();
        assert!(!lib.add_folder("/music/a"), "already inside /music");
        assert!(!lib.add_folder("/music"));
        assert!(lib.add_folder("/more"));
        assert!(!is_under("/music2/x.mp3", "/music"), "a folder is not a prefix of a name");
        let in_a = Filter { folder: Some("/music/a"), ..Filter::default() };
        assert_eq!(lib.find(&in_a, Sort::Artist, false).len(), 2);
        lib.replace_folder("/more", vec![track("/more/n.mp3", "New", "", 0, "N")]);
        assert_eq!(lib.tracks.len(), 6);
        lib.replace_folder("/more", vec![]);
        assert_eq!(lib.tracks.len(), 5, "a rescan drops what is gone");
        lib.remove_folder("/music");
        assert_eq!((lib.folders.as_slice(), lib.tracks.len()), (["/more".to_string()].as_slice(), 0));
        assert!(lib.add_folder("/"), "a parent takes the folders inside it");
        assert_eq!(lib.folders, ["/"]);
    }
}
