//! The gallery of popamp.app: its catalog of skins and plug-ins, and
//! installing or updating one in a click. Each download is checked against
//! the catalog's size and SHA-256 before it is used. Windows has no
//! packages, so there this is the way to install them.

use crate::model::{AppEvent, Events, Model};
use crate::panels::{button, Act};
use crate::pen::largest;
use crate::widgets::{fit, Event, Form, Rect, Widget, BUTTON, ROW_H};
use popamp_audio::stream::fetch_bytes;
use popamp_skin::Bitmap;
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};

const CATALOG: &str = "https://popamp.app/api/v1/catalog.json";
/// Downloads come from the site only.
const SITE: &str = "https://popamp.app/";
/// The biggest download the gallery takes.
const MAX_SIZE: u64 = 32 << 20;
const MAX_PREVIEW: u64 = 4 << 20;

/// What an entry of the catalog is.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Kind {
    Skin,
    /// A plug-in, with its kind (dsp, source, visual).
    Plugin(String),
}

/// One entry of the catalog, checked on arrival.
#[derive(Debug, Clone, PartialEq)]
pub struct Item {
    pub kind: Kind,
    /// Lower-case letters, digits and dashes: it names a file or folder.
    pub id: String,
    pub name: String,
    pub summary: String,
    pub author: String,
    pub version: String,
    pub license: String,
    pub url: String,
    pub sha256: String,
    pub size: u64,
    pub preview: Option<String>,
    /// What a plug-in may do, one line each, from its permissions.
    pub permissions: Vec<String>,
    /// The average of the stars it got, and from how many people.
    pub rating: f64,
    pub votes: u64,
    pub downloads: u64,
    /// When it last changed, "YYYY-MM-DD HH:MM:SS": it sorts as text.
    pub updated: String,
}

impl Item {
    /// The site's rank for "Top rated": the average as if every entry had
    /// started with five votes of 3.5, so a few fives do not top many
    /// good votes. Entries nobody rated come last.
    fn score(&self) -> f64 {
        if self.votes == 0 {
            return 0.0;
        }
        let votes = self.votes as f64;
        (5.0 * 3.5 + self.rating * votes) / (5.0 + votes)
    }

    fn section(&self) -> Section {
        match &self.kind {
            Kind::Skin => Section::Skins,
            Kind::Plugin(k) if k == "visual" => Section::Visuals,
            Kind::Plugin(_) => Section::Plugins,
        }
    }
}

/// The tabs of the gallery.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Section {
    Skins,
    Plugins,
    Visuals,
}

impl Section {
    const ALL: [Section; 3] = [Section::Skins, Section::Plugins, Section::Visuals];

    fn label(self) -> &'static str {
        match self {
            Section::Skins => "Skins",
            Section::Plugins => "Plug-ins",
            Section::Visuals => "Visualizations",
        }
    }
}

/// The orders of the list, the site's three.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Sort {
    Popular,
    Rating,
    New,
}

impl Sort {
    const ALL: [Sort; 3] = [Sort::Popular, Sort::Rating, Sort::New];

    fn label(self) -> &'static str {
        match self {
            Sort::Popular => "Popular",
            Sort::Rating => "Top rated",
            Sort::New => "New",
        }
    }

    /// Best first; ties by name.
    fn order(self, a: &Item, b: &Item) -> std::cmp::Ordering {
        let first = match self {
            Sort::Popular => b.downloads.cmp(&a.downloads),
            Sort::Rating => b.score().total_cmp(&a.score()),
            Sort::New => b.updated.cmp(&a.updated),
        };
        first.then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
    }
}

/// Reads the catalog. Entries that are not well formed are left out.
pub fn parse_catalog(v: &serde_json::Value) -> Result<Vec<Item>, String> {
    if v["format"].as_u64() != Some(1) {
        return Err("the catalog is in a format this version does not read".into());
    }
    let items = v["items"].as_array().ok_or("the catalog lists nothing")?;
    Ok(items.iter().filter_map(|i| parse_item(i).map_err(|e| log::warn!("gallery: {e}")).ok()).collect())
}

fn parse_item(v: &serde_json::Value) -> Result<Item, String> {
    let text = |key: &str| v[key].as_str().map(str::to_string);
    let id = text("id").filter(|id| is_id(id)).ok_or("an entry without a usable id")?;
    let bad = |what: &str| format!("{id}: {what}");
    let kind = match v["kind"].as_str() {
        Some("skin") => Kind::Skin,
        Some("plugin") => Kind::Plugin(text("plugin_kind").unwrap_or_default()),
        _ => return Err(bad("unknown kind")),
    };
    let url = text("url").filter(|u| u.starts_with(SITE)).ok_or_else(|| bad("no download on the site"))?;
    let sha256 = text("sha256").filter(|s| s.len() == 64 && s.bytes().all(|b| b.is_ascii_hexdigit())).ok_or_else(|| bad("no checksum"))?;
    let size = v["size"].as_u64().filter(|&s| s > 0 && s <= MAX_SIZE).ok_or_else(|| bad("no usable size"))?;
    Ok(Item {
        kind,
        name: text("name").unwrap_or_else(|| id.clone()),
        summary: text("summary").unwrap_or_default(),
        author: text("author").unwrap_or_default(),
        version: text("version").ok_or_else(|| bad("no version"))?,
        license: text("license").unwrap_or_default(),
        url,
        sha256: sha256.to_lowercase(),
        size,
        preview: text("preview").filter(|u| u.starts_with(SITE)),
        permissions: permissions(&v["permissions"]),
        rating: v["rating"].as_f64().filter(|r| (0.0..=5.0).contains(r)).unwrap_or(0.0),
        votes: v["rating_count"].as_u64().unwrap_or(0),
        downloads: v["downloads"].as_u64().unwrap_or(0),
        updated: text("updated_at").unwrap_or_default(),
        id,
    })
}

fn is_id(s: &str) -> bool {
    (1..=64).contains(&s.len()) && s.bytes().all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-') && !s.starts_with('-')
}

/// "May run …", "May connect to …", "May read …": what the catalog says
/// a plug-in asks for.
fn permissions(p: &serde_json::Value) -> Vec<String> {
    let mut lines = Vec::new();
    let (mut programs, mut limits) = (Vec::new(), Vec::new());
    for c in p["exec"].as_array().into_iter().flatten() {
        let program = c["command"].as_str().unwrap_or("").split(' ').next().unwrap_or("").to_string();
        let new_limits = c["limits"].as_array().into_iter().flatten().filter_map(|l| l.as_str()).map(|l| format!("  {l}"));
        programs.extend(Some(program).filter(|p| !p.is_empty() && !programs.contains(p)));
        for l in new_limits {
            if !limits.contains(&l) {
                limits.push(l);
            }
        }
    }
    if !programs.is_empty() {
        lines.push(format!("May run {}", programs.join(" and ")));
        lines.extend(limits);
    }
    let hosts: Vec<&str> = p["net"].as_array().into_iter().flatten().filter_map(|h| h.as_str()).collect();
    if !hosts.is_empty() {
        lines.push(format!("May connect to {}", hosts.join(", ")));
    }
    if p["files"].as_bool().unwrap_or(false) {
        lines.push("May read the folders you allow".into());
    }
    lines
}

/// Work for a background thread.
#[derive(Debug, Clone)]
pub enum Job {
    Catalog,
    Preview {
        id: String,
        url: String,
    },
    /// Download `item`, check it and install it into `dir`.
    Install {
        item: Box<Item>,
        dir: PathBuf,
    },
}

/// A finished job, delivered to the event loop.
#[derive(Debug)]
pub enum Fetched {
    Catalog(Result<Vec<Item>, String>),
    Preview { id: String, result: Result<Bitmap, String> },
    Installed { item: Box<Item>, result: Result<PathBuf, String> },
}

/// Runs `job` on its own thread and sends the result as [`AppEvent::Gallery`].
pub fn start(job: Job, proxy: String, events: Events) {
    let spawned = std::thread::Builder::new().name("popamp-gallery".into()).spawn(move || {
        events.send(AppEvent::Gallery(run(job, &proxy)));
    });
    if let Err(e) = spawned {
        log::warn!("cannot start a gallery download: {e}");
    }
}

fn run(job: Job, proxy: &str) -> Fetched {
    match job {
        Job::Catalog => Fetched::Catalog(
            fetch_bytes(CATALOG, proxy, 4 << 20)
                .and_then(|b| serde_json::from_slice(&b).map_err(|e| e.to_string()))
                .and_then(|v| parse_catalog(&v)),
        ),
        Job::Preview { id, url } => {
            let result = fetch_bytes(&url, proxy, MAX_PREVIEW)
                .and_then(|b| Bitmap::decode(&b))
                .map(|b| fit(&b, largest(PREVIEW.0), largest(PREVIEW.1)));
            Fetched::Preview { id, result }
        }
        Job::Install { item, dir } => {
            let result = fetch_bytes(&item.url, proxy, item.size + 1)
                .and_then(|data| verify(&item, &data).map(|()| data))
                .and_then(|data| install(&item, &dir, &data));
            Fetched::Installed { item, result }
        }
    }
}

/// Checks a download against the catalog.
pub fn verify(item: &Item, data: &[u8]) -> Result<(), String> {
    if data.len() as u64 != item.size {
        return Err(format!("the download is {} bytes, the catalog says {}", data.len(), item.size));
    }
    let sum: String = Sha256::digest(data).iter().map(|b| format!("{b:02x}")).collect();
    if sum != item.sha256 {
        return Err("the download does not match the catalog's checksum".into());
    }
    Ok(())
}

/// Puts a checked download in place: a skin into the skins folder `dir`,
/// a plug-in into a folder of its own in the plug-ins folder `dir`. What
/// was there before is replaced only once the new one is whole.
pub fn install(item: &Item, dir: &Path, data: &[u8]) -> Result<PathBuf, String> {
    let io = |e: std::io::Error| e.to_string();
    std::fs::create_dir_all(dir).map_err(io)?;
    let work = dir.join(format!(".gallery-{}", item.id));
    let _ = std::fs::remove_dir_all(&work);
    std::fs::create_dir_all(&work).map_err(io)?;
    let placed = match item.kind {
        Kind::Skin => place_skin(item, dir, &work, data),
        Kind::Plugin(_) => place_plugin(item, dir, &work, data),
    };
    let _ = std::fs::remove_dir_all(&work);
    placed
}

fn place_skin(item: &Item, dir: &Path, work: &Path, data: &[u8]) -> Result<PathBuf, String> {
    let ext = if zip_has(data, popamp_skin::vector::FILE) { "psz" } else { "wsz" };
    let name = format!("{}.{ext}", item.id);
    let staged = work.join(&name);
    std::fs::write(&staged, data).map_err(|e| e.to_string())?;
    popamp_skin::Skin::load(&staged, None).map_err(|e| format!("not a usable skin ({e})"))?;
    let dest = dir.join(name);
    std::fs::rename(&staged, &dest).map_err(|e| e.to_string())?;
    Ok(dest)
}

fn zip_has(data: &[u8], file: &str) -> bool {
    zip::ZipArchive::new(std::io::Cursor::new(data)).is_ok_and(|z| z.file_names().any(|n| n == file))
}

fn place_plugin(item: &Item, dir: &Path, work: &Path, data: &[u8]) -> Result<PathBuf, String> {
    let staged = work.join(&item.id);
    unzip(data, &staged)?;
    let manifest = std::fs::read_to_string(staged.join("plugin.toml")).map_err(|_| "no plugin.toml in the download".to_string())?;
    let id = toml_value(&manifest, "id");
    if id.as_deref() != Some(item.id.as_str()) {
        return Err(format!("the plug-in calls itself {:?}, not {:?}", id.unwrap_or_default(), item.id));
    }
    // Before the copy in place is replaced: a newer Popamp's plug-in would not start.
    if let Some(min) = toml_value(&manifest, "min_popamp") {
        popamp_core::version::check_min(&min)?;
    }
    let dest = dir.join(&item.id);
    let old = work.join("old");
    if dest.exists() {
        std::fs::rename(&dest, &old).map_err(|e| e.to_string())?;
    }
    if let Err(e) = std::fs::rename(&staged, &dest) {
        let _ = std::fs::rename(&old, &dest);
        return Err(e.to_string());
    }
    Ok(dest)
}

/// The top-level `key = "value"` of a TOML file, before any table.
fn toml_value(text: &str, key: &str) -> Option<String> {
    text.lines().take_while(|l| !l.trim_start().starts_with('[')).find_map(|l| {
        let (k, v) = l.split_once('=')?;
        (k.trim() == key).then(|| v.trim().trim_matches('"').to_string())
    })
}

/// Unpacks a zip into `to`, refusing names that leave it.
fn unzip(data: &[u8], to: &Path) -> Result<(), String> {
    let mut zip = zip::ZipArchive::new(std::io::Cursor::new(data)).map_err(|e| format!("not a zip archive: {e}"))?;
    let mut total = 0u64;
    for i in 0..zip.len() {
        let mut entry = zip.by_index(i).map_err(|e| e.to_string())?;
        let rel = entry.enclosed_name().ok_or_else(|| format!("the archive names a file outside itself: {}", entry.name()))?;
        let path = to.join(rel);
        if entry.is_dir() {
            std::fs::create_dir_all(&path).map_err(|e| e.to_string())?;
            continue;
        }
        total += entry.size();
        if total > MAX_SIZE * 4 {
            return Err("the archive unpacks to too much".into());
        }
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        let mut out = std::fs::File::create(&path).map_err(|e| e.to_string())?;
        std::io::copy(&mut entry, &mut out).map_err(|e| e.to_string())?;
    }
    Ok(())
}

/// The size previews are shown at.
pub const PREVIEW: (i32, i32) = (275, 406);

/// What the gallery installed, kept in `gallery.json` in the data folder:
/// id to version and where it went, to offer updates.
#[derive(Debug, Default, Clone, PartialEq)]
pub struct Index(BTreeMap<String, (String, PathBuf)>);

impl Index {
    pub fn load(file: &Path) -> Index {
        let Ok(text) = std::fs::read_to_string(file) else { return Index::default() };
        let v: serde_json::Value = serde_json::from_str(&text).unwrap_or_default();
        let entries = v.as_object().into_iter().flatten().filter_map(|(id, e)| {
            let (version, path) = (e["version"].as_str()?, e["path"].as_str()?);
            Some((id.clone(), (version.to_string(), PathBuf::from(path))))
        });
        Index(entries.collect())
    }

    pub fn save(&self, file: &Path) -> Result<(), String> {
        let map: serde_json::Map<String, serde_json::Value> = self
            .0
            .iter()
            .map(|(id, (version, path))| (id.clone(), serde_json::json!({ "version": version, "path": path.to_string_lossy() })))
            .collect();
        let tmp = file.with_extension("json.new");
        let text = serde_json::to_string_pretty(&serde_json::Value::Object(map)).map_err(|e| e.to_string())?;
        std::fs::write(&tmp, text).and_then(|()| std::fs::rename(&tmp, file)).map_err(|e| e.to_string())
    }

    pub fn add(&mut self, id: &str, version: &str, path: PathBuf) {
        self.0.insert(id.to_string(), (version.to_string(), path));
    }

    /// The version installed of `id`, if it is still there.
    pub fn version(&self, id: &str) -> Option<&str> {
        self.0.get(id).filter(|(_, path)| path.exists()).map(|(v, _)| v.as_str())
    }

    pub fn path(&self, id: &str) -> Option<&Path> {
        self.0.get(id).map(|(_, p)| p.as_path()).filter(|p| p.exists())
    }
}

// --- The window ------------------------------------------------------------------

/// Lines of text under the list about the selected entry.
const DETAIL_LINES: usize = 11;
/// Characters in a line of details.
const COLUMNS: usize = 42;

/// Browse the catalog, see a preview and what a plug-in may do, install
/// or update in a click.
pub struct GalleryPanel {
    pub(crate) form: Form<Act>,
    sections: usize,
    sort: usize,
    search: usize,
    list: usize,
    preview: usize,
    details: [usize; DETAIL_LINES],
    status: usize,
    action: usize,
    /// The catalog, as far as this build can use it.
    items: Vec<Item>,
    /// The places in `items` the list shows, after the search.
    shown: Vec<usize>,
    previews: HashMap<String, Bitmap>,
    index: Index,
    index_file: PathBuf,
    /// The id being downloaded.
    busy: Option<String>,
    /// Requests for `app.rs` to run in the background.
    pub(crate) jobs: Vec<Job>,
}

/// What the button does for the selected entry.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Action {
    Install,
    Update,
    /// An installed skin, current: switch to it.
    Use,
    /// An installed plug-in, current.
    Installed,
}

impl Action {
    fn label(self) -> &'static str {
        match self {
            Action::Install => "Install",
            Action::Update => "Update",
            Action::Use => "Use",
            Action::Installed => "Installed",
        }
    }
}

impl GalleryPanel {
    pub(crate) fn new(model: &Model) -> GalleryPanel {
        let (list_w, (shot_w, shot_h)) = (260, PREVIEW);
        let w = 10 + list_w + 10 + shot_w + 10;
        let mut form = Form::new(w, 0);
        let tabs =
            |labels: Vec<&str>| Widget::Tabs { labels: labels.into_iter().map(str::to_string).collect(), selected: 0, action: Act::Search };
        let sections = form.add(Rect::new(10, 10, w - 20, BUTTON.1), tabs(Section::ALL.map(Section::label).to_vec()));
        let y = 10 + BUTTON.1 + 10;
        let search = form.add(Rect::new(10, y, list_w - 80, 24), Widget::Input { text: String::new(), cursor: 0 });
        button(&mut form, 10 + list_w - 74, y, "Search", Act::Search, true);
        let sort = form.add(Rect::new(10, y + 32, list_w, 22), tabs(Sort::ALL.map(Sort::label).to_vec()));
        let list_h = 4 + 10 * ROW_H;
        let list = form.add(Rect::new(10, y + 62, list_w, list_h), Widget::List { rows: vec![], selected: None, scroll: 0 });
        let top = y + 62 + list_h + 8;
        let details = std::array::from_fn(|n| {
            form.add(Rect::new(12, top + n as i32 * ROW_H, list_w - 4, ROW_H), Widget::Label { text: String::new(), dim: n > 0 })
        });
        let preview = form.add(Rect::new(10 + list_w + 10, y, shot_w, shot_h), Widget::Image { image: None, note: String::new() });
        let y = (y + shot_h).max(top + DETAIL_LINES as i32 * ROW_H) + 14;
        let status = form.add(Rect::new(10, y, w - 2 * BUTTON.0 - 36, BUTTON.1), Widget::Label { text: String::new(), dim: true });
        let action = form.add(
            Rect::new(w - 2 * BUTTON.0 - 16, y, BUTTON.0, BUTTON.1),
            Widget::Button { label: Action::Install.label().into(), action: Act::Ok, default: false },
        );
        button(&mut form, w - BUTTON.0 - 10, y, "Close", Act::Close, false);
        form.h = y + BUTTON.1 + 10;
        let index_file = model.paths.data_dir.join("gallery.json");
        let mut p = GalleryPanel {
            form,
            sections,
            sort,
            search,
            list,
            preview,
            details,
            status,
            action,
            items: vec![],
            shown: vec![],
            previews: HashMap::new(),
            index: Index::load(&index_file),
            index_file,
            busy: None,
            jobs: vec![Job::Catalog],
        };
        p.form.set_label(p.status, "Loading the catalog…".into());
        p
    }

    fn selected(&self) -> Option<&Item> {
        self.form.selected(self.list).and_then(|i| self.shown.get(i)).map(|&i| &self.items[i])
    }

    /// Where an entry goes.
    fn folder(model: &Model, item: &Item) -> PathBuf {
        match item.kind {
            Kind::Skin => model.paths.skins_dir(),
            Kind::Plugin(_) => model.paths.plugins_dir(),
        }
    }

    fn action(&self, item: &Item) -> Action {
        match (self.index.version(&item.id), &item.kind) {
            (None, _) => Action::Install,
            (Some(v), _) if v != item.version => Action::Update,
            (Some(_), Kind::Skin) => Action::Use,
            (Some(_), Kind::Plugin(_)) => Action::Installed,
        }
    }

    fn row(&self, item: &Item) -> String {
        let tag = match self.action(item) {
            Action::Install => "",
            Action::Update => " (update)",
            Action::Use | Action::Installed => " (installed)",
        };
        format!("{} {}{tag}", item.name, item.version)
    }

    /// Shows the entries of the picked tab that have every word of the
    /// search, in the picked order.
    fn filter(&mut self) {
        let (section, sort) = (Section::ALL[self.form.tab(self.sections)], Sort::ALL[self.form.tab(self.sort)]);
        let words: Vec<String> = self.form.text(self.search).to_lowercase().split_whitespace().map(str::to_string).collect();
        let keep = |i: &Item| {
            let text = format!("{} {} {}", i.name, i.summary, i.author).to_lowercase();
            words.iter().all(|w| text.contains(w.as_str()))
        };
        let current = self.selected().map(|i| i.id.clone());
        self.shown = (0..self.items.len()).filter(|&n| self.items[n].section() == section && keep(&self.items[n])).collect();
        self.shown.sort_by(|&a, &b| sort.order(&self.items[a], &self.items[b]));
        let rows = self.shown.iter().map(|&n| self.row(&self.items[n])).collect();
        let select = current.and_then(|id| self.shown.iter().position(|&n| self.items[n].id == id)).or(Some(0));
        self.form.set_rows(self.list, rows, select);
        self.show_selected();
    }

    /// Fills the details, the preview and the button for the selection.
    fn show_selected(&mut self) {
        let Some(item) = self.selected().cloned() else {
            for &d in &self.details {
                self.form.set_label(d, String::new());
            }
            let note = self.empty_note();
            self.form.set_image(self.preview, None, note);
            return;
        };
        let lines = details(&item);
        for (n, &d) in self.details.iter().enumerate() {
            self.form.set_label(d, lines.get(n).cloned().unwrap_or_default());
        }
        self.form.set_label(self.action, self.action(&item).label().into());
        match (self.previews.get(&item.id), &item.preview) {
            (Some(img), _) => self.form.set_image(self.preview, Some(img.clone()), ""),
            (None, Some(url)) => {
                self.form.set_image(self.preview, None, "Loading the preview…");
                self.jobs.push(Job::Preview { id: item.id.clone(), url: url.clone() });
            }
            (None, None) => self.form.set_image(self.preview, None, "No preview"),
        }
    }

    /// What the preview says when the list is empty.
    fn empty_note(&self) -> &'static str {
        let section = Section::ALL[self.form.tab(self.sections)];
        if section != Section::Skins && !cfg!(feature = "plugins") {
            "Plug-ins need Popamp Plus"
        } else if self.items.is_empty() {
            ""
        } else {
            "Nothing found"
        }
    }

    pub(crate) fn handle(&mut self, ev: Event<Act>, model: &mut Model) -> bool {
        match ev {
            Event::Edited | Event::Clicked(Act::Search) | Event::Picked(..) => self.filter(),
            Event::Selected => self.show_selected(),
            Event::Clicked(Act::Ok) | Event::Activated => self.act(model),
            Event::Clicked(Act::Close) | Event::Cancel => return false,
            _ => {}
        }
        true
    }

    fn act(&mut self, model: &mut Model) {
        let Some(item) = self.selected().cloned() else { return };
        if self.busy.is_some() {
            return;
        }
        let dir = Self::folder(model, &item);
        let note = match self.action(&item) {
            Action::Use => {
                model.set_skin(self.index.path(&item.id).map(Path::to_path_buf));
                format!("Using {}.", item.name)
            }
            Action::Installed => format!("{} is installed. Turn it on in Options > Plug-ins.", item.name),
            Action::Install if foreign(&item, &dir) => format!("{} is already in {}; the gallery leaves it alone.", item.id, dir.display()),
            Action::Install | Action::Update => {
                self.busy = Some(item.id.clone());
                let note = format!("Downloading {}…", item.name);
                self.jobs.push(Job::Install { item: Box::new(item), dir });
                note
            }
        };
        self.form.set_label(self.status, note);
    }

    pub(crate) fn fetched(&mut self, f: Fetched, model: &mut Model) {
        match f {
            Fetched::Catalog(Ok(items)) => {
                let usable = |i: &Item| cfg!(feature = "plugins") || i.kind == Kind::Skin;
                let hidden = items.iter().filter(|i| !usable(i)).count();
                self.items = items.into_iter().filter(usable).collect();
                let updates = self.items.iter().filter(|i| self.action(i) == Action::Update).count();
                let mut note = format!("{} in the gallery", self.items.len());
                if updates > 0 {
                    note += &format!(", {updates} to update");
                }
                match hidden {
                    0 => {}
                    1 => note += "; 1 plug-in needs Popamp Plus",
                    n => note += &format!("; {n} plug-ins need Popamp Plus"),
                }
                self.form.set_label(self.status, note + ".");
                self.filter();
            }
            Fetched::Catalog(Err(e)) => self.form.set_label(self.status, format!("Gallery: {e}")),
            Fetched::Preview { id, result } => {
                match result {
                    Ok(img) => {
                        self.previews.insert(id.clone(), img);
                    }
                    Err(e) => log::warn!("gallery preview of {id}: {e}"),
                }
                if self.selected().is_some_and(|i| i.id == id) {
                    match self.previews.get(&id) {
                        Some(img) => self.form.set_image(self.preview, Some(img.clone()), ""),
                        None => self.form.set_image(self.preview, None, "No preview"),
                    }
                }
            }
            Fetched::Installed { item, result } => {
                self.busy = None;
                let note = match result {
                    Ok(path) => self.installed(&item, path, model),
                    Err(e) => format!("{}: {e}", item.name),
                };
                self.form.set_label(self.status, note);
                self.filter();
            }
        }
    }

    fn installed(&mut self, item: &Item, path: PathBuf, model: &mut Model) -> String {
        self.index.add(&item.id, &item.version, path.clone());
        if let Err(e) = self.index.save(&self.index_file) {
            log::warn!("gallery: cannot keep the list of installed entries: {e}");
        }
        match item.kind {
            Kind::Skin => {
                model.set_skin(Some(path));
                format!("Using {}.", item.name)
            }
            Kind::Plugin(_) => {
                #[cfg(feature = "plugins")]
                model.load_plugins();
                format!("{} {} is installed. Turn it on in Options > Plug-ins.", item.name, item.version)
            }
        }
    }
}

/// Something of that id the gallery did not put there.
fn foreign(item: &Item, dir: &Path) -> bool {
    match item.kind {
        Kind::Skin => ["wsz", "psz"].iter().any(|ext| dir.join(format!("{}.{ext}", item.id)).exists()),
        Kind::Plugin(_) => dir.join(&item.id).exists(),
    }
}

/// The lines about `item` under the list.
fn details(item: &Item) -> Vec<String> {
    let what = match &item.kind {
        Kind::Skin => "Skin".to_string(),
        Kind::Plugin(k) if k.is_empty() => "Plug-in".to_string(),
        Kind::Plugin(k) => format!("{} plug-in", if k == "dsp" { "DSP".to_string() } else { capital(k) }),
    };
    let by = Some(&item.author).filter(|a| !a.is_empty()).map(|a| format!(" by {a}")).unwrap_or_default();
    let mut lines = vec![what + &by, standing(item)];
    lines.extend(wrap(&item.summary));
    if matches!(item.kind, Kind::Plugin(_)) {
        if item.permissions.is_empty() {
            lines.push("Asks for no permissions.".into());
        }
        lines.extend(item.permissions.iter().flat_map(|l| wrap(l)));
    }
    if !item.license.is_empty() {
        lines.push(format!("License: {}", item.license));
    }
    lines
}

/// "Rated 4.5 of 5 by 12, 340 downloads".
fn standing(item: &Item) -> String {
    let rated = match item.votes {
        0 => "Not rated yet".to_string(),
        n => format!("Rated {:.1} of 5 by {n}", item.rating),
    };
    let s = if item.downloads == 1 { "" } else { "s" };
    format!("{rated}, {} download{s}", item.downloads)
}

fn capital(s: &str) -> String {
    let mut c = s.chars();
    c.next().map(|f| f.to_uppercase().chain(c).collect()).unwrap_or_default()
}

/// `text` in lines of at most [`COLUMNS`] characters, broken at spaces.
fn wrap(text: &str) -> Vec<String> {
    let mut lines: Vec<String> = Vec::new();
    for word in text.split(' ').filter(|w| !w.is_empty()) {
        match lines.last_mut() {
            Some(last) if last.chars().count() + 1 + word.chars().count() <= COLUMNS => {
                last.push(' ');
                last.push_str(word);
            }
            _ => lines.push(word.to_string()),
        }
    }
    lines
}

#[cfg(test)]
pub mod tests {
    use super::*;

    /// The catalog popamp.app served on 2026-11-10, two entries of it.
    pub const CATALOG_JSON: &str = r#"{"format":1,"items":[
        {"kind":"skin","id":"popamp-steel","name":"Popamp Steel","summary":"Brushed steel.","author":"GoodZon","version":"1.0",
         "url":"https://popamp.app/skins/popamp-steel/download?v=1.0","sha256":"7DDF38BB4B9C224AB95BC8602DA069B4D513CEDCAAD066F2F3285BAA5D05FB67",
         "size":55038,"preview":"https://popamp.app/uploads/previews/a.png","license":"MIT or Apache-2.0","api":null,"plugin_kind":null,
         "permissions":{"exec":[],"net":[],"files":false},"rating":4.5,"rating_count":2,"downloads":13,"updated_at":"2026-11-08 13:51:07"},
        {"kind":"plugin","id":"youtube","name":"YouTube","summary":"Plays YouTube links.","author":"GoodZon","version":"1.0.0",
         "url":"https://popamp.app/plugins/youtube/download?v=1.0.0","sha256":"206e12f5672d1c789f220771ef7f580a4aad38f4b79fa4a71f4733f747f5852d",
         "size":94315,"preview":null,"license":"MIT","api":"1.0","plugin_kind":"source",
         "permissions":{"exec":[{"command":"yt-dlp --no-warnings -j -- {url}","limits":["{url} only on youtube.com"]}],"net":["example.org"],"files":true}},
        {"kind":"skin","id":"../evil","version":"1","url":"https://popamp.app/x","sha256":"00","size":1},
        {"kind":"skin","id":"elsewhere","version":"1","url":"https://example.org/x.wsz","sha256":"7ddf38bb4b9c224ab95bc8602da069b4d513cedcaad066f2f3285baa5d05fb67","size":1}
    ]}"#;

    pub fn catalog() -> Vec<Item> {
        parse_catalog(&serde_json::from_str(CATALOG_JSON).unwrap()).unwrap()
    }

    #[test]
    fn reads_the_catalog_and_leaves_out_what_is_not_safe() {
        let items = catalog();
        assert_eq!(items.iter().map(|i| i.id.as_str()).collect::<Vec<_>>(), ["popamp-steel", "youtube"]);
        let (skin, plugin) = (&items[0], &items[1]);
        assert_eq!(
            (&skin.kind, skin.size, skin.sha256.as_str()),
            (&Kind::Skin, 55038, "7ddf38bb4b9c224ab95bc8602da069b4d513cedcaad066f2f3285baa5d05fb67")
        );
        assert_eq!(plugin.kind, Kind::Plugin("source".into()));
        assert_eq!(plugin.preview, None);
        assert_eq!(
            plugin.permissions,
            ["May run yt-dlp", "  {url} only on youtube.com", "May connect to example.org", "May read the folders you allow"]
        );
        assert!(skin.permissions.is_empty());
        assert_eq!((skin.rating, skin.votes, skin.downloads, skin.updated.as_str()), (4.5, 2, 13, "2026-11-08 13:51:07"));
        assert_eq!((plugin.rating, plugin.votes, plugin.downloads, plugin.updated.as_str()), (0.0, 0, 0, ""));
        assert!(parse_catalog(&serde_json::json!({"format": 2, "items": []})).is_err());
    }

    pub fn item(kind: Kind, id: &str, data: &[u8]) -> Item {
        let sha256 = Sha256::digest(data).iter().map(|b| format!("{b:02x}")).collect();
        Item {
            kind,
            id: id.into(),
            name: id.into(),
            summary: String::new(),
            author: String::new(),
            version: "1.0".into(),
            license: String::new(),
            url: format!("{SITE}{id}"),
            sha256,
            size: data.len() as u64,
            preview: None,
            permissions: vec![],
            rating: 0.0,
            votes: 0,
            downloads: 0,
            updated: String::new(),
        }
    }

    #[test]
    fn a_download_must_match_the_catalog() {
        let it = item(Kind::Skin, "a", b"skin bytes");
        assert_eq!(verify(&it, b"skin bytes"), Ok(()));
        assert!(verify(&it, b"skin bytez").unwrap_err().contains("checksum"));
        assert!(verify(&it, b"skin").unwrap_err().contains("4 bytes"));
    }

    pub fn zip_of(files: &[(&str, &[u8])]) -> Vec<u8> {
        use std::io::Write;
        let mut zip = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
        for (name, data) in files {
            zip.start_file(*name, zip::write::SimpleFileOptions::default()).unwrap();
            zip.write_all(data).unwrap();
        }
        zip.finish().unwrap().into_inner()
    }

    /// A 1×1 24-bit BMP.
    pub fn bmp() -> Vec<u8> {
        let mut b = b"BM".to_vec();
        for v in [58u32, 0, 54, 40, 1, 1] {
            b.extend_from_slice(&v.to_le_bytes());
        }
        b.extend_from_slice(&1u16.to_le_bytes());
        b.extend_from_slice(&24u16.to_le_bytes());
        for v in [0u32, 4, 2835, 2835, 0, 0] {
            b.extend_from_slice(&v.to_le_bytes());
        }
        b.extend_from_slice(&[0, 0, 255, 0]);
        b
    }

    pub fn temp(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("popamp-gallery-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        dir
    }

    #[test]
    fn a_skin_goes_into_the_skins_folder_under_its_id() {
        let dir = temp("skin");
        let data = zip_of(&[("main.bmp", &bmp())]);
        let path = install(&item(Kind::Skin, "tiny", &data), &dir, &data).unwrap();
        assert_eq!(path, dir.join("tiny.wsz"));
        assert_eq!(std::fs::read(&path).unwrap(), data);
        assert!(!dir.join(".gallery-tiny").exists());
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn a_plugin_replaces_its_old_version_when_the_new_one_is_whole() {
        let dir = temp("plugin");
        let v1 = zip_of(&[("plugin.toml", b"id = \"echo\"\nversion = \"1\"\n"), ("plugin.wasm", b"one")]);
        install(&item(Kind::Plugin("dsp".into()), "echo", &v1), &dir, &v1).unwrap();
        let v2 = zip_of(&[("plugin.toml", b"id = \"echo\"\nversion = \"2\"\n"), ("plugin.wasm", b"two")]);
        let path = install(&item(Kind::Plugin("dsp".into()), "echo", &v2), &dir, &v2).unwrap();
        assert_eq!(path, dir.join("echo"));
        assert_eq!(std::fs::read(path.join("plugin.wasm")).unwrap(), b"two");
        let broken = zip_of(&[("plugin.toml", b"id = \"other\"\n")]);
        let e = install(&item(Kind::Plugin("dsp".into()), "echo", &broken), &dir, &broken).unwrap_err();
        assert!(e.contains("\"other\""), "{e}");
        assert_eq!(std::fs::read(path.join("plugin.wasm")).unwrap(), b"two", "the old one stays");
        let newer = zip_of(&[
            (
                "plugin.toml",
                b"id = \"echo\"
min_popamp = \"99.0.0\"
",
            ),
            ("plugin.wasm", b"three"),
        ]);
        let e = install(&item(Kind::Plugin("dsp".into()), "echo", &newer), &dir, &newer).unwrap_err();
        assert!(e.starts_with("needs Popamp 99.0.0 or newer"), "{e}");
        assert_eq!(std::fs::read(path.join("plugin.wasm")).unwrap(), b"two", "the old one stays");
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn an_archive_cannot_write_outside_its_folder() {
        let dir = temp("slip");
        let data = zip_of(&[("plugin.toml", b"id = \"x\"\n"), ("../../escaped", b"no")]);
        let e = install(&item(Kind::Plugin("dsp".into()), "x", &data), &dir.join("plugins"), &data).unwrap_err();
        assert!(e.contains("outside"), "{e}");
        assert!(!dir.join("escaped").exists() && !dir.join("plugins/x").exists());
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn the_index_remembers_what_is_still_installed() {
        let dir = temp("index");
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("gallery.json");
        let mut index = Index::load(&file);
        index.add("steel", "1.0", dir.clone());
        index.add("gone", "2.0", dir.join("missing"));
        index.save(&file).unwrap();
        let back = Index::load(&file);
        assert_eq!(back, index);
        assert_eq!((back.version("steel"), back.version("gone"), back.version("other")), (Some("1.0"), None, None));
        let _ = std::fs::remove_dir_all(dir);
    }

    fn panel() -> (GalleryPanel, Model) {
        let m = Model::for_tests();
        let mut p = GalleryPanel::new(&m);
        assert!(matches!(&p.jobs[..], [Job::Catalog]));
        p.jobs.clear();
        (p, m)
    }

    fn rows(p: &GalleryPanel) -> Vec<String> {
        match p.form.widget(p.list) {
            Widget::List { rows, .. } => rows.clone(),
            _ => unreachable!(),
        }
    }

    fn label(p: &GalleryPanel, i: usize) -> String {
        match p.form.widget(i) {
            Widget::Label { text, .. } => text.clone(),
            Widget::Button { label, .. } => label.clone(),
            _ => unreachable!(),
        }
    }

    #[test]
    fn the_catalog_fills_the_list_and_shows_the_first_entry() {
        let (mut p, mut m) = panel();
        p.fetched(Fetched::Catalog(Ok(catalog())), &mut m);
        assert_eq!(rows(&p), ["Popamp Steel 1.0"], "the skins tab");
        if cfg!(feature = "plugins") {
            assert_eq!(label(&p, p.status), "2 in the gallery.");
        } else {
            assert_eq!(label(&p, p.status), "1 in the gallery; 1 plug-in needs Popamp Plus.");
        }
        assert_eq!(label(&p, p.details[0]), "Skin by GoodZon");
        assert_eq!(label(&p, p.action), "Install");
        assert!(matches!(&p.jobs[..], [Job::Preview { id, .. }] if id == "popamp-steel"));
    }

    #[cfg(feature = "plugins")]
    #[test]
    fn a_plugin_shows_what_it_may_do_before_it_is_installed() {
        let (mut p, mut m) = panel();
        p.fetched(Fetched::Catalog(Ok(catalog())), &mut m);
        let t = p.sections;
        pick(&mut p, &mut m, t, 1);
        let details: Vec<String> = p.details.iter().map(|&d| label(&p, d)).collect();
        assert_eq!(
            details[..5],
            [
                "Source plug-in by GoodZon",
                "Not rated yet, 0 downloads",
                "Plays YouTube links.",
                "May run yt-dlp",
                "{url} only on youtube.com"
            ]
        );
        assert!(matches!(p.form.widget(p.preview), Widget::Image { image: None, note } if note == "No preview"));
    }

    fn ranked(id: &str, kind: Kind, rating: f64, votes: u64, downloads: u64, updated: &str) -> Item {
        Item { rating, votes, downloads, updated: updated.into(), ..item(kind, id, id.as_bytes()) }
    }

    fn pick(p: &mut GalleryPanel, m: &mut Model, tabs: usize, n: usize) {
        let r = p.form.rect(tabs);
        let ev = p.form.mouse_down(r.x + r.w * n as i32 / 3 + 2, r.y + 2, false);
        p.handle(ev, m);
    }

    #[test]
    fn skins_plugins_and_visualizations_have_tabs_of_their_own() {
        let (mut p, mut m) = panel();
        let items = vec![
            ranked("steel", Kind::Skin, 0.0, 0, 1, "2026-01-01"),
            ranked("echo", Kind::Plugin("dsp".into()), 0.0, 0, 1, "2026-01-01"),
            ranked("bars", Kind::Plugin("visual".into()), 0.0, 0, 1, "2026-01-01"),
            ranked("tube", Kind::Plugin("source".into()), 0.0, 0, 1, "2026-01-01"),
        ];
        p.fetched(Fetched::Catalog(Ok(items)), &mut m);
        assert_eq!(rows(&p), ["steel 1.0"]);
        let t = p.sections;
        pick(&mut p, &mut m, t, 1);
        let plugins: &[&str] = if cfg!(feature = "plugins") { &["echo 1.0", "tube 1.0"] } else { &[] };
        assert_eq!(rows(&p), plugins);
        let t = p.sections;
        pick(&mut p, &mut m, t, 2);
        let visuals: &[&str] = if cfg!(feature = "plugins") { &["bars 1.0"] } else { &[] };
        assert_eq!(rows(&p), visuals);
        if !cfg!(feature = "plugins") {
            assert!(matches!(p.form.widget(p.preview), Widget::Image { note, .. } if note == "Plug-ins need Popamp Plus"));
        }
    }

    #[test]
    fn sorts_like_the_site_by_downloads_rating_or_date() {
        let (mut p, mut m) = panel();
        let items = vec![
            // Two votes of 5 rank below twenty of 4.5, as on the site.
            ranked("few-fives", Kind::Skin, 5.0, 2, 10, "2026-03-01 10:00:00"),
            ranked("many", Kind::Skin, 4.5, 20, 30, "2026-01-01 10:00:00"),
            ranked("unrated", Kind::Skin, 0.0, 0, 50, "2026-02-01 10:00:00"),
        ];
        p.fetched(Fetched::Catalog(Ok(items)), &mut m);
        assert_eq!(rows(&p), ["unrated 1.0", "many 1.0", "few-fives 1.0"], "popular first");
        let t = p.sort;
        pick(&mut p, &mut m, t, 1);
        assert_eq!(rows(&p), ["many 1.0", "few-fives 1.0", "unrated 1.0"]);
        let t = p.sort;
        pick(&mut p, &mut m, t, 2);
        assert_eq!(rows(&p), ["few-fives 1.0", "unrated 1.0", "many 1.0"]);
        assert_eq!(label(&p, p.details[1]), "Not rated yet, 50 downloads", "the selection stays");
        let r = p.form.rect(p.list);
        let ev = p.form.mouse_down(r.x + 5, r.y + 3, false);
        p.handle(ev, &mut m);
        assert_eq!(label(&p, p.details[1]), "Rated 5.0 of 5 by 2, 10 downloads");
    }

    #[test]
    fn the_search_keeps_entries_with_every_word() {
        let (mut p, mut m) = panel();
        let mut items = catalog();
        items[1].kind = Kind::Skin;
        p.fetched(Fetched::Catalog(Ok(items)), &mut m);
        for c in "plays you".chars() {
            let ev = p.form.key(crate::widgets::Key::Char(c));
            p.handle(ev, &mut m);
        }
        assert_eq!(rows(&p), ["YouTube 1.0.0"]);
        let ev = p.form.key(crate::widgets::Key::Char('z'));
        p.handle(ev, &mut m);
        assert!(matches!(p.form.widget(p.preview), Widget::Image { note, .. } if note == "Nothing found"));
    }

    #[test]
    fn installing_a_skin_uses_it_and_remembers_its_version() {
        let (mut p, mut m) = panel();
        let data = zip_of(&[("main.bmp", &bmp())]);
        let mut skin = item(Kind::Skin, "tiny", &data);
        skin.name = "Tiny".into();
        p.fetched(Fetched::Catalog(Ok(vec![skin.clone()])), &mut m);
        p.jobs.clear();
        p.handle(Event::Clicked(Act::Ok), &mut m);
        let [Job::Install { item, dir }] = &p.jobs[..] else { panic!("{:?}", p.jobs) };
        assert_eq!((&**item, dir), (&skin, &m.paths.skins_dir()));
        let result = install(item, dir, &data);
        p.fetched(Fetched::Installed { item: Box::new(skin.clone()), result }, &mut m);
        assert_eq!(m.skin_path, Some(m.paths.skins_dir().join("tiny.wsz")));
        assert_eq!(
            (label(&p, p.status), rows(&p), label(&p, p.action)),
            ("Using Tiny.".into(), vec!["Tiny 1.0 (installed)".into()], "Use".into())
        );
        assert_eq!(Index::load(&m.paths.data_dir.join("gallery.json")).version("tiny"), Some("1.0"));
    }

    #[test]
    fn a_newer_version_in_the_catalog_is_an_update() {
        let (mut p, mut m) = panel();
        let data = zip_of(&[("main.bmp", &bmp())]);
        let old = item(Kind::Skin, "tiny", &data);
        let path = install(&old, &m.paths.skins_dir(), &data).unwrap();
        p.index.add("tiny", "0.9", path);
        p.fetched(Fetched::Catalog(Ok(vec![old])), &mut m);
        assert_eq!((rows(&p), label(&p, p.action)), (vec!["tiny 1.0 (update)".to_string()], "Update".to_string()));
        assert_eq!(label(&p, p.status), "1 in the gallery, 1 to update.");
    }

    #[test]
    fn what_the_gallery_did_not_install_is_left_alone() {
        let (mut p, mut m) = panel();
        let dir = m.paths.skins_dir();
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("tiny.wsz"), b"mine").unwrap();
        p.fetched(Fetched::Catalog(Ok(vec![item(Kind::Skin, "tiny", b"x")])), &mut m);
        p.jobs.clear();
        p.handle(Event::Clicked(Act::Ok), &mut m);
        assert!(p.jobs.is_empty());
        assert!(label(&p, p.status).contains("leaves it alone"));
        assert_eq!(std::fs::read(dir.join("tiny.wsz")).unwrap(), b"mine");
    }

    /// Installs everything popamp.app offers, into a folder of its own:
    /// `cargo test -p popamp gallery_live -- --ignored`.
    #[test]
    #[ignore = "uses the network"]
    fn gallery_live_everything_installs() {
        let Fetched::Catalog(items) = run(Job::Catalog, "") else { unreachable!() };
        let items = items.unwrap();
        assert!(!items.is_empty());
        let dir = temp("live");
        for item in items {
            let Fetched::Installed { item, result } = run(Job::Install { item: Box::new(item), dir: dir.join("x") }, "") else {
                unreachable!()
            };
            let path = result.unwrap_or_else(|e| panic!("{}: {e}", item.id));
            println!("{} {} -> {}", item.id, item.version, path.display());
            #[cfg(feature = "plugins")]
            if let Kind::Plugin(_) = item.kind {
                let m = popamp_plugins::manifest::Manifest::load(&path).unwrap_or_else(|e| panic!("{}: {e}", item.id));
                assert_eq!((m.id.as_str(), m.version.as_str()), (item.id.as_str(), item.version.as_str()));
            }
            if let Some(url) = item.preview {
                let Fetched::Preview { result, .. } = run(Job::Preview { id: item.id, url }, "") else { unreachable!() };
                let img = result.unwrap();
                assert!(img.w <= PREVIEW.0 && img.h <= PREVIEW.1);
            }
        }
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn details_wrap_at_spaces() {
        assert_eq!(wrap("a bb  ccc"), ["a bb ccc"]);
        let long = "word ".repeat(12);
        assert!(wrap(&long).iter().all(|l| l.chars().count() <= COLUMNS) && wrap(&long).len() == 2);
    }
}
