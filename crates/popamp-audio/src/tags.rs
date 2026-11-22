//! Changing a track's tags and cover in the file, through lofty.
//!
//! The changes go into a copy beside the file, which then takes the
//! file's place: a crash or a full disk leaves the old file whole, and a
//! decoder still reading the old file (on Linux) keeps reading it.

use crate::cover::Picture;
use lofty::config::{ParseOptions, WriteOptions};
use lofty::file::{AudioFile, TaggedFileExt};
use lofty::flac::FlacFile;
use lofty::mp4::Mp4File;
use lofty::mpeg::MpegFile;
use lofty::ogg::{OggPictureStorage, OpusFile, VorbisFile};
use lofty::picture::{MimeType, PictureInformation, PictureType};
use lofty::tag::items::Timestamp;
use lofty::tag::{ItemKey, MergeTag, SplitTag, Tag, TagType};
use popamp_core::meta::TrackMeta;
use std::path::{Path, PathBuf};

/// A tag the File info window edits.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Field {
    Title,
    Artist,
    Album,
    AlbumArtist,
    Year,
    Genre,
    /// The track number, `3` or `3/12`.
    Track,
    /// The disc number, `1` or `1/2`.
    Disc,
    Composer,
    Comment,
}

impl Field {
    pub const ALL: [Field; 10] = [
        Field::Title,
        Field::Artist,
        Field::Album,
        Field::AlbumArtist,
        Field::Year,
        Field::Genre,
        Field::Track,
        Field::Disc,
        Field::Composer,
        Field::Comment,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Field::Title => "Title",
            Field::Artist => "Artist",
            Field::Album => "Album",
            Field::AlbumArtist => "Album artist",
            Field::Year => "Year",
            Field::Genre => "Genre",
            Field::Track => "Track",
            Field::Disc => "Disc",
            Field::Composer => "Composer",
            Field::Comment => "Comment",
        }
    }

    /// The value as read into `meta`.
    pub fn get(self, meta: &TrackMeta) -> Option<&str> {
        match self {
            Field::Title => meta.title.as_deref(),
            Field::Artist => meta.artist.as_deref(),
            Field::Album => meta.album.as_deref(),
            Field::AlbumArtist => meta.album_artist.as_deref(),
            Field::Year => meta.year.as_deref(),
            Field::Genre => meta.genre.as_deref(),
            Field::Track => meta.track.as_deref(),
            Field::Disc => meta.disc.as_deref(),
            Field::Composer => meta.composer.as_deref(),
            Field::Comment => meta.comment.as_deref(),
        }
    }

    /// The keys the value goes under: the number and the total for the
    /// track and the disc.
    fn keys(self) -> (ItemKey, Option<ItemKey>) {
        match self {
            Field::Title => (ItemKey::TrackTitle, None),
            Field::Artist => (ItemKey::TrackArtist, None),
            Field::Album => (ItemKey::AlbumTitle, None),
            Field::AlbumArtist => (ItemKey::AlbumArtist, None),
            Field::Year => (ItemKey::RecordingDate, Some(ItemKey::Year)),
            Field::Genre => (ItemKey::Genre, None),
            Field::Track => (ItemKey::TrackNumber, Some(ItemKey::TrackTotal)),
            Field::Disc => (ItemKey::DiscNumber, Some(ItemKey::DiscTotal)),
            Field::Composer => (ItemKey::Composer, None),
            Field::Comment => (ItemKey::Comment, None),
        }
    }
}

/// What happens to the cover.
#[derive(Debug, Clone, PartialEq)]
pub enum CoverChange {
    Keep,
    /// A new front cover in place of the old one.
    Set(Picture),
    /// No pictures at all, so none shows.
    Remove,
}

/// The new values of some fields (an empty one removes the tag) and the
/// cover.
#[derive(Debug, Clone, PartialEq)]
pub struct Changes {
    pub fields: Vec<(Field, String)>,
    pub cover: CoverChange,
}

/// Whether Popamp can change the tags of the file at `path`: only where
/// lofty writes them before the sound or in the container, where Symphonia
/// reads them. In WAV and AIFF they would go after the sound, unseen, and
/// Symphonia 0.5 misjudges the length of a bare AAC file with a tag.
pub fn editable(path: &Path) -> bool {
    lofty::read_from_path(path).is_ok_and(|f| writes(f.file_type()))
}

/// Whether the name of the file at `path` says it is of a format Popamp
/// can change the tags of, whatever is in it.
pub fn named_editable(path: &Path) -> bool {
    lofty::file::FileType::from_path(path).is_some_and(writes)
}

fn writes(kind: lofty::file::FileType) -> bool {
    use lofty::file::FileType as T;
    matches!(kind, T::Mpeg | T::Flac | T::Vorbis | T::Opus | T::Mp4)
}

/// The fields as lofty reads them from the file at `path`, which is what
/// [`write`] changes: a track or disc as `3/12` when the total is there.
pub fn read(path: &Path) -> Result<Vec<(Field, String)>, String> {
    let file = lofty::read_from_path(path).map_err(|e| e.to_string())?;
    let Some(tag) = file.primary_tag().or(file.first_tag()) else { return Ok(Field::ALL.iter().map(|&f| (f, String::new())).collect()) };
    let value = |f: Field| {
        let (key, second) = f.keys();
        let main = tag.get_string(key).unwrap_or_default();
        match (f, second.and_then(|k| tag.get_string(k))) {
            (Field::Year, Some(year)) if main.is_empty() => year.to_string(),
            (Field::Track | Field::Disc, Some(total)) if !main.is_empty() => format!("{main}/{total}"),
            _ => main.to_string(),
        }
    };
    Ok(Field::ALL.iter().map(|&f| (f, value(f))).collect())
}

/// The picture in `data` with its type, if it is a JPEG, PNG, GIF or BMP.
pub fn picture(data: Vec<u8>) -> Option<Picture> {
    let media_type = match data.get(..4)? {
        [0xFF, 0xD8, 0xFF, _] => "image/jpeg",
        [0x89, b'P', b'N', b'G'] => "image/png",
        [b'G', b'I', b'F', b'8'] => "image/gif",
        [b'B', b'M', _, _] => "image/bmp",
        _ => return None,
    };
    Some(Picture { media_type: media_type.into(), data: data.into() })
}

impl Changes {
    /// Why the values cannot be written, if they cannot: a year must be a
    /// date, and a track or disc a number or two.
    pub fn check(&self) -> Result<(), String> {
        for (field, value) in &self.fields {
            let (main, rest) = split(*field, value.trim());
            let ok = match field {
                Field::Year => main.is_empty() || main.parse::<Timestamp>().is_ok(),
                Field::Track | Field::Disc => [main, rest].iter().all(|v| v.chars().all(|c| c.is_ascii_digit())),
                _ => true,
            };
            if !ok {
                let want = if *field == Field::Year { "a year or a date" } else { "a number, or two as 3/12" };
                return Err(format!("{} must be {want}", field.label()));
            }
        }
        Ok(())
    }
}

/// Writes `changes` into the file at `path`.
pub fn write(path: &Path, changes: &Changes) -> Result<(), String> {
    changes.check()?;
    let copy = sibling(path).ok_or("not a file")?;
    let result = write_copy(path, &copy, changes).and_then(|()| std::fs::rename(&copy, path).map_err(|e| e.to_string()));
    if result.is_err() {
        let _ = std::fs::remove_file(&copy);
    }
    result
}

/// A hidden name beside `path` for the copy being changed, with the same
/// extension, which tells lofty the file type.
fn sibling(path: &Path) -> Option<PathBuf> {
    let stem = path.file_stem()?.to_string_lossy();
    let ext = path.extension().map(|e| format!(".{}", e.to_string_lossy())).unwrap_or_default();
    Some(path.with_file_name(format!(".{stem}.popamp-tags{ext}")))
}

fn write_copy(path: &Path, copy: &Path, changes: &Changes) -> Result<(), String> {
    use lofty::file::FileType as T;
    std::fs::copy(path, copy).map_err(|e| e.to_string())?;
    let probe = lofty::probe::Probe::open(copy).and_then(|p| Ok(p.guess_file_type()?)).map_err(|e| e.to_string())?;
    let mut io = std::fs::OpenOptions::new().read(true).write(true).open(copy).map_err(|e| e.to_string())?;
    match probe.file_type() {
        Some(T::Mpeg) => write_mpeg(&mut io, changes),
        Some(T::Flac) => write_flac(&mut io, changes),
        Some(T::Vorbis) => write_vorbis(&mut io, changes),
        Some(T::Opus) => write_opus(&mut io, changes),
        Some(T::Mp4) => write_mp4(&mut io, changes),
        _ => Err("Popamp cannot change the tags of this file".into()),
    }
}

fn write_mpeg(io: &mut std::fs::File, changes: &Changes) -> Result<(), String> {
    let mut file = MpegFile::read_from(io, ParseOptions::new()).map_err(|e| e.to_string())?;
    let id3v2 = file.remove_id3v2().unwrap_or_default();
    file.set_id3v2(edited(id3v2, changes));
    // The older tags too, so they do not show the old values.
    if let Some(t) = file.remove_id3v1() {
        file.set_id3v1(edited(t, changes));
    }
    if let Some(t) = file.remove_ape() {
        file.set_ape(edited(t, changes));
    }
    save(&file, io)
}

fn write_flac(io: &mut std::fs::File, changes: &Changes) -> Result<(), String> {
    let mut file = FlacFile::read_from(io, ParseOptions::new()).map_err(|e| e.to_string())?;
    // A FLAC file keeps its pictures in blocks of their own.
    let fields = Changes { fields: changes.fields.clone(), cover: CoverChange::Keep };
    let comments = file.remove_vorbis_comments().unwrap_or_default();
    file.set_vorbis_comments(edited(comments, &fields));
    if let Some(t) = file.remove_id3v2() {
        file.set_id3v2(edited(t, &fields));
    }
    change_blocks(&mut file, &changes.cover)?;
    save(&file, io)
}

fn write_vorbis(io: &mut std::fs::File, changes: &Changes) -> Result<(), String> {
    let mut file = VorbisFile::read_from(io, ParseOptions::new()).map_err(|e| e.to_string())?;
    let comments = std::mem::take(file.vorbis_comments_mut());
    *file.vorbis_comments_mut() = edited(comments, changes);
    save(&file, io)
}

fn write_opus(io: &mut std::fs::File, changes: &Changes) -> Result<(), String> {
    let mut file = OpusFile::read_from(io, ParseOptions::new()).map_err(|e| e.to_string())?;
    let comments = std::mem::take(file.vorbis_comments_mut());
    *file.vorbis_comments_mut() = edited(comments, changes);
    save(&file, io)
}

fn write_mp4(io: &mut std::fs::File, changes: &Changes) -> Result<(), String> {
    let mut file = Mp4File::read_from(io, ParseOptions::new()).map_err(|e| e.to_string())?;
    let ilst = file.remove_ilst().unwrap_or_default();
    file.set_ilst(edited(ilst, changes));
    save(&file, io)
}

/// `tag` with `changes` made to what lofty can express in any tag; the
/// items only this kind of tag knows stay as they were.
fn edited<T>(tag: T, changes: &Changes) -> T
where
    T: SplitTag,
    T::Remainder: MergeTag<Merged = T>,
{
    let (rest, mut generic) = tag.split_tag();
    apply(&mut generic, changes);
    rest.merge_tag(generic)
}

/// Changes the picture blocks of a FLAC file.
fn change_blocks(file: &mut FlacFile, cover: &CoverChange) -> Result<(), String> {
    match cover {
        CoverChange::Keep => {}
        CoverChange::Set(p) => {
            file.remove_picture_type(PictureType::CoverFront);
            // The size and colours, or zeros, which mean unknown, for an
            // image lofty does not read.
            let picture = front(p);
            let info = PictureInformation::from_picture(&picture).unwrap_or_default();
            file.insert_picture(picture, Some(info)).map_err(|e| e.to_string())?;
        }
        CoverChange::Remove => drop(file.remove_pictures()),
    }
    Ok(())
}

fn save(file: &impl AudioFile, io: &mut std::fs::File) -> Result<(), String> {
    use std::io::Seek;
    io.rewind().map_err(|e| e.to_string())?;
    file.save_to(io, WriteOptions::default()).map_err(|e| e.to_string())?;
    io.sync_all().map_err(|e| e.to_string())
}

/// `p` as lofty's front cover.
fn front(p: &Picture) -> lofty::picture::Picture {
    lofty::picture::Picture::unchecked(p.data.to_vec())
        .pic_type(PictureType::CoverFront)
        .mime_type(MimeType::from_str(&p.media_type))
        .build()
}

fn apply(tag: &mut Tag, changes: &Changes) {
    for (field, value) in &changes.fields {
        let (key, second) = field.keys();
        let (main, rest) = split(*field, value.trim());
        set(tag, key, main);
        if let Some(second) = second {
            set(tag, second, rest);
        }
    }
    match &changes.cover {
        CoverChange::Keep => {}
        CoverChange::Set(p) => {
            // MP4 pictures have no type: all of them are covers.
            if tag.tag_type() == TagType::Mp4Ilst {
                remove_pictures(tag);
            }
            tag.remove_picture_type(PictureType::CoverFront);
            tag.push_picture(front(p));
        }
        CoverChange::Remove => remove_pictures(tag),
    }
}

fn remove_pictures(tag: &mut Tag) {
    while tag.picture_count() > 0 {
        tag.remove_picture(0);
    }
}

/// The value for the main key and the one for the second: `3/12` is the
/// track 3 of 12. A year goes only under the date: the old year is removed.
fn split(field: Field, value: &str) -> (&str, &str) {
    match field {
        Field::Track | Field::Disc => match value.split_once('/') {
            Some((n, total)) => (n.trim(), total.trim()),
            None => (value, ""),
        },
        _ => (value, ""),
    }
}

fn set(tag: &mut Tag, key: ItemKey, value: &str) {
    tag.remove_key(key);
    if !value.is_empty() {
        tag.insert_text(key, value.to_string());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cover;

    const FORMATS: [&str; 5] = ["mp3", "flac", "ogg", "opus", "m4a"];

    /// A copy of the silent fixture of type `ext` in a directory of its own.
    fn fixture(ext: &str, name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("popamp-tags-{}-{name}-{ext}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        let to = d.join(format!("a.{ext}"));
        std::fs::copy(Path::new(env!("CARGO_MANIFEST_DIR")).join(format!("tests/fixtures/silence.{ext}")), &to).unwrap();
        to
    }

    /// A value for each field, with `value` in the text ones.
    fn all(value: &str) -> Changes {
        let text = |f: Field| match f {
            Field::Year => "1999".to_string(),
            Field::Track => "3".to_string(),
            Field::Disc => "1".to_string(),
            _ => format!("{} {value}", f.label()),
        };
        Changes { fields: Field::ALL.iter().map(|&f| (f, text(f))).collect(), cover: CoverChange::Keep }
    }

    #[test]
    fn every_field_reads_back_in_every_format() {
        for ext in FORMATS {
            let path = fixture(ext, "fields");
            assert!(editable(&path), "{ext}");
            let changes = all("x");
            write(&path, &changes).unwrap_or_else(|e| panic!("{ext}: {e}"));
            let meta = crate::read_meta(&path, &Default::default()).unwrap();
            for (f, v) in &changes.fields {
                assert_eq!(f.get(&meta), Some(v.as_str()), "{ext}: {f:?}");
            }
            assert!(meta.length_ms.is_some(), "{ext}: still plays");
        }
    }

    #[test]
    fn an_empty_field_removes_the_tag() {
        for ext in FORMATS {
            let path = fixture(ext, "empty");
            write(&path, &all("x")).unwrap();
            let changes = Changes { fields: vec![(Field::Artist, String::new()), (Field::Title, "Kept".into())], cover: CoverChange::Keep };
            write(&path, &changes).unwrap();
            let meta = crate::read_meta(&path, &Default::default()).unwrap();
            assert_eq!((meta.artist.as_deref(), meta.title.as_deref()), (None, Some("Kept")), "{ext}");
            assert_eq!(meta.album.as_deref(), Some("Album x"), "{ext}: the rest stays");
        }
    }

    #[test]
    fn a_new_cover_replaces_the_front_one_and_remove_takes_them_all() {
        let png = picture(b"\x89PNG\r\n\x1a\nnot really".to_vec()).unwrap();
        for ext in FORMATS {
            let path = fixture(ext, "cover");
            let jpeg = picture(b"\xFF\xD8\xFF\xE0old".to_vec()).unwrap();
            write(&path, &Changes { fields: vec![], cover: CoverChange::Set(jpeg) }).unwrap();
            write(&path, &Changes { fields: vec![], cover: CoverChange::Set(png.clone()) }).unwrap();
            assert_eq!(cover::tagged(&path), Some(png.clone()), "{ext}");
            write(&path, &Changes { fields: vec![], cover: CoverChange::Remove }).unwrap();
            assert_eq!(cover::tagged(&path), None, "{ext}");
        }
    }

    #[test]
    fn a_failed_write_leaves_the_file_as_it_was() {
        let path = fixture("mp3", "fail");
        std::fs::write(&path, b"not audio at all").unwrap();
        assert!(write(&path, &all("x")).is_err());
        assert_eq!(std::fs::read(&path).unwrap(), b"not audio at all");
        let left: Vec<_> = std::fs::read_dir(path.parent().unwrap()).unwrap().collect();
        assert_eq!(left.len(), 1, "no copy left behind");
        assert!(!editable(&path));
        assert!(!editable(&fixture("wav", "editable")));
        assert!(editable(&fixture("flac", "editable")));
    }

    #[test]
    fn a_reader_of_the_old_file_keeps_its_bytes() {
        use std::io::Read;
        let path = fixture("flac", "reader");
        let before = std::fs::read(&path).unwrap();
        let mut open = std::fs::File::open(&path).unwrap();
        write(&path, &all("x")).unwrap();
        let mut seen = Vec::new();
        open.read_to_end(&mut seen).unwrap();
        // On Linux the open file is the old one; on Windows it may be either,
        // but it is whole.
        if cfg!(unix) {
            assert_eq!(seen, before);
        }
        assert_ne!(std::fs::read(&path).unwrap(), before);
    }

    /// What ffprobe would call the tags the editor does not touch, put in
    /// with lofty's own types.
    #[test]
    fn tags_the_editor_does_not_know_stay() {
        use lofty::id3::v2::{ExtendedTextFrame, Frame, Id3v2Tag};
        use lofty::ogg::tag::VorbisComments;
        let changes = Changes { fields: vec![(Field::Title, "New".into())], cover: CoverChange::Keep };
        // FLAC and Ogg: free-form comments.
        for ext in ["flac", "ogg"] {
            let path = fixture(ext, "keep");
            let mut vc = VorbisComments::default();
            vc.push("custom_thing".into(), "keep me".into());
            vc.push("MUSICBRAINZ_ALBUMID".into(), "abc-123".into());
            let mut io = std::fs::OpenOptions::new().read(true).write(true).open(&path).unwrap();
            if ext == "flac" {
                let mut file = FlacFile::read_from(&mut io, ParseOptions::new()).unwrap();
                file.set_vorbis_comments(vc);
                save(&file, &mut io).unwrap();
            } else {
                let mut file = VorbisFile::read_from(&mut io, ParseOptions::new()).unwrap();
                *file.vorbis_comments_mut() = vc;
                save(&file, &mut io).unwrap();
            }
            write(&path, &changes).unwrap();
            let mut io = std::fs::File::open(&path).unwrap();
            let vc = match ext {
                "flac" => FlacFile::read_from(&mut io, ParseOptions::new()).unwrap().vorbis_comments().unwrap().clone(),
                _ => VorbisFile::read_from(&mut io, ParseOptions::new()).unwrap().vorbis_comments().clone(),
            };
            assert_eq!(vc.get("custom_thing"), Some("keep me"), "{ext}");
            assert_eq!(vc.get("MUSICBRAINZ_ALBUMID"), Some("abc-123"), "{ext}");
            assert_eq!(vc.get("TITLE"), Some("New"), "{ext}");
        }
        // MP3: a TXXX frame of its own.
        let path = fixture("mp3", "keep");
        let mut io = std::fs::OpenOptions::new().read(true).write(true).open(&path).unwrap();
        let mut file = MpegFile::read_from(&mut io, ParseOptions::new()).unwrap();
        let mut id3 = Id3v2Tag::default();
        id3.insert(Frame::UserText(ExtendedTextFrame::new(lofty::TextEncoding::UTF8, "custom thing", "keep me")));
        file.set_id3v2(id3);
        save(&file, &mut io).unwrap();
        write(&path, &changes).unwrap();
        let file = MpegFile::read_from(&mut std::fs::File::open(&path).unwrap(), ParseOptions::new()).unwrap();
        assert_eq!(file.id3v2().unwrap().get_user_text("custom thing"), Some("keep me"));
    }

    #[test]
    fn what_is_written_reads_back_with_the_totals() {
        for ext in FORMATS {
            let path = fixture(ext, "read");
            assert!(read(&path).unwrap().iter().all(|(_, v)| v.is_empty()), "{ext}: nothing yet");
            let mut changes = all("y");
            changes.fields.retain(|(f, _)| !matches!(f, Field::Track | Field::Disc));
            changes.fields.extend([(Field::Track, "4/11".into()), (Field::Disc, "2".into())]);
            write(&path, &changes).unwrap();
            let back = read(&path).unwrap();
            for (f, v) in &changes.fields {
                assert!(back.contains(&(*f, v.clone())), "{ext}: {f:?} {back:?}");
            }
        }
        assert!(read(Path::new("/nonexistent/x.mp3")).is_err());
    }

    #[test]
    fn a_year_must_be_a_date_and_a_track_a_number() {
        let path = fixture("mp3", "check");
        let before = std::fs::read(&path).unwrap();
        for (field, value, why) in [
            (Field::Year, "199x", "Year must be a year or a date"),
            (Field::Track, "three", "Track must be a number, or two as 3/12"),
            (Field::Disc, "1/two", "Disc must be a number, or two as 3/12"),
        ] {
            let changes = Changes { fields: vec![(field, value.into())], cover: CoverChange::Keep };
            assert_eq!(write(&path, &changes), Err(why.to_string()));
        }
        assert_eq!(std::fs::read(&path).unwrap(), before);
        for (field, value) in [(Field::Year, "2001-05-04"), (Field::Year, ""), (Field::Track, " 3 / 12 "), (Field::Disc, "")] {
            assert_eq!(Changes { fields: vec![(field, value.into())], cover: CoverChange::Keep }.check(), Ok(()), "{value}");
        }
    }

    #[test]
    fn pictures_are_known_by_their_first_bytes() {
        assert_eq!(picture(b"\xFF\xD8\xFF\xE1x".to_vec()).unwrap().media_type, "image/jpeg");
        assert_eq!(picture(b"GIF89a".to_vec()).unwrap().media_type, "image/gif");
        assert_eq!(picture(b"BM....".to_vec()).unwrap().media_type, "image/bmp");
        assert_eq!(picture(b"<svg>".to_vec()), None);
        assert_eq!(picture(b"BM".to_vec()), None);
    }

    #[test]
    fn track_and_disc_totals_go_under_keys_of_their_own() {
        assert_eq!(split(Field::Track, "3 / 12"), ("3", "12"));
        assert_eq!(split(Field::Disc, "2"), ("2", ""));
        assert_eq!(split(Field::Title, "a/b"), ("a/b", ""));
        let path = fixture("flac", "totals");
        write(&path, &Changes { fields: vec![(Field::Track, "3/12".into())], cover: CoverChange::Keep }).unwrap();
        let tag = lofty::read_from_path(&path).unwrap();
        let tag = tag.primary_tag().unwrap();
        assert_eq!((tag.get_string(ItemKey::TrackNumber), tag.get_string(ItemKey::TrackTotal)), (Some("3"), Some("12")));
        write(&path, &Changes { fields: vec![(Field::Track, "4".into())], cover: CoverChange::Keep }).unwrap();
        let tag = lofty::read_from_path(&path).unwrap();
        assert_eq!(tag.primary_tag().unwrap().get_string(ItemKey::TrackTotal), None, "the old total goes");
    }
}
