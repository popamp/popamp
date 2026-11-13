//! Classic skin support: `.wsz`/`.zip` archives or unpacked
//! directories, with per-file fallback to Popamp's own default skin.

pub mod bitmap;
mod classic;
mod colors;
pub mod cursors;
pub mod layout;
mod names;
pub mod sprites;
pub mod svg;
pub mod textcfg;
pub mod vector;

pub use bitmap::{Bitmap, Rgba};
pub use cursors::{CursorImage, CursorKind};
pub use layout::{Layout, Picture};
use layout::{Shape, WindowKey};
pub use sprites::{Rect, Sheet};
pub use textcfg::{PlColors, Regions, TextStyle};

use std::collections::HashMap;
use std::io::Read;
use std::path::Path;

pub struct Skin {
    pub name: String,
    sheets: HashMap<Sheet, Bitmap>,
    pub viscolor: [u32; 24],
    pub pl: PlColors,
    /// The skin's own mouse cursors; the system cursor stands in for the rest.
    pub cursors: HashMap<CursorKind, CursorImage>,
    /// The windows and their elements.
    pub layout: Layout,
    /// A vector skin's pictures.
    pub svgs: svg::Svgs,
    /// Drawn at screen size rather than scaled up from x1.
    pub vector: bool,
    /// A vector skin's drawings of classic sheets, used in their place.
    pub sheet_svgs: HashMap<Sheet, u16>,
    /// A vector skin's small texts (title, kbps...); classic skins use
    /// `text.bmp`.
    pub text: Option<TextStyle>,
    /// A vector skin's play time; classic skins use their digit bitmaps.
    pub clock: Option<TextStyle>,
    /// A vector skin's playlist font file; otherwise `pl.font` names a
    /// system font.
    pub pl_font: Option<Vec<u8>>,
    /// The names of a vector skin's colour sets; empty when it has none.
    pub color_sets: Vec<String>,
    /// Which of `color_sets` the skin was loaded with.
    pub color_set: usize,
}

impl Skin {
    /// Popamp's own skin, sharp at any scale. The player's default skin,
    /// and where every other skin takes what it leaves out.
    pub fn builtin_vector() -> Skin {
        // The vector skin of the popamp-skin-default repository, built in.
        let files = popamp_skin_default::FILES.iter().map(|(name, data)| (name.to_string(), data.to_vec())).collect();
        // The files are part of the program; a test loads them.
        vector::load_onto(Skin::bare(), &files, None).expect("the built-in skin loads")
    }

    /// The built-in skin as x1 bitmaps, for classic skins to fall back on.
    pub fn builtin() -> Skin {
        Skin { vector: false, svgs: svg::Svgs::default(), sheet_svgs: HashMap::new(), ..Skin::builtin_vector() }
    }

    /// A skin with nothing in it, to load the built-in one onto.
    pub(crate) fn bare() -> Skin {
        Skin {
            name: String::new(),
            sheets: HashMap::new(),
            viscolor: [0; 24],
            pl: PlColors::default(),
            cursors: HashMap::new(),
            layout: Layout::default(),
            svgs: svg::Svgs::default(),
            vector: false,
            sheet_svgs: HashMap::new(),
            text: None,
            clock: None,
            pl_font: None,
            color_sets: Vec::new(),
            color_set: 0,
        }
    }

    /// Builds the classic layout for the skin's bitmaps and window shapes.
    pub(crate) fn with_layout(mut self, regions: Regions) -> Skin {
        self.layout = classic::layout(self.sheet(Sheet::Volume).h, self.sheet(Sheet::Balance).h);
        let shapes = [(WindowKey::Main, regions.normal, regions.window_shade), (WindowKey::Eq, regions.equalizer, regions.equalizer_ws)];
        for (key, normal, shade) in shapes {
            let Some(w) = self.layout.windows.iter_mut().find(|w| w.key == key) else { continue };
            w.normal.shape = normal.map(Shape::Polygons);
            if let Some(s) = &mut w.shade {
                s.shape = shade.map(Shape::Polygons);
            }
        }
        self
    }

    pub fn sheet(&self, s: Sheet) -> &Bitmap {
        &self.sheets[&s]
    }

    /// Whether `paint`, drawn with its corner at 0, 0, covers `x`, `y`
    /// (skin pixels) with at least half-opaque colour. Bitmaps are opaque.
    pub fn opaque(&self, paint: &layout::Paint, x: i32, y: i32) -> bool {
        let (x, y, src) = (x - paint.dx, y - paint.dy, paint.src);
        if x < 0 || y < 0 || x >= src.w || y >= src.h {
            return false;
        }
        match paint.picture {
            Picture::Sheet(_) => true,
            Picture::Svg(id) => self.svgs.raster(id, src, src.w, src.h).is_some_and(|r| r.px[(y * r.w + x) as usize] >> 24 >= 0x80),
        }
    }

    /// The skin has its own `nums_ex` (digits with a minus sign), which
    /// then replaces `numbers`. The built-in skin has none.
    pub fn has_nums_ex(&self) -> bool {
        self.sheets.contains_key(&Sheet::NumsEx)
    }

    /// Loads a skin archive or directory: a vector skin when it has a
    /// `skin.toml`, a classic one otherwise. `colors` names the colour set
    /// wanted of a vector skin that has several; its first is used when it
    /// has none of that name.
    pub fn load(path: &Path, colors: Option<&str>) -> Result<Skin, String> {
        let read = |key: Key| if path.is_dir() { read_dir_files(path, key) } else { read_zip_files(path, key) };
        if is_vector(path) {
            return read(vector_key).and_then(|files| vector::load(&files, colors)).map_err(|e| format!("{}: {e}", path.display()));
        }
        let name = path.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
        Ok(Self::from_files(name, &read(classic_key)?))
    }

    /// Builds a skin from lower-cased basename → bytes.
    pub fn from_files(name: String, files: &HashMap<String, Vec<u8>>) -> Skin {
        let mut skin = Skin::builtin();
        skin.name = name;
        let decode = |file: &str| -> Option<Bitmap> {
            let bytes = files.get(file)?;
            match Bitmap::decode(bytes) {
                Ok(b) if b.w > 0 && b.h > 0 => Some(b),
                Ok(_) => None,
                Err(e) => {
                    log::warn!("skin {}: cannot decode {file}: {e}", skin.name);
                    None
                }
            }
        };
        let mut found = 0;
        for sheet in Sheet::ALL {
            if let Some(b) = decode(sheet.file_name()) {
                skin.sheets.insert(sheet, b);
                found += 1;
            }
        }
        // Old skins without balance.bmp reuse their volume bitmap.
        if !files.contains_key("balance.bmp") || decode("balance.bmp").is_none() {
            if let Some(v) = decode("volume.bmp") {
                skin.sheets.insert(Sheet::Balance, v);
            }
        }
        if let Some(t) = files.get("viscolor.txt") {
            skin.viscolor = textcfg::parse_viscolor(&popamp_core::text::decode_lossy(t), &skin.viscolor);
        }
        if let Some(t) = files.get("pledit.txt") {
            // Skins that only ship pledit.txt start from the classic colours.
            skin.pl = textcfg::parse_pledit(&popamp_core::text::decode_lossy(t), &PlColors::default());
        } else if found > 0 {
            skin.pl = PlColors::default();
        }
        let regions = files.get("region.txt").map(|t| textcfg::parse_region(&popamp_core::text::decode_lossy(t))).unwrap_or_default();
        for kind in CursorKind::ALL {
            let Some(bytes) = files.get(kind.file_name()) else { continue };
            match cursors::parse(bytes) {
                Some(c) => {
                    skin.cursors.insert(kind, c);
                }
                None => log::warn!("skin {}: cannot decode {}", skin.name, kind.file_name()),
            }
        }
        if found == 0 {
            log::warn!("skin {}: no recognised bitmaps, using defaults", skin.name);
        }
        skin.with_layout(regions)
    }
}

const MAX_FILE: u64 = 16 << 20;

fn wanted(name: &str) -> bool {
    name.ends_with(".bmp") || name.ends_with(".png") || name.ends_with(".txt") || name.ends_with(".cur")
}

fn is_vector(path: &Path) -> bool {
    if path.is_dir() {
        return path.join(vector::FILE).is_file();
    }
    let zip = std::fs::File::open(path).ok().and_then(|f| zip::ZipArchive::new(f).ok());
    zip.is_some_and(|mut z| z.by_name(vector::FILE).is_ok())
}

/// The name a file inside a skin is known by, or `None` to skip it.
type Key = fn(&str) -> Option<String>;

/// Classic skins: the lower-case base name, wherever the file is.
fn classic_key(path: &str) -> Option<String> {
    let base = path.rsplit(['/', '\\']).next().unwrap_or("").to_ascii_lowercase();
    wanted(&base).then_some(base)
}

/// Vector skins: the path from the skin's top folder.
fn vector_key(path: &str) -> Option<String> {
    Some(path.replace('\\', "/"))
}

fn read_zip_files(path: &Path, key: Key) -> Result<HashMap<String, Vec<u8>>, String> {
    let file = std::fs::File::open(path).map_err(|e| format!("{}: {e}", path.display()))?;
    read_zip(file, key).map_err(|e| format!("{}: {e}", path.display()))
}

/// Checks downloaded bytes before they are saved as a skin: a zip archive
/// whose main window bitmap decodes.
pub fn check_archive(bytes: &[u8]) -> Result<(), String> {
    let files = read_zip(std::io::Cursor::new(bytes), classic_key)?;
    let main = files.get(Sheet::Main.file_name()).ok_or("no main window bitmap in the archive")?;
    Bitmap::decode(main).map(drop)
}

fn read_zip(file: impl Read + std::io::Seek, key: Key) -> Result<HashMap<String, Vec<u8>>, String> {
    let mut zip = zip::ZipArchive::new(file).map_err(|e| format!("not a skin archive: {e}"))?;
    let mut out = HashMap::new();
    for i in 0..zip.len() {
        let Ok(mut entry) = zip.by_index(i) else { continue };
        if entry.is_dir() || entry.size() > MAX_FILE {
            continue;
        }
        let Some(base) = key(entry.name()).filter(|k| !out.contains_key(k)) else { continue };
        let mut data = Vec::with_capacity(entry.size() as usize);
        if entry.by_ref().take(MAX_FILE).read_to_end(&mut data).is_ok() {
            out.insert(base, data);
        }
    }
    Ok(out)
}

/// Hidden files and folders, such as `.git` in a skin cloned from its
/// repository, are never part of a skin.
fn visible(entry: &std::fs::DirEntry) -> bool {
    !entry.file_name().to_string_lossy().starts_with('.')
}

fn read_dir_files(dir: &Path, key: Key) -> Result<HashMap<String, Vec<u8>>, String> {
    let mut out = HashMap::new();
    let mut stack = vec![(dir.to_path_buf(), 0)];
    while let Some((d, depth)) = stack.pop() {
        let entries = std::fs::read_dir(&d).map_err(|e| format!("{}: {e}", d.display()))?;
        for entry in entries.flatten().filter(visible) {
            let p = entry.path();
            if p.is_dir() {
                if depth < 2 {
                    stack.push((p, depth + 1));
                }
                continue;
            }
            let inside = p.strip_prefix(dir).unwrap_or(&p).to_string_lossy();
            let Some(base) = key(&inside).filter(|k| !out.contains_key(k)) else { continue };
            if entry.metadata().map(|m| m.len() <= MAX_FILE).unwrap_or(false) {
                if let Ok(data) = std::fs::read(&p) {
                    out.insert(base, data);
                }
            }
        }
    }
    Ok(out)
}

/// When a skin last changed: the newest modification time of the archive,
/// or of the folder and anything in it.
pub fn changed_at(path: &Path) -> Option<std::time::SystemTime> {
    let mut newest = std::fs::metadata(path).and_then(|m| m.modified()).ok()?;
    let mut stack = vec![(path.to_path_buf(), 0)];
    while let Some((d, depth)) = stack.pop() {
        for entry in std::fs::read_dir(&d).into_iter().flatten().flatten().filter(visible) {
            let Ok(meta) = entry.metadata() else { continue };
            newest = newest.max(meta.modified().unwrap_or(newest));
            if meta.is_dir() && depth < 3 {
                stack.push((entry.path(), depth + 1));
            }
        }
    }
    Some(newest)
}

/// Skin archives: classic `.wsz` and Popamp's vector `.psz`. Both are zip
/// files; what is inside tells which kind a skin is.
pub const ARCHIVE_EXTENSIONS: [&str; 2] = ["wsz", "psz"];

fn has_extension(path: &Path, exts: &[&str]) -> bool {
    path.extension().and_then(|e| e.to_str()).is_some_and(|e| exts.iter().any(|x| e.eq_ignore_ascii_case(x)))
}

/// A file that is a skin by its name, so opening it installs the skin.
pub fn is_skin_archive(path: &Path) -> bool {
    has_extension(path, &ARCHIVE_EXTENSIONS)
}

/// Lists installable skins in `dirs`: archives, plain `.zip` files and
/// sub-directories, by name. Of two with the same name, the one in the
/// earlier folder wins, so the user's copy hides a packaged one.
pub fn list_skins(dirs: &[std::path::PathBuf]) -> Vec<std::path::PathBuf> {
    let mut seen = std::collections::HashSet::new();
    let mut v: Vec<_> = dirs
        .iter()
        .flat_map(|dir| std::fs::read_dir(dir).into_iter().flatten().flatten())
        .map(|e| e.path())
        .filter(|p| p.is_dir() || is_skin_archive(p) || has_extension(p, &["zip"]))
        .filter(|p| seen.insert(p.file_name().map(ToOwned::to_owned)))
        .collect();
    let name = |p: &Path| p.file_name().unwrap_or_default().to_string_lossy().into_owned();
    v.sort_by(|a, b| popamp_core::text::natural_cmp(&name(a), &name(b)));
    v
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::ImageEncoder as _;

    #[test]
    fn loads_skin_cursors() {
        let mut files = HashMap::new();
        files.insert("normal.cur".to_string(), cursors::tests::sample_cur());
        files.insert("close.cur".to_string(), b"broken".to_vec());
        let skin = Skin::from_files("t".into(), &files);
        assert_eq!(skin.cursors.keys().collect::<Vec<_>>(), [&CursorKind::Normal]);
        assert!(Skin::builtin().cursors.is_empty());
    }

    #[test]
    fn builtin_sheets_have_reference_sizes() {
        let skin = Skin::builtin();
        for s in Sheet::ALL.into_iter().filter(|&s| s != Sheet::NumsEx) {
            assert_eq!((skin.sheet(s).w, skin.sheet(s).h), s.size(), "{s:?}");
        }
    }

    #[test]
    fn the_built_in_skin_is_the_vector_skin_folder() {
        let skin = Skin::builtin_vector();
        assert_eq!(skin.name, "Popamp Default");
        assert!(skin.vector);
        assert_eq!(skin.sheet_svgs.len(), Sheet::ALL.len() - 1, "all but nums_ex");
        assert_eq!(skin.viscolor[0], 0x06090d, "colours from skin.toml");
        assert!(!skin.has_nums_ex(), "its clock uses numbers");
        let classic = Skin::builtin();
        assert!(!classic.vector && classic.sheet_svgs.is_empty(), "x1 bitmaps for classic skins");
    }

    #[test]
    fn loads_from_files_with_fallbacks() {
        let mut files = HashMap::new();
        let mut img = Vec::new();
        image::codecs::png::PngEncoder::new(&mut img).write_image(&[255u8; 68 * 433 * 3], 68, 433, image::ExtendedColorType::Rgb8).unwrap();
        files.insert("volume.bmp".to_string(), img); // PNG data despite the name
        files.insert("main.bmp".to_string(), b"not an image".to_vec());
        files.insert("pledit.txt".to_string(), b"[Text]\nNormal=#112233\n".to_vec());
        let skin = Skin::from_files("t".into(), &files);
        assert_eq!(skin.sheet(Sheet::Volume).px[0], 0xffffff);
        assert_eq!(skin.sheet(Sheet::Balance).px[0], 0xffffff, "balance falls back to volume");
        assert_eq!(skin.sheet(Sheet::Main).w, 275, "bad main.bmp falls back to default");
        assert_eq!(skin.pl.normal, 0x112233);
    }

    #[test]
    fn regions_shape_the_main_and_eq_windows() {
        let region = b"[Normal]\nNumPoints=4\nPointList=0,0, 275,0, 275,116, 0,116\n[WindowShade]\nNumPoints=3\nPointList=0,0, 9,0, 0,9\n";
        let skin = Skin::from_files("t".into(), &HashMap::from([("region.txt".to_string(), region.to_vec())]));
        let main = skin.layout.window(WindowKey::Main).unwrap();
        assert_eq!(main.normal.shape, Some(Shape::Polygons(vec![vec![(0, 0), (275, 0), (275, 116), (0, 116)]])));
        assert_eq!(main.mode(layout::View::Shade).shape, Some(Shape::Polygons(vec![vec![(0, 0), (9, 0), (0, 9)]])));
        assert_eq!(skin.layout.window(WindowKey::Eq).unwrap().normal.shape, None);
    }

    fn zip_of(files: &[(&str, &[u8])]) -> Vec<u8> {
        use std::io::Write;
        let mut w = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
        let opts = zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);
        for (name, data) in files {
            w.start_file(*name, opts).unwrap();
            w.write_all(data).unwrap();
        }
        w.finish().unwrap().into_inner()
    }

    /// A small skin in `tests/fixtures`, made to show one feature.
    fn fixture(name: &str) -> Skin {
        Skin::load(&Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures").join(name), None).unwrap()
    }

    #[test]
    fn a_button_can_take_the_mouse_only_on_its_shape() {
        let skin = fixture("shaped");
        let main = skin.layout.window(WindowKey::Main).unwrap();
        assert!(main.normal.elements.iter().any(|e| e.hit.is_some()), "a round button");
    }

    #[test]
    fn a_skin_can_bring_its_own_fonts_and_shade_every_window() {
        let skin = fixture("fonts-shade");
        let own_font = |style: &Option<TextStyle>| style.as_ref().and_then(|t| t.font.as_ref()).is_some();
        assert!(own_font(&skin.text) && own_font(&skin.clock), "its own fonts");
        for key in WindowKey::ALL {
            assert!(skin.layout.window(key).unwrap().shade.is_some(), "{key:?} has a shade mode");
        }
    }

    #[test]
    fn a_main_window_can_be_a_bar() {
        let skin = fixture("bar");
        let main = skin.layout.window(WindowKey::Main).unwrap();
        assert_eq!(main.placement, layout::Placement::Bar(popamp_core::config::BarEdge::Bottom));
        assert!(skin.layout.window(WindowKey::Playlist).is_some() && skin.layout.window(WindowKey::Eq).is_some());
    }

    #[test]
    fn a_skin_folder_ignores_hidden_files_such_as_git() {
        let dir = std::env::temp_dir().join(format!("popamp-skin-hidden-{}", std::process::id()));
        std::fs::create_dir_all(dir.join(".git/objects")).unwrap();
        std::fs::write(dir.join("skin.toml"), b"").unwrap();
        let git_file = std::fs::File::create(dir.join(".git/objects/pack")).unwrap();
        let later = std::time::SystemTime::now() + std::time::Duration::from_secs(3600);
        git_file.set_modified(later).unwrap();
        let names: Vec<String> = read_dir_files(&dir, vector_key).unwrap().into_keys().collect();
        let changed = changed_at(&dir).unwrap();
        std::fs::remove_dir_all(&dir).unwrap();
        assert_eq!(names, ["skin.toml"]);
        assert!(changed < later, "a commit is not a change to the skin");
    }

    #[test]
    fn lists_both_kinds_of_skin_archives() {
        let dir = std::env::temp_dir().join(format!("popamp-skin-list-{}", std::process::id()));
        std::fs::create_dir_all(dir.join("folder")).unwrap();
        for f in ["a.wsz", "b.psz", "c.ZIP", "notes.txt"] {
            std::fs::write(dir.join(f), b"").unwrap();
        }
        let names: Vec<String> =
            list_skins(std::slice::from_ref(&dir)).iter().map(|p| p.file_name().unwrap().to_string_lossy().into_owned()).collect();
        std::fs::remove_dir_all(&dir).unwrap();
        assert_eq!(names, ["a.wsz", "b.psz", "c.ZIP", "folder"]);
    }

    #[test]
    fn the_users_skin_hides_a_system_one_of_the_same_name() {
        let root = std::env::temp_dir().join(format!("popamp-skin-dirs-{}", std::process::id()));
        let (user, system) = (root.join("user"), root.join("system"));
        for dir in [user.join("steel"), system.join("steel"), system.join("Modern")] {
            std::fs::create_dir_all(dir).unwrap();
        }
        std::fs::write(user.join("b.wsz"), b"").unwrap();
        let list = list_skins(&[user.clone(), system.clone(), root.join("missing")]);
        std::fs::remove_dir_all(&root).unwrap();
        assert_eq!(list, [user.join("b.wsz"), system.join("Modern"), user.join("steel")]);
    }

    #[test]
    fn skin_archives_are_told_by_their_extension() {
        assert!(is_skin_archive(Path::new("/x/Bento.WSZ")));
        assert!(is_skin_archive(Path::new("my.psz")));
        assert!(!is_skin_archive(Path::new("music.zip")), "a zip may be anything");
        assert!(!is_skin_archive(Path::new("song.mp3")));
    }

    #[test]
    fn checks_downloaded_archives() {
        let mut png = Vec::new();
        image::codecs::png::PngEncoder::new(&mut png).write_image(&[0u8; 3], 1, 1, image::ExtendedColorType::Rgb8).unwrap();
        assert_eq!(check_archive(&zip_of(&[("Skin/MAIN.BMP", &png)])), Ok(()));
        assert!(check_archive(b"<html>not found</html>").unwrap_err().contains("not a skin archive"));
        assert!(check_archive(&zip_of(&[("eqmain.bmp", &png)])).unwrap_err().contains("main window"));
        assert!(check_archive(&zip_of(&[("main.bmp", b"junk")])).is_err());
    }
}
