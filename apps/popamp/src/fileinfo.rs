//! The File info window, which changes the tags and the cover of a file,
//! and the Album art window, which shows the cover of the current track.

use crate::dialogs::DialogKind;
use crate::model::Model;
use crate::panels::{button, Act};
use crate::pen::largest;
use crate::widgets::{fit, Event, Form, Rect, Widget, BUTTON, ROW_H};
use popamp_audio::cover::{self, Cover};
use popamp_audio::tags::{self, Changes, CoverChange, Field};
use popamp_core::cue;
use popamp_core::playlist::{is_url, Entry};
use popamp_skin::Bitmap;
use std::path::{Path, PathBuf};

const W: i32 = 620;
const KEY_W: i32 = 100;
/// The text boxes end before the cover.
const FIELD_W: i32 = 310;
const COVER: i32 = 180;
const COVER_X: i32 = W - COVER - 10;
const FIELD_H: i32 = 22;
const FIELD_STEP: i32 = 26;

/// Tags and format of the selected (or current) entry; the tags and the
/// cover can be changed when the file is one Popamp can write.
pub struct FileInfo {
    pub(crate) form: Form<Act>,
    edit: Option<Editor>,
}

struct Editor {
    path: PathBuf,
    /// Each field's text box and the value it started with.
    inputs: Vec<(Field, usize, String)>,
    cover: usize,
    change: CoverChange,
    status: usize,
}

impl FileInfo {
    pub fn new(model: &Model) -> FileInfo {
        let entry = model.info_entry();
        match entry.and_then(editable) {
            Some(file) => FileInfo::editor(file),
            None => FileInfo::viewer(entry, &model.file_info_fields()),
        }
    }

    /// The fields as text boxes, the format as labels, the cover with its
    /// buttons.
    fn editor(file: Writable) -> FileInfo {
        let Writable { path, values, rows } = file;
        let mut form = Form::new(W, 0);
        let mut y = location(&mut form, &path.to_string_lossy());
        let top = y;
        let inputs = values
            .into_iter()
            .map(|(f, value)| {
                form.add(Rect::new(10, y + 2, KEY_W - 10, ROW_H), Widget::Label { text: f.label().into(), dim: true });
                let cursor = value.chars().count();
                let i = form.add(Rect::new(KEY_W, y, FIELD_W, FIELD_H), Widget::Input { text: value.clone(), cursor });
                y += FIELD_STEP;
                (f, i, value)
            })
            .collect();
        y = labels(&mut form, y + 4, rows.iter().filter(|(k, _)| !is_field(k)), FIELD_W);
        let cover = form.add(Rect::new(COVER_X, top, COVER, COVER), Widget::Image { image: shown(&path), note: tr!("No cover").into() });
        let by = top + COVER + 8;
        button(&mut form, COVER_X, by, tr!("Change…"), Act::ChangeCover, false);
        button(&mut form, W - 10 - BUTTON.0, by, tr!("Remove"), Act::RemoveCover, false);
        let y = y.max(by + BUTTON.1) + 8;
        let status = form.add(Rect::new(10, y, W - 20, ROW_H), Widget::Label { text: String::new(), dim: false });
        let by = y + ROW_H + 8;
        button(&mut form, W - 2 * BUTTON.0 - 16, by, tr!("Save"), Act::Ok, true);
        button(&mut form, W - BUTTON.0 - 10, by, tr!("Close"), Act::Close, false);
        form.h = by + BUTTON.1 + 10;
        FileInfo { form, edit: Some(Editor { path, inputs, cover, change: CoverChange::Keep, status }) }
    }

    /// `rows` as labels, with the cover and why nothing can be changed.
    fn viewer(entry: Option<&Entry>, rows: &[(String, String)]) -> FileInfo {
        let mut form = Form::new(W, 0);
        let local = entry.filter(|e| !is_url(&e.location)).map(|e| PathBuf::from(&e.location));
        let y = labels(&mut form, 10, rows.iter(), FIELD_W);
        if let Some(path) = &local {
            form.add(Rect::new(COVER_X, 10, COVER, COVER), Widget::Image { image: shown(path), note: tr!("No cover").into() });
        }
        let why = entry.map_or("", |e| why_shown(&e.location));
        let y = y.max(if local.is_some() { 10 + COVER } else { 0 }) + 8;
        form.add(Rect::new(10, y, W - 20, ROW_H), Widget::Label { text: why.into(), dim: true });
        let by = y + ROW_H + 8;
        button(&mut form, W - BUTTON.0 - 10, by, tr!("Close"), Act::Close, true);
        form.h = by + BUTTON.1 + 10;
        FileInfo { form, edit: None }
    }

    /// Acts on what happened in the form. Returns `false` when the window
    /// should close.
    pub fn handle(&mut self, ev: Event<Act>, model: &mut Model) -> bool {
        match ev {
            Event::Clicked(Act::Ok) => self.save(model),
            Event::Clicked(Act::ChangeCover) => {
                let dir = self.edit.as_ref().and_then(|e| e.path.parent()).map(Path::to_path_buf);
                model.request_dialog(DialogKind::PickCover { dir });
                true
            }
            Event::Clicked(Act::RemoveCover) => {
                self.remove_cover();
                true
            }
            Event::Clicked(_) | Event::Cancel => false,
            _ => true,
        }
    }

    /// Writes what changed. Keeps the window open, saying why, when that
    /// fails.
    fn save(&mut self, model: &mut Model) -> bool {
        let Some(ed) = &self.edit else { return false };
        let fields: Vec<(Field, String)> = ed
            .inputs
            .iter()
            .map(|(f, i, was)| (*f, self.form.text(*i).trim(), was))
            .filter(|(_, now, was)| now != &was.trim())
            .map(|(f, now, _)| (f, now.to_string()))
            .collect();
        if fields.is_empty() && ed.change == CoverChange::Keep {
            return false;
        }
        match tags::write(&ed.path, &Changes { fields, cover: ed.change.clone() }) {
            Ok(()) => {
                model.tags_changed(&ed.path.to_string_lossy());
                false
            }
            Err(e) => {
                self.form.set_label(ed.status, tr!("Not saved: {e}", e));
                true
            }
        }
    }

    /// The picture at `path` becomes the cover, once saved.
    pub fn cover_picked(&mut self, path: &Path) {
        let Some(ed) = &mut self.edit else { return };
        let picture = std::fs::read(path).ok().and_then(tags::picture);
        match picture.as_ref().and_then(|p| Bitmap::decode(&p.data).ok()) {
            Some(img) => {
                self.form.set_image(ed.cover, Some(fit(&img, largest(COVER), largest(COVER))), "");
                self.form.set_label(ed.status, String::new());
                ed.change = CoverChange::Set(picture.expect("decoded"));
            }
            None => self.form.set_label(ed.status, tr!("{file} is not a JPEG, PNG, GIF or BMP picture.", file = path.display())),
        }
    }

    /// No pictures in the tags, once saved; a cover file beside the track
    /// still shows.
    fn remove_cover(&mut self) {
        let Some(ed) = &mut self.edit else { return };
        ed.change = CoverChange::Remove;
        let beside = cover::beside(&ed.path).and_then(|p| decode_file(&p));
        self.form.set_image(ed.cover, beside.map(|b| fit(&b, largest(COVER), largest(COVER))), tr!("No cover"));
    }
}

/// A file whose tags Popamp can change, as read from the file itself: the
/// playlist may hold older tags, or none yet.
struct Writable {
    path: PathBuf,
    values: Vec<(Field, String)>,
    /// What File info says about the format.
    rows: Vec<(String, String)>,
}

/// The file of `e`, when Popamp can change its tags.
fn editable(e: &Entry) -> Option<Writable> {
    let path = PathBuf::from(&e.location);
    if is_url(&e.location) || !tags::editable(&path) {
        return None;
    }
    let values = tags::read(&path).ok()?;
    // A file Popamp edits is of a format of its own, not a plug-in's.
    let rows = popamp_audio::read_meta(&path, &Default::default()).map(|m| crate::model::meta_rows(&m)).unwrap_or_default();
    Some(Writable { path, values, rows })
}

/// Why the tags of what is at `location` are shown but cannot be changed.
fn why_shown(location: &str) -> &'static str {
    let path = Path::new(location);
    if is_url(location) {
        tr!("A stream's tags cannot be changed.")
    } else if cue::split_location(location).is_some() {
        tr!("This track is part of a whole-disc image; its tags are in the CUE sheet.")
    } else if !path.exists() {
        tr!("The file is not there.")
    } else if tags::named_editable(path) {
        tr!("Popamp cannot read this file: it may be damaged or still downloading.")
    } else {
        tr!("Popamp can change the tags of MP3, FLAC, Ogg Vorbis and MP4 files.")
    }
}

fn is_field(key: &str) -> bool {
    key == tr!("Location") || Field::ALL.iter().any(|f| f.label() == key)
}

/// The location across the top. Returns where the next row goes.
fn location(form: &mut Form<Act>, path: &str) -> i32 {
    form.add(Rect::new(10, 10, KEY_W - 10, ROW_H), Widget::Label { text: tr!("Location").into(), dim: true });
    form.add(Rect::new(KEY_W, 10, W - KEY_W - 10, ROW_H), Widget::Label { text: path.into(), dim: false });
    10 + ROW_H + 8
}

/// `rows` as key and value labels from `y` down. Returns where the next row
/// goes.
fn labels<'a>(form: &mut Form<Act>, mut y: i32, rows: impl Iterator<Item = &'a (String, String)>, value_w: i32) -> i32 {
    for (k, v) in rows {
        form.add(Rect::new(10, y, KEY_W - 10, ROW_H), Widget::Label { text: k.clone(), dim: true });
        form.add(Rect::new(KEY_W, y, value_w, ROW_H), Widget::Label { text: v.clone(), dim: false });
        y += ROW_H;
    }
    y
}

/// The cover of the track at `path`, sized for the File info window.
fn shown(path: &Path) -> Option<Bitmap> {
    cover_of(path).map(|b| fit(&b, largest(COVER), largest(COVER)))
}

/// The cover of the track at `path`: in its tags, else a file beside it.
pub fn cover_of(path: &Path) -> Option<Bitmap> {
    match cover::find(path)? {
        Cover::Tagged(p) => Bitmap::decode(&p.data).ok(),
        Cover::File(f) => decode_file(&f),
    }
}

fn decode_file(path: &Path) -> Option<Bitmap> {
    Bitmap::decode(&std::fs::read(path).ok()?).ok()
}

// --- Album art -------------------------------------------------------------------

const ART: i32 = 300;

/// The cover of the current track, following it.
pub struct AlbumArt {
    pub(crate) form: Form<Act>,
    image: usize,
    /// The track whose cover shows, and the tags generation it was read at.
    shown: Option<(String, u64)>,
}

impl AlbumArt {
    pub fn new(model: &Model) -> AlbumArt {
        let mut form = Form::new(ART, ART);
        let image = form.add(Rect::new(0, 0, ART, ART), Widget::Image { image: None, note: String::new() });
        let mut p = AlbumArt { form, image, shown: Some((String::new(), u64::MAX)) };
        p.refresh(model);
        p
    }

    /// Shows the cover of the current track if it changed, or its tags did.
    /// Returns whether it did.
    pub fn refresh(&mut self, model: &Model) -> bool {
        let now = model.playing_entry().map(|e| (e.location.clone(), model.tags_generation));
        if now == self.shown {
            return false;
        }
        let image = now.as_ref().filter(|(l, _)| !is_url(l)).and_then(|(l, _)| cover_of(Path::new(l)));
        let note = if now.is_some() { tr!("No cover") } else { tr!("Nothing to play") };
        self.form.set_image(self.image, image.map(|b| fit(&b, largest(ART), largest(ART))), note);
        self.shown = now;
        true
    }

    pub fn handle(&mut self, ev: Event<Act>) -> bool {
        !matches!(ev, Event::Cancel)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::panels::tests::click_button as click;
    use crate::panels::{Panel, PanelKind};
    use crate::widgets::Key;
    use popamp_core::plformats::Item;

    /// A copy of the silent fixture of type `ext`, and a model playing it.
    fn track(ext: &str, name: &str) -> (Model, PathBuf) {
        let dir = std::env::temp_dir().join(format!("popamp-fileinfo-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join(format!("song.{ext}"));
        let fixture = Path::new(env!("CARGO_MANIFEST_DIR")).join(format!("../../crates/popamp-audio/tests/fixtures/silence.{ext}"));
        std::fs::copy(fixture, &path).unwrap();
        let mut m = Model::for_tests();
        let item = Item { location: path.to_string_lossy().into_owned(), title: None, length_ms: None };
        m.on_expanded(vec![item], crate::model::AddMode::Append);
        m.playlist.select_only(0);
        (m, path)
    }

    fn editor(p: &Panel) -> &Editor {
        let Panel::Info(info) = p else { panic!("not File info") };
        info.edit.as_ref().expect("editable")
    }

    /// Replaces the text of `field` as typed.
    fn set(p: &mut Panel, m: &mut Model, field: Field, text: &str) {
        let i = editor(p).inputs.iter().find(|(f, ..)| *f == field).unwrap().1;
        let r = p.form().rect(i);
        p.form_mut().mouse_down(r.x + 2, r.y + 2, false);
        for _ in 0..p.form().text(i).chars().count() {
            let ev = p.form_mut().key(Key::Backspace);
            p.handle(ev, m);
        }
        for c in text.chars() {
            let ev = p.form_mut().key(Key::Char(c));
            p.handle(ev, m);
        }
    }

    fn press(p: &mut Panel, m: &mut Model, key: Key) -> bool {
        let ev = p.form_mut().key(key);
        p.handle(ev, m)
    }

    fn status(p: &Panel) -> String {
        match p.form().widget(editor(p).status) {
            Widget::Label { text, .. } => text.clone(),
            _ => unreachable!(),
        }
    }

    fn texts(p: &Panel) -> Vec<String> {
        (0..p.form().count_all())
            .filter_map(|i| match p.form().widget(i) {
                Widget::Label { text, .. } | Widget::Input { text, .. } => Some(text.clone()),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn saving_writes_what_changed_and_the_playlist_reads_it_again() {
        let (mut m, path) = track("flac", "save");
        let mut p = Panel::open(PanelKind::FileInfo, &m);
        set(&mut p, &mut m, Field::Title, "New title");
        set(&mut p, &mut m, Field::Track, "2/9");
        assert!(!press(&mut p, &mut m, Key::Enter), "Save closes the window");
        let meta = popamp_audio::read_meta(&path, &Default::default()).unwrap();
        assert_eq!((meta.title.as_deref(), meta.track.as_deref()), (Some("New title"), Some("2")));
        assert!(m.playlist.entry(0).unwrap().meta.is_none(), "the entry reads its tags again");
        assert_eq!(m.tags_generation, 1);
        let p = Panel::open(PanelKind::FileInfo, &m);
        let shown = texts(&p);
        for want in ["New title", "2/9", "FLAC"] {
            assert!(shown.iter().any(|t| t == want), "{want} read from the file: {shown:?}");
        }
    }

    #[test]
    fn a_bad_value_keeps_the_window_open_and_says_why() {
        let (mut m, path) = track("mp3", "bad");
        let before = std::fs::read(&path).unwrap();
        let mut p = Panel::open(PanelKind::FileInfo, &m);
        set(&mut p, &mut m, Field::Year, "soon");
        assert!(click(&mut p, &mut m, Act::Ok));
        assert_eq!(status(&p), "Not saved: Year must be a year or a date");
        assert_eq!(std::fs::read(&path).unwrap(), before);
        assert_eq!(m.tags_generation, 0);
    }

    #[test]
    fn nothing_changed_closes_without_writing() {
        let (mut m, path) = track("ogg", "same");
        let before = std::fs::read(&path).unwrap();
        let mut p = Panel::open(PanelKind::FileInfo, &m);
        assert!(!click(&mut p, &mut m, Act::Ok));
        assert_eq!(std::fs::read(&path).unwrap(), before);
        assert!(!press(&mut Panel::open(PanelKind::FileInfo, &m), &mut m, Key::Escape));
    }

    #[test]
    fn a_picked_picture_becomes_the_cover_and_remove_takes_it() {
        let (mut m, path) = track("m4a", "cover");
        let png = path.with_file_name("art.png");
        Bitmap::new(1200, 600, 0xff00_80ff).save_png(&png).unwrap();
        let mut p = Panel::open(PanelKind::FileInfo, &m);
        assert!(click(&mut p, &mut m, Act::ChangeCover), "asks for a picture");
        let not = path.with_file_name("notes.txt");
        std::fs::write(&not, "hello").unwrap();
        p.cover_picked(&not);
        assert!(status(&p).ends_with("is not a JPEG, PNG, GIF or BMP picture."), "{}", status(&p));
        p.cover_picked(&png);
        assert_eq!(status(&p), "");
        let Widget::Image { image: Some(shown), .. } = p.form().widget(editor(&p).cover) else { panic!("no preview") };
        assert_eq!((shown.w, shown.h), (3 * COVER, 3 * COVER / 2), "fitted to the cover at ×3");
        assert!(!click(&mut p, &mut m, Act::Ok));
        assert_eq!(cover::tagged(&path).map(|c| c.data.to_vec()), Some(std::fs::read(&png).unwrap()));
        let mut p = Panel::open(PanelKind::FileInfo, &m);
        assert!(matches!(p.form().widget(editor(&p).cover), Widget::Image { image: Some(_), .. }), "shows the saved cover");
        assert!(click(&mut p, &mut m, Act::RemoveCover));
        assert!(matches!(p.form().widget(editor(&p).cover), Widget::Image { image: None, .. }));
        assert!(!click(&mut p, &mut m, Act::Ok));
        assert_eq!(cover::tagged(&path), None);
    }

    #[test]
    fn it_says_why_the_tags_cannot_be_changed() {
        let (mut m, path) = track("flac", "why");
        let says = |m: &mut Model, location: &str, words: &str| {
            m.info_file = Some(Entry::new(location.to_string()));
            let shown = texts(&Panel::open(PanelKind::FileInfo, m));
            assert!(shown.iter().any(|t| t.contains(words)), "{location}: {shown:?}");
        };
        says(&mut m, &path.with_file_name("disc.cue#4").to_string_lossy(), "CUE sheet");
        // Of a size, but nothing written yet: a download still going.
        std::fs::write(&path, vec![0u8; 4096]).unwrap();
        let file = path.to_string_lossy().into_owned();
        says(&mut m, &file, "damaged or still downloading");
        std::fs::remove_file(&path).unwrap();
        says(&mut m, &file, "not there");
    }

    #[test]
    fn files_popamp_cannot_write_are_only_shown() {
        let (mut m, _) = track("wav", "wav");
        let mut p = Panel::open(PanelKind::FileInfo, &m);
        let Panel::Info(info) = &p else { panic!() };
        assert!(info.edit.is_none());
        assert!(texts(&p).contains(&"Popamp can change the tags of MP3, FLAC, Ogg Vorbis and MP4 files.".to_string()));
        assert!(!press(&mut p, &mut m, Key::Enter), "Enter closes");
        let mut m = Model::for_tests();
        m.on_expanded(
            vec![Item { location: "http://radio.example/live".into(), title: None, length_ms: None }],
            crate::model::AddMode::Append,
        );
        m.playlist.select_only(0);
        let p = Panel::open(PanelKind::FileInfo, &m);
        assert!(texts(&p).contains(&"A stream's tags cannot be changed.".to_string()));
        assert!(texts(&Panel::open(PanelKind::FileInfo, &Model::for_tests())).contains(&"No file selected.".to_string()));
    }

    #[test]
    fn album_art_follows_the_current_track_and_its_tags() {
        let (mut m, path) = track("flac", "art");
        let png = path.with_file_name("cover.png");
        let mut art = AlbumArt::new(&m);
        let shows = |a: &AlbumArt| match a.form.widget(a.image) {
            Widget::Image { image, note } => (image.as_ref().map(|b| (b.w, b.h)), note.clone()),
            _ => unreachable!(),
        };
        assert_eq!(shows(&art), (None, "Nothing to play".into()));
        m.play_index(0);
        assert!(art.refresh(&m));
        assert_eq!(shows(&art), (None, "No cover".into()));
        assert!(!art.refresh(&m), "nothing new");
        Bitmap::new(1000, 1000, 0xffff_0000).save_png(&png).unwrap();
        m.tags_changed(&path.to_string_lossy());
        assert!(art.refresh(&m));
        assert_eq!(shows(&art).0, Some((3 * ART, 3 * ART)), "a cover file beside it, fitted to the window at ×3");
        assert!(art.handle(Event::None) && !art.handle(Event::Cancel));
    }
}
