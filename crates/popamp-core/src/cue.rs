//! CUE sheets: one audio file (or a few) cut into tracks.
//!
//! A track of a sheet is played from the location `<sheet>#<number>`, so
//! a playlist keeps it like any other file. A sheet may also be kept
//! inside the album file; its tracks are then `<file>#<number>`.

mod embedded;

use crate::text::decode_lossy;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Sheet {
    pub title: Option<String>,
    pub performer: Option<String>,
    pub tracks: Vec<Track>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Track {
    pub number: u32,
    pub file: PathBuf,
    pub title: Option<String>,
    pub performer: Option<String>,
    pub start_ms: u64,
    /// Where the next track of the same file starts; `None` for the last.
    pub end_ms: Option<u64>,
}

impl Sheet {
    pub fn track(&self, number: u32) -> Option<&Track> {
        self.tracks.iter().find(|t| t.number == number)
    }
}

pub fn is_cue(location: &str) -> bool {
    Path::new(location).extension().is_some_and(|e| e.eq_ignore_ascii_case("cue"))
}

pub fn track_location(sheet: &str, number: u32) -> String {
    format!("{sheet}#{number}")
}

/// The sheet and track number of a location made by [`track_location`].
pub fn split_location(location: &str) -> Option<(&str, u32)> {
    let (sheet, number) = location.rsplit_once('#')?;
    if !(is_cue(sheet) || embedded::may_carry(sheet)) || number.is_empty() || !number.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    Some((sheet, number.parse().ok()?))
}

/// Reads a sheet; `extra` are the formats plug-ins add, which a missing
/// file named by the sheet may have been converted to.
/// A path that is not a `.cue` file is an album file, and its sheet is
/// the one inside it: every track of that plays from the file itself.
pub fn read(path: &Path, extra: &[String]) -> std::io::Result<Sheet> {
    if !is_cue(&path.to_string_lossy()) {
        let text = embedded::sheet_text(path).ok_or_else(|| std::io::Error::other("no CUE sheet in the file"))?;
        return Ok(parse_with(&text, |_| path.to_path_buf()));
    }
    let bytes = std::fs::read(path)?;
    Ok(parse(&decode_lossy(&bytes), path.parent().unwrap_or(Path::new("")), extra))
}

/// True for a file that may keep its sheet inside.
pub fn may_carry_sheet(location: &str) -> bool {
    embedded::may_carry(location)
}

/// The value after a command: quoted, or the first word.
fn value(rest: &str) -> String {
    let rest = rest.trim();
    match rest.strip_prefix('"') {
        Some(q) => q.split('"').next().unwrap_or_default().to_string(),
        None => rest.split_whitespace().next().unwrap_or_default().to_string(),
    }
}

/// `mm:ss:ff`, with 75 frames a second.
fn index_ms(time: &str) -> Option<u64> {
    let mut parts = time.split(':').map(|p| p.trim().parse::<u64>().ok());
    let (m, s, f) = (parts.next()??, parts.next()??, parts.next()??);
    Some((m * 60 + s) * 1000 + f * 1000 / 75)
}

/// The file a sheet names; when it is not there, one of the same name in
/// another format (sheets often outlive a conversion from WAV).
fn find_file(base: &Path, name: &str, extra: &[String]) -> PathBuf {
    let path = base.join(name.replace('\\', "/"));
    if path.exists() {
        return path;
    }
    let formats = crate::COMMON_EXTENSIONS.iter().copied().chain(extra.iter().map(String::as_str));
    formats.map(|e| path.with_extension(e)).find(|p| p.exists()).unwrap_or(path)
}

pub fn parse(text: &str, base: &Path, extra: &[String]) -> Sheet {
    parse_with(text, |name| find_file(base, name, extra))
}

/// Parses a sheet; `file_of` finds the file a `FILE` line names.
fn parse_with(text: &str, file_of: impl Fn(&str) -> PathBuf) -> Sheet {
    let mut sheet = Sheet::default();
    let mut file: Option<PathBuf> = None;
    // The track being read: its number, and whether it is audio.
    let mut current: Option<Track> = None;
    let mut done: Vec<Track> = Vec::new();
    for line in text.lines() {
        let line = line.trim().trim_start_matches('\u{feff}');
        let (command, rest) = line.split_once(char::is_whitespace).unwrap_or((line, ""));
        match command.to_ascii_uppercase().as_str() {
            "FILE" => file = Some(file_of(&value(rest))),
            "TRACK" => {
                done.extend(current.take().filter(|t| t.start_ms != u64::MAX));
                let mut words = rest.split_whitespace();
                let number = words.next().and_then(|n| n.parse().ok());
                let audio = words.next().is_some_and(|k| k.eq_ignore_ascii_case("AUDIO"));
                if let (Some(number), Some(file), true) = (number, &file, audio) {
                    let file = file.clone();
                    current = Some(Track { number, file, title: None, performer: None, start_ms: u64::MAX, end_ms: None });
                }
            }
            "TITLE" | "PERFORMER" => {
                let v = Some(value(rest)).filter(|v| !v.is_empty());
                let is_title = command.eq_ignore_ascii_case("TITLE");
                match (&mut current, is_title) {
                    (Some(t), true) => t.title = v,
                    (Some(t), false) => t.performer = v,
                    (None, true) => sheet.title = v,
                    (None, false) => sheet.performer = v,
                }
            }
            "INDEX" => {
                let mut words = rest.split_whitespace();
                // The track plays from the file its start is in, which may
                // follow its TRACK line (a pregap kept in the file before).
                if let (Some(t), Some("01"), Some(ms), Some(file)) = (&mut current, words.next(), words.next().and_then(index_ms), &file) {
                    t.start_ms = ms;
                    t.file = file.clone();
                }
            }
            _ => {}
        }
    }
    done.extend(current.filter(|t| t.start_ms != u64::MAX));
    // A track ends where the next one in its file starts.
    for i in 0..done.len() {
        if let Some(next) = done.get(i + 1).filter(|n| n.file == done[i].file) {
            done[i].end_ms = Some(next.start_ms);
        }
    }
    sheet.tracks = done;
    sheet
}

#[cfg(test)]
mod tests {
    use super::*;

    const SHEET: &str = "\u{feff}REM GENRE Rock
PERFORMER \"The Band\"
TITLE \"Live\"
FILE \"live.wav\" WAVE
  TRACK 01 AUDIO
    TITLE \"Intro\"
    INDEX 01 00:00:00
  TRACK 02 AUDIO
    TITLE \"Song\"
    PERFORMER \"Guest\"
    INDEX 00 03:58:00
    INDEX 01 04:00:37
  TRACK 03 MODE1/2352
    INDEX 01 08:00:00
FILE \"bonus.flac\" WAVE
  TRACK 04 AUDIO
    TITLE Bonus
    INDEX 01 00:00:00
";

    #[test]
    fn a_sheet_cuts_its_files_into_tracks() {
        let sheet = parse(SHEET, Path::new("/music"), &[]);
        assert_eq!((sheet.title.as_deref(), sheet.performer.as_deref()), (Some("Live"), Some("The Band")));
        let t = &sheet.tracks;
        assert_eq!(t.iter().map(|t| t.number).collect::<Vec<_>>(), [1, 2, 4], "the data track is left out");
        assert_eq!((t[0].title.as_deref(), t[0].start_ms, t[0].end_ms), (Some("Intro"), 0, Some(240_493)));
        assert_eq!((t[1].performer.as_deref(), t[1].start_ms, t[1].end_ms), (Some("Guest"), 240_493, None));
        assert_eq!(t[1].file, Path::new("/music/live.wav"));
        assert_eq!((t[2].title.as_deref(), t[2].file.as_path(), t[2].end_ms), (Some("Bonus"), Path::new("/music/bonus.flac"), None));
    }

    /// EAC's "gaps appended to previous": a file per track, and a track's
    /// pregap (INDEX 00) at the end of the file before its own.
    #[test]
    fn a_track_plays_from_the_file_its_start_is_in() {
        let sheet = "FILE \"01.wav\" WAVE
  TRACK 01 AUDIO
    INDEX 01 00:00:00
  TRACK 02 AUDIO
    INDEX 00 05:16:05
FILE \"02.wav\" WAVE
    INDEX 01 00:00:00
  TRACK 03 AUDIO
    INDEX 00 03:59:22
FILE \"03.wav\" WAVE
    INDEX 01 00:00:00
";
        let t = parse(sheet, Path::new("/m"), &[]).tracks;
        let got: Vec<_> = t.iter().map(|t| (t.number, t.file.to_string_lossy().into_owned(), t.start_ms, t.end_ms)).collect();
        let want = [(1, "/m/01.wav", 0, None), (2, "/m/02.wav", 0, None), (3, "/m/03.wav", 0, None)];
        assert_eq!(got, want.map(|(n, f, s, e)| (n, f.to_string(), s, e)));
    }

    #[test]
    fn a_named_file_that_was_converted_is_found_by_its_name() {
        let dir = std::env::temp_dir().join(format!("popamp-cue-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("live.flac"), b"").unwrap();
        assert_eq!(parse(SHEET, &dir, &[]).tracks[0].file, dir.join("live.flac"));
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn a_named_file_in_a_format_of_a_plugin_is_found_when_it_is_on() {
        let dir = std::env::temp_dir().join(format!("popamp-cue-ape-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("live.ape"), b"").unwrap();
        assert_eq!(parse(SHEET, &dir, &[]).tracks[0].file, dir.join("live.wav"), "no decoder, no file");
        assert_eq!(parse(SHEET, &dir, &["ape".to_string()]).tracks[0].file, dir.join("live.ape"));
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn a_track_location_names_the_sheet_and_the_number() {
        let loc = track_location("/music/Live.CUE", 2);
        assert_eq!(split_location(&loc), Some(("/music/Live.CUE", 2)));
        assert_eq!(split_location("/music/a.flac#2"), Some(("/music/a.flac", 2)), "a sheet inside the file");
        assert_eq!(split_location("/music/a.mp3#2"), None);
        assert_eq!(split_location("/music/a.cue"), None);
        assert_eq!(split_location("/music/a.cue#x"), None);
        assert!(is_cue("/music/a.cue") && !is_cue("/music/a.flac"));
    }

    /// A FLAC file's metadata blocks, as (type, body); no audio follows.
    fn flac(blocks: &[(u8, Vec<u8>)]) -> Vec<u8> {
        let mut out = b"fLaC".to_vec();
        for (i, (kind, body)) in blocks.iter().enumerate() {
            let last = if i + 1 == blocks.len() { 0x80 } else { 0 };
            out.push(last | kind);
            out.extend_from_slice(&(body.len() as u32).to_be_bytes()[1..]);
            out.extend_from_slice(body);
        }
        out
    }

    fn stream_info(rate: u32) -> (u8, Vec<u8>) {
        let mut b = vec![0u8; 34];
        b[10] = (rate >> 12) as u8;
        b[11] = (rate >> 4) as u8;
        b[12] = ((rate & 0xF) << 4) as u8 | 0x02;
        (0, b)
    }

    fn comments(pairs: &[&str]) -> (u8, Vec<u8>) {
        let mut b = 4u32.to_le_bytes().to_vec();
        b.extend_from_slice(b"test");
        b.extend_from_slice(&(pairs.len() as u32).to_le_bytes());
        for p in pairs {
            b.extend_from_slice(&(p.len() as u32).to_le_bytes());
            b.extend_from_slice(p.as_bytes());
        }
        (4, b)
    }

    /// A track of a CUESHEET block: number, offset, its index points as (number, offset).
    type BlockTrack<'a> = (u8, u64, &'a [(u8, u64)]);

    /// A CD's CUESHEET block.
    fn cuesheet_block(tracks: &[BlockTrack]) -> (u8, Vec<u8>) {
        let mut b = vec![0u8; 128 + 8];
        b.push(0x80);
        b.extend_from_slice(&[0; 258]);
        b.push(tracks.len() as u8);
        for (number, offset, points) in tracks {
            b.extend_from_slice(&offset.to_be_bytes());
            b.push(*number);
            b.extend_from_slice(&[0; 12 + 14]);
            b.push(points.len() as u8);
            for (n, o) in *points {
                b.extend_from_slice(&o.to_be_bytes());
                b.push(*n);
                b.extend_from_slice(&[0; 3]);
            }
        }
        (5, b)
    }

    fn temp_file(name: &str, bytes: &[u8]) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("popamp-cue-in-{}-{name}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join(name);
        std::fs::write(&path, bytes).unwrap();
        path
    }

    #[test]
    fn a_flac_file_may_carry_its_sheet_in_a_tag() {
        let tag = "CUESHEET=PERFORMER \"Band\"\nFILE \"Range.wav\" WAVE\n  TRACK 01 AUDIO\n    TITLE \"Тучи\"\n    INDEX 01 00:00:00\n  TRACK 02 AUDIO\n    INDEX 01 00:04:00\n";
        let padding = (1, vec![0u8; 5000]);
        let path = temp_file("album.flac", &flac(&[stream_info(44_100), padding, comments(&["TITLE=Whole", tag])]));
        let sheet = read(&path, &[]).unwrap();
        assert_eq!(sheet.performer.as_deref(), Some("Band"));
        let t = &sheet.tracks;
        assert_eq!((t[0].title.as_deref(), t[0].file.as_path(), t[0].end_ms), (Some("Тучи"), path.as_path(), Some(4000)));
        assert_eq!((t[1].number, t[1].file.as_path(), t[1].start_ms), (2, path.as_path(), 4000));
        std::fs::remove_dir_all(path.parent().unwrap()).unwrap();
    }

    #[test]
    fn a_flac_file_may_carry_its_sheet_as_a_cuesheet_block() {
        // Track 2 has a pregap (index 0) of a second before it starts at 5 s; 170 is the lead-out.
        let blocks = [stream_info(44_100), cuesheet_block(&[(1, 0, &[(1, 0)]), (2, 176_400, &[(0, 0), (1, 44_100)]), (170, 441_000, &[])])];
        let path = temp_file("disc.flac", &flac(&blocks));
        let t = read(&path, &[]).unwrap().tracks;
        assert_eq!(t.iter().map(|t| (t.number, t.start_ms, t.end_ms)).collect::<Vec<_>>(), [(1, 0, Some(5000)), (2, 5000, None)]);
        assert_eq!(t[0].file, path);
        std::fs::remove_dir_all(path.parent().unwrap()).unwrap();
    }

    #[test]
    fn an_ape_file_may_carry_its_sheet_in_its_ape_tag() {
        let sheet = "FILE \"x.wav\" WAVE\n  TRACK 01 AUDIO\n    INDEX 01 00:00:00\n  TRACK 02 AUDIO\n    INDEX 01 01:00:00\n";
        let mut items = Vec::new();
        for (key, value) in [("Artist", "Band"), ("Cuesheet", sheet)] {
            items.extend_from_slice(&(value.len() as u32).to_le_bytes());
            items.extend_from_slice(&0u32.to_le_bytes());
            items.extend_from_slice(key.as_bytes());
            items.push(0);
            items.extend_from_slice(value.as_bytes());
        }
        let mut file = b"MAC \x96\x0f audio".to_vec();
        file.extend_from_slice(&items);
        file.extend_from_slice(b"APETAGEX");
        for n in [2000, items.len() as u32 + 32, 2, 0] {
            file.extend_from_slice(&n.to_le_bytes());
        }
        file.extend_from_slice(&[0; 8]);
        file.extend_from_slice(b"TAG");
        file.extend_from_slice(&[0; 125]);
        let path = temp_file("album.ape", &file);
        let t = read(&path, &[]).unwrap().tracks;
        assert_eq!(t.iter().map(|t| (t.number, t.start_ms)).collect::<Vec<_>>(), [(1, 0), (2, 60_000)]);
        assert_eq!(t[1].file, path);
        std::fs::remove_dir_all(path.parent().unwrap()).unwrap();
    }

    #[test]
    fn a_file_without_a_sheet_has_none() {
        let path = temp_file("plain.flac", &flac(&[stream_info(44_100), comments(&["TITLE=Song"])]));
        assert!(read(&path, &[]).is_err());
        std::fs::remove_dir_all(path.parent().unwrap()).unwrap();
    }
}
