//! Plug-ins, in a build with the `plugins` feature: loading them, and the
//! Plug-ins pages of Preferences that turn them on and off.

use crate::dialogs::DialogKind;
use crate::model::{AppEvent, Model};
use crate::panels::Act;
use crate::widgets::{Event, Form, PageEntry, Rect, Widget, BUTTON};
use popamp_audio::dsp::Effect;
use popamp_audio::formats::Decoders;
use popamp_audio::EngineCmd;
use popamp_core::config::PluginConfig;
use popamp_plugins::installed::{self, Installed, State};
use popamp_plugins::manifest::Manifest;
use std::path::{Path, PathBuf};

/// The plug-ins in the plug-ins folder and what became of each.
#[derive(Default)]
pub struct Plugins {
    pub list: Vec<Installed>,
    /// The folders removed since the player started.
    pub removed: Vec<PathBuf>,
    /// Requests to load the plug-ins; only the last one's answer counts.
    loads: u64,
}

impl Model {
    /// Loads the plug-ins the user allowed, on a thread of its own:
    /// compiling a module takes a while.
    pub fn load_plugins(&mut self) {
        self.plugins.loads += 1;
        let n = self.plugins.loads;
        let (roots, config, cx, events) = (self.paths.plugins_dirs(), self.config.plugins(), self.source_context(), self.events.clone());
        let own = [self.paths.config_dir.clone(), self.paths.data_dir.clone()];
        let _ = std::thread::Builder::new().name("popamp-plugins".into()).spawn(move || {
            let list = installed::load_all(&roots, &config, &cx, &own);
            let effects = installed::effects(&list);
            events.send(AppEvent::Plugins(n, list, effects));
        });
    }

    /// Puts the source plug-ins that are on in front of the built-in
    /// sources, adds the formats of the decoder ones, and gives the engine
    /// the dsp ones.
    pub fn on_plugins(&mut self, n: u64, list: Vec<Installed>, effects: Vec<Box<dyn Effect>>) {
        if n != self.plugins.loads {
            return;
        }
        for p in list.iter().filter_map(|i| if let State::On(p) = &i.state { Some(p) } else { None }) {
            let (events, id) = (self.events.clone(), p.manifest().id.clone());
            p.when_stopped(Box::new(move |why| {
                events.send(AppEvent::PluginStopped(id.clone(), why.to_string()));
            }));
        }
        // The engine first: a track waiting for the sources opens next,
        // and may be of a plug-in's format.
        self.decoders = Decoders::new(installed::decoders(&list));
        self.push_engine_settings();
        self.sources.set_plugins(installed::sources(&list), self.decoders.clone());
        self.engine.send(EngineCmd::Effects(effects));
        self.plugins.list = list;
    }

    /// Deletes a plug-in's folder, one in the user's plug-ins folder and
    /// nowhere else, and takes back its permission, so a new copy asks
    /// again. Loads the plug-ins anew.
    pub fn remove_plugin(&mut self, dir: &Path, id: Option<&str>) {
        if dir.parent() != Some(self.paths.plugins_dir().as_path()) {
            log::warn!("not removing {}: not in the plug-ins folder", dir.display());
            return;
        }
        if let Err(e) = std::fs::remove_dir_all(dir) {
            log::warn!("cannot remove {}: {e}", dir.display());
            self.flash(tr!("CANNOT REMOVE PLUG-IN: {e}", e));
            return;
        }
        if let Some(id) = id {
            self.config.set_plugin_grant(id, "");
        }
        self.plugins.removed.push(dir.to_path_buf());
        self.load_plugins();
    }

    /// The visual plug-ins that are on, to open from the menu.
    #[cfg(feature = "visualizers")]
    pub fn visualizer_items(&self) -> Vec<crate::menu::MenuItem> {
        use crate::menu::{MenuAction, MenuItem};
        let plugins = installed::visuals(&self.plugins.list);
        let open = plugins.iter().map(|p| MenuItem::action(&p.manifest().name, "", MenuAction::Visualizer(p.manifest().id.clone())));
        plugins.first().map(|_| MenuItem::sep()).into_iter().chain(open).collect()
    }

    /// The visual plug-in with this id, if it is on.
    #[cfg(feature = "visualizers")]
    pub fn visual_plugin(&self, id: &str) -> Option<std::sync::Arc<popamp_plugins::host::Plugin>> {
        installed::visuals(&self.plugins.list).into_iter().find(|p| p.manifest().id == id)
    }

    /// Says in the title display that a plug-in stopped, and why; the
    /// Plug-ins page says it too.
    pub fn on_plugin_stopped(&mut self, id: &str, why: &str) {
        let name = self.plugins.list.iter().filter_map(Installed::manifest).find(|m| m.id == id).map_or(id, |m| m.name.as_str());
        log::warn!("plug-in {id} stopped: {why}");
        self.flash(tr!("PLUG-IN {name} STOPPED: {why}", name = name.to_uppercase(), why));
    }
}

/// The Plug-ins page of Preferences, and under it a page for every
/// plug-in in the plug-ins folders: what it does, a tick that turns it on,
/// what it may do and its settings, greyed out while it is off. OK allows
/// what each ticked plug-in says it may do.
pub struct PluginPages {
    /// The Plug-ins page.
    pub page: usize,
    rows: Vec<PluginRow>,
    /// The page of each plug-in, in the order of `Plugins::list` when the
    /// pages were made: the list itself changes as plug-ins load anew.
    pages: Vec<PluginPage>,
    col: Column,
    /// What was ticked and typed when the pages opened, to tell whether
    /// OK changes anything.
    before: String,
}

/// A plug-in's page, and what is needed to remove it.
struct PluginPage {
    page: usize,
    name: String,
    dir: PathBuf,
    id: Option<String>,
    /// Its controls, put away once it is removed.
    controls: std::ops::Range<usize>,
    removed: bool,
}

struct PluginRow {
    /// Its place in `Plugins::list`.
    plugin: usize,
    id: String,
    /// The permissions it asks for now, stored as allowed when it is on.
    grant: String,
    on: bool,
    /// The hosts its manifest lists, and whether each is ticked.
    hosts: Vec<(String, bool)>,
    /// The field with the folders it may read, if it reads files.
    folders: Option<usize>,
    fields: Vec<(String, usize)>,
    /// What only a plug-in that is on lets the user change.
    tunables: Vec<usize>,
    /// The lines with the commands it runs, shown on request.
    commands: Vec<usize>,
}

impl PluginRow {
    /// Greys out what it lets the user change while it is off.
    fn grey_out(&self, form: &mut Form<Act>) {
        for &i in &self.tunables {
            form.set_disabled(i, !self.on);
        }
    }
}

/// Where the pages are laid out in the form: their left edge and width.
#[derive(Clone, Copy)]
pub(crate) struct Column {
    pub x: i32,
    pub w: i32,
}

impl Column {
    /// A line `height` high at `y`, `indent` in from the left.
    fn at(self, indent: i32, y: i32, height: i32) -> Rect {
        Rect::new(self.x + indent, y, self.w - indent, height)
    }
}

/// What the Plug-ins page says: how a plug-in is turned on.
const HELP: [&str; 3] = [
    tr_const!("Plug-ins add sources, file types, sound effects and visualizations to Popamp. They are off until you turn them on."),
    tr_const!("To turn one on, pick it in the list on the left, read what it does and what it may do, and tick Enabled. Its settings are under it."),
    tr_const!("OK keeps what you chose; Cancel forgets it."),
];

/// How far in a page puts its headings and what is under them.
const HEADING: i32 = 4;
const BODY: i32 = 20;

/// Lines of `text`, broken between words, on `page` at `y`, which moves
/// down past them. Returns the labels.
fn lines(form: &mut Form<Act>, page: usize, (col, indent): (Column, i32), y: &mut i32, text: &str, dim: bool) -> Vec<usize> {
    wrapped(text)
        .into_iter()
        .map(|text| {
            let i = form.add_on(page, col.at(indent, *y, 20), Widget::Label { text, dim });
            *y += 20;
            i
        })
        .collect()
}

impl PluginPages {
    /// Adds the Plug-ins page and the pages of the plug-ins to `form` and
    /// to `entries`. Returns them, and how far down the tallest page goes.
    pub(crate) fn build(form: &mut Form<Act>, entries: &mut Vec<PageEntry>, col: Column, model: &Model) -> (PluginPages, i32) {
        let page = entries.len();
        entries.push(PageEntry::top(tr!("Plug-ins")));
        let mut y = 10;
        for text in HELP {
            lines(form, page, (col, HEADING), &mut y, crate::i18n::tr(text), false);
            y += 10;
        }
        let dir = model.paths.plugins_dir().display().to_string();
        let found = match model.plugins.list.is_empty() {
            true => tr!("There are none yet in {dir}.", dir),
            false => tr!("Each is a folder in {dir}.", dir),
        };
        lines(form, page, (col, HEADING), &mut y, &found, true);
        let mut bottom = y;
        let config = model.config.plugins();
        let own_dir = model.paths.plugins_dir();
        let (mut rows, mut pages) = (Vec::new(), Vec::new());
        for (n, p) in model.plugins.list.iter().enumerate() {
            let name = p.manifest().map_or(&p.folder, |m| &m.name).clone();
            entries.push(PageEntry::under(page, &name));
            let (own, first, mut y) = (entries.len() - 1, form.count(), 10);
            let removable = p.dir.parent() == Some(own_dir.as_path());
            rows.extend(plugin_page(form, (own, col), &mut y, (n, removable), p, &config));
            bottom = bottom.max(y);
            let id = p.manifest().map(|m| m.id.clone());
            pages.push(PluginPage { page: own, name, dir: p.dir.clone(), id, controls: first..form.count(), removed: false });
        }
        let mut pages = PluginPages { page, rows, pages, col, before: String::new() };
        pages.before = pages.state(form);
        (pages, bottom)
    }

    /// Acts on an event of the plug-in pages. Returns whether it was theirs.
    pub(crate) fn handle(&mut self, ev: Event<Act>, form: &mut Form<Act>, model: &mut Model) -> bool {
        match ev {
            Event::Toggled(Act::Plugin(n), on) => {
                if let Some(row) = self.rows.iter_mut().find(|r| r.plugin == n) {
                    row.on = on;
                    row.grey_out(form);
                }
            }
            Event::Toggled(Act::PluginHost(n, host), on) => {
                if let Some(row) = self.rows.iter_mut().find(|r| r.plugin == n) {
                    row.hosts[host].1 = on;
                }
            }
            Event::Toggled(Act::PluginCommands(n), on) => {
                for row in self.rows.iter().filter(|r| r.plugin == n) {
                    for &i in &row.commands {
                        form.set_hidden(i, !on);
                    }
                }
            }
            Event::Clicked(Act::PluginJournal(n)) => {
                if let Some(id) = self.pages.get(n).and_then(|pg| pg.id.as_deref()) {
                    open_journal(model, id);
                }
            }
            Event::Clicked(Act::PluginRemove(n)) => {
                if let Some(pg) = self.pages.get(n) {
                    model.request_dialog(DialogKind::RemovePlugin { dir: pg.dir.clone(), id: pg.id.clone(), name: pg.name.clone() });
                }
            }
            _ => return false,
        }
        true
    }

    /// Puts away the pages of the plug-ins removed since; `pages` is the
    /// list of pages. Returns whether one was.
    pub(crate) fn refresh(&mut self, form: &mut Form<Act>, pages: usize, model: &Model) -> bool {
        let col = self.col;
        let mut any = false;
        for pg in self.pages.iter_mut().filter(|pg| !pg.removed && model.plugins.removed.contains(&pg.dir)) {
            pg.removed = true;
            for i in pg.controls.clone() {
                form.set_hidden(i, true);
            }
            form.add_on(pg.page, col.at(HEADING, 10, 20), Widget::Label { text: tr!("{name} is removed.", name = pg.name), dim: false });
            form.set_page_label(pages, pg.page, tr!("{name} (removed)", name = pg.name));
            any = true;
        }
        any
    }

    /// Stores what is ticked and typed. Returns whether the plug-ins
    /// should load anew: something changed, or one stopped or did not
    /// load and OK tries it again.
    pub(crate) fn apply(&self, form: &Form<Act>, model: &mut Model) -> bool {
        let removed = |row: &PluginRow| self.pages.get(row.plugin).is_some_and(|pg| model.plugins.removed.contains(&pg.dir));
        let kept: Vec<&PluginRow> = self.rows.iter().filter(|r| !removed(r)).collect();
        for row in kept {
            model.config.set_plugin_grant(&row.id, if row.on { &row.grant } else { "" });
            let (off, on) = installed::stored_addresses(&row.hosts);
            model.config.set_plugin_limit(&row.id, installed::HOSTS_OFF, &off);
            model.config.set_plugin_limit(&row.id, installed::HOSTS_ON, &on);
            if let Some(i) = row.folders {
                model.config.set_plugin_limit(&row.id, installed::FOLDERS, form.text(i));
            }
            for (key, i) in &row.fields {
                model.config.set_plugin_value(&row.id, key, form.text(*i));
            }
        }
        let retry = model.plugins.list.iter().any(|p| match &p.state {
            State::Failed(..) => true,
            State::On(plugin) => plugin.stopped_because().is_some(),
            _ => false,
        });
        retry || self.state(form) != self.before
    }

    /// What is ticked and typed, in a line.
    fn state(&self, form: &Form<Act>) -> String {
        let text = |i: usize| form.text(i).to_string();
        let rows = self.rows.iter().map(|r| {
            let fields: Vec<String> = r.fields.iter().map(|(_, i)| text(*i)).collect();
            format!("{} {:?} {:?} {:?}", r.on, r.hosts, r.folders.map(text), fields)
        });
        rows.collect::<Vec<_>>().join("\n")
    }
}

/// Lays out the page of plug-in number `n` from `y` down: its name, the
/// tick that turns it on, what it does and may do, what it may reach, its settings, and
/// the commands it runs, put away until asked for. `y` ends below all but
/// those. A plug-in that cannot be read gets its folder and why.
fn plugin_page(
    form: &mut Form<Act>,
    (page, col): (usize, Column),
    y: &mut i32,
    (n, removable): (usize, bool),
    p: &Installed,
    config: &PluginConfig,
) -> Option<PluginRow> {
    let title = p.manifest().map_or_else(|| p.folder.clone(), |m| format!("{} {}", m.name, m.version));
    let room = if removable { BUTTON.0 + 8 } else { 0 };
    let line = col.at(HEADING, *y, 20);
    form.add_on(page, Rect { w: line.w - room, ..line }, Widget::Label { text: title, dim: false });
    if removable {
        let r = Rect::new(col.x + col.w - BUTTON.0, *y - 2, BUTTON.0, BUTTON.1);
        form.add_on(page, r, Widget::Button { label: tr!("Remove").into(), action: Act::PluginRemove(n), default: false });
    }
    *y += 26;
    if !removable {
        let text = tr!("Installed as a package (popamp-plugin-{folder}): remove it with your package manager.", folder = p.folder);
        lines(form, page, (col, HEADING), y, &text, true);
        *y += 6;
    }
    let Some(m) = p.manifest() else {
        lines(form, page, (col, HEADING), y, &problem(p).unwrap_or_default(), true);
        return None;
    };
    let on = matches!(p.state, State::On(_) | State::Failed(..));
    form.add_on(page, col.at(HEADING, *y, 20), Widget::Check { label: tr!("Enabled").into(), checked: on, action: Act::Plugin(n) });
    *y += 28;
    lines(form, page, (col, HEADING), y, &m.description, true);
    let grant = m.permissions.grant();
    let allowed = config.grant(&m.id);
    let more = if !allowed.is_empty() && allowed != grant { " (more than you allowed)" } else { "" };
    lines(form, page, (col, HEADING), y, &format!("It {}{more}.", m.permissions.describe()), false);
    lines(form, page, (col, HEADING), y, &problem(p).unwrap_or_default(), true);
    *y += 12;
    let mut row = PluginRow {
        plugin: n,
        id: m.id.clone(),
        grant,
        on,
        hosts: Vec::new(),
        folders: None,
        fields: Vec::new(),
        tunables: Vec::new(),
        commands: Vec::new(),
    };
    (row.hosts, row.folders) = permissions(form, page, col, y, &mut row.tunables, n, m, config);
    row.fields = settings_fields(form, page, col, y, &mut row.tunables, m, config);
    if !m.permissions.exec.is_empty() {
        let check = Widget::Check { label: tr!("Show the commands it runs").into(), checked: false, action: Act::PluginCommands(n) };
        form.add_on(page, col.at(HEADING, *y, 20), check);
        *y += 22;
        let mut below = *y;
        for command in &m.permissions.exec {
            for text in std::iter::once(command.text()).chain(command.limits()) {
                row.commands.extend(lines(form, page, (col, BODY), &mut below, &text, true));
            }
        }
        for &i in &row.commands {
            form.set_hidden(i, true);
        }
    }
    row.grey_out(form);
    Some(row)
}

/// The fields of the plug-in's own settings, by key, under a heading.
fn settings_fields(
    form: &mut Form<Act>,
    page: usize,
    col: Column,
    y: &mut i32,
    tunables: &mut Vec<usize>,
    m: &Manifest,
    config: &PluginConfig,
) -> Vec<(String, usize)> {
    if m.settings.is_empty() {
        return Vec::new();
    }
    tunables.push(form.add_on(page, col.at(HEADING, *y, 20), Widget::Label { text: tr!("Settings").into(), dim: false }));
    *y += 24;
    let mut fields = Vec::new();
    for s in &m.settings {
        tunables.push(form.add_on(page, Rect::new(col.x + BODY, *y + 2, 140, 24), Widget::Label { text: s.label.clone(), dim: false }));
        let text = config.value(&m.id, &s.key).unwrap_or(&s.default).to_string();
        let cursor = text.chars().count();
        let field = form.add_on(page, col.at(BODY + 142, *y, 24), Widget::Input { text, cursor });
        tunables.push(field);
        fields.push((s.key.clone(), field));
        *y += 28;
    }
    *y += 8;
    fields
}

/// The block where the user narrows what plug-in number `n` may do: a
/// check box per host, the folders it may read, and its journal. Comes
/// before the plug-in's own settings. Returns the hosts and the folder field.
#[allow(clippy::too_many_arguments)]
fn permissions(
    form: &mut Form<Act>,
    page: usize,
    col: Column,
    y: &mut i32,
    tunables: &mut Vec<usize>,
    n: usize,
    m: &Manifest,
    config: &PluginConfig,
) -> (Vec<(String, bool)>, Option<usize>) {
    let asks = &m.permissions;
    if asks.exec.is_empty() && asks.net.is_empty() && asks.files.is_none() {
        return (Vec::new(), None);
    }
    tunables.push(form.add_on(page, col.at(HEADING, *y, 20), Widget::Label { text: tr!("Permissions").into(), dim: false }));
    *y += 24;
    let hosts = installed::addresses(m, config);
    for (i, (address, open)) in hosts.iter().enumerate() {
        let label = match installed::caution(address) {
            Some(why) => tr!("Connect to {address} ({why})", address, why),
            None => tr!("Connect to {address}", address),
        };
        tunables.push(form.add_on(page, col.at(BODY, *y, 20), Widget::Check { label, checked: *open, action: Act::PluginHost(n, i) }));
        *y += 22;
    }
    let folders = asks.files.map(|_| {
        lines(form, page, (col, BODY), y, tr!("Folders it may read, separated by ; (~ is your whole home):"), true);
        let text = installed::folders_text(m, config, std::env::home_dir().as_deref());
        let cursor = text.chars().count();
        let field = form.add_on(page, col.at(BODY, *y, 24), Widget::Input { text, cursor });
        tunables.push(field);
        *y += 28;
        lines(form, page, (col, BODY), y, tr!("Hidden folders and the player's own stay closed."), true);
        *y += 2;
        field
    });
    let r = col.at(BODY, *y, BUTTON.1);
    form.add_on(
        page,
        Rect { w: BUTTON.0, ..r },
        Widget::Button { label: tr!("Journal").into(), action: Act::PluginJournal(n), default: false },
    );
    *y += BUTTON.1 + 12;
    (hosts, folders)
}

/// Shows what the plug-in `id` asked for; an empty file if nothing yet.
fn open_journal(model: &mut Model, id: &str) {
    let Some(path) = installed::journal_path(&model.paths.plugins_dirs(), id) else { return };
    let made = path
        .parent()
        .map_or(Ok(()), std::fs::create_dir_all)
        .and_then(|()| std::fs::OpenOptions::new().create(true).append(true).open(&path));
    // The system's text viewer: TextEdit (or the user's choice) through `open -t` on macOS.
    let mut viewer = match () {
        _ if cfg!(windows) => std::process::Command::new("notepad"),
        _ if cfg!(target_os = "macos") => {
            let mut open = std::process::Command::new("open");
            open.arg("-t");
            open
        }
        _ => std::process::Command::new("xdg-open"),
    };
    if let Err(e) = made.and_then(|_| viewer.arg(&path).spawn()) {
        log::warn!("{}: {e}", path.display());
        model.flash(tr!("CANNOT OPEN JOURNAL: {file}", file = path.display()));
    }
}

/// The characters in a line of a plug-in page.
const COLUMNS: usize = 56;

/// A command, description or message in lines that fit the page, broken
/// between words; none for no text.
fn wrapped(text: &str) -> Vec<String> {
    if text.trim().is_empty() {
        return Vec::new();
    }
    let mut lines = vec![String::new()];
    for word in text.split_whitespace() {
        let last = lines.last_mut().expect("starts with one");
        if !last.is_empty() && last.len() + 1 + word.len() > COLUMNS {
            lines.push(word.to_string());
        } else {
            if !last.is_empty() {
                last.push(' ');
            }
            last.push_str(word);
        }
    }
    lines
}

/// Why a plug-in is not working, in words.
fn problem(p: &Installed) -> Option<String> {
    match &p.state {
        State::Broken(e) => Some(tr!("Broken: {e}", e)),
        State::Failed(_, e) => Some(tr!("Did not load: {e}", e)),
        State::On(plugin) => plugin.stopped_because().map(|why| tr!("Stopped after failing: {why}. OK starts it again.", why)),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dialogs::{DialogKind, Request};
    use crate::model::{AddMode, UiReq};
    use crate::panels::tests::{click, plugins_page, press, reach, type_text};
    use crate::panels::{Panel, PanelKind};
    use crate::widgets::Key;
    use std::path::Path;

    /// A model with plug-in folders holding `manifests`; their modules are
    /// not WebAssembly, so a plug-in that is allowed fails to load.
    fn with_plugins(manifests: &[(&str, &str)], allowed: &[(&str, &str)]) -> Model {
        let mut m = Model::for_tests();
        let root = m.paths.plugins_dir();
        for (id, text) in manifests {
            std::fs::create_dir_all(root.join(id)).unwrap();
            std::fs::write(root.join(id).join("plugin.toml"), text).unwrap();
            std::fs::write(root.join(id).join("plugin.wasm"), b"").unwrap();
        }
        for (id, grant) in allowed {
            m.config.set_plugin_grant(id, grant);
        }
        m.plugins.list = installed::load_all(&m.paths.plugins_dirs(), &m.config.plugins(), &Default::default(), &[]);
        m
    }

    const YT: &str = "id = \"yt\"\nname = \"YT\"\nversion = \"2\"\napi = \"1.0\"\nkind = \"source\"\ndescription = \"Videos.\"\n[source]\nhosts = [\"x.invalid\"]\n[[permissions.exec]]\nid = \"get\"\nprogram = \"yt-dlp\"\nargs = [\"-j\", \"{url}\"]\n[permissions.exec.vars]\nurl = { type = \"url\", hosts = [\"x.invalid\"] }\n[[setting]]\nkey = \"format\"\nlabel = \"Format\"\ntype = \"text\"\ndefault = \"best\"\n";

    fn grant_of(manifest: &str) -> String {
        popamp_plugins::manifest::Manifest::parse(manifest).unwrap().permissions.grant()
    }

    #[test]
    fn a_long_command_is_broken_between_words() {
        let lines = wrapped(&"word ".repeat(30));
        assert!(lines.len() > 1 && lines.iter().all(|l| l.len() <= COLUMNS), "{lines:?}");
        assert_eq!(lines.join(" ").trim_end(), "word ".repeat(30).trim_end());
        assert_eq!(wrapped("short one"), ["short one"]);
        assert!(wrapped("").is_empty());
        assert_eq!(wrapped("an error\n  --> at 1:1"), ["an error --> at 1:1"], "lines of a message run on");
    }

    /// The controls on the Plug-ins page and the plug-ins' own pages.
    fn controls(p: &Panel) -> Vec<usize> {
        let (f, first) = (p.form(), plugins_page(p));
        (0..f.count()).filter(|&i| f.page_of(i).is_some_and(|page| page >= first)).collect()
    }

    fn text_of(w: &Widget<Act>) -> Option<String> {
        match w {
            Widget::Label { text, .. } => Some(text.clone()),
            Widget::Check { label, checked, .. } => Some(format!("[{}] {label}", if *checked { "x" } else { " " })),
            _ => None,
        }
    }

    /// The texts on the plug-in pages, put away or not; a check box as `[x] label`.
    fn labels(p: &Panel) -> Vec<String> {
        controls(p).into_iter().filter_map(|i| text_of(p.form().widget(i))).collect()
    }

    /// The control whose text is `text`, as [`labels`] gives it.
    fn find(p: &Panel, text: &str) -> usize {
        controls(p).into_iter().find(|&i| text_of(p.form().widget(i)).as_deref() == Some(text)).unwrap_or_else(|| panic!("no {text}"))
    }

    /// Clicks the control whose text is `text`, its page shown first.
    fn click_on(p: &mut Panel, m: &mut Model, text: &str) {
        let i = find(p, text);
        let r = reach(p, m, i);
        click(p, m, r);
    }

    fn inputs(p: &Panel) -> Vec<usize> {
        controls(p).into_iter().filter(|&i| matches!(p.form().widget(i), Widget::Input { .. })).collect()
    }

    #[test]
    fn plug_ins_open_in_preferences_at_a_page_that_says_how_to_turn_one_on() {
        let m = with_plugins(&[("yt", YT)], &[]);
        let p = Panel::open(PanelKind::Plugins, &m);
        assert_eq!((p.kind(), p.title()), (PanelKind::Preferences, "Popamp Preferences"));
        let help = controls(&p).into_iter().find(|&i| text_of(p.form().widget(i)).is_some_and(|t| t.starts_with("To turn one on")));
        assert!(help.is_some_and(|i| p.form().visible(i)), "{:?}", labels(&p));
        let empty = labels(&Panel::open(PanelKind::Plugins, &Model::for_tests()));
        let dir = Model::for_tests().paths.plugins_dir();
        let said = empty.join(" ");
        assert!(said.contains("There are none yet in"), "{empty:?}");
        assert!(said.contains(dir.file_name().unwrap().to_str().unwrap()), "names the folder: {empty:?}");
    }

    #[test]
    fn every_plugin_has_a_page_whose_settings_wait_for_enabled() {
        let mut m = with_plugins(&[("yt", YT), ("bad", "id = 1")], &[]);
        let mut p = Panel::open(PanelKind::Plugins, &m);
        let shown = labels(&p);
        assert!(shown.contains(&"YT 2".to_string()) && shown.contains(&"Videos.".to_string()), "{shown:?}");
        assert!(shown.contains(&"It runs yt-dlp with your rights.".to_string()), "{shown:?}");
        assert!(shown.contains(&"bad".to_string()), "a broken one is listed by its folder: {shown:?}");
        assert!(shown.iter().any(|l| l.starts_with("Broken: plugin.toml:")), "{shown:?}");
        let field = inputs(&p)[0];
        let r = reach(&mut p, &mut m, field);
        click(&mut p, &mut m, r);
        type_text(&mut p, &mut m, "!");
        assert_eq!(p.form().text(field), "best", "greyed out while off");
        click_on(&mut p, &mut m, "[ ] Enabled");
        click(&mut p, &mut m, r);
        type_text(&mut p, &mut m, "!");
        assert!(!press(&mut p, &mut m, Key::Enter), "OK closes");
        assert_eq!(m.config.plugins().grant("yt"), grant_of(YT));
        assert_eq!(m.config.plugins().value("yt", "format"), Some("best!"));
    }

    #[test]
    fn the_commands_a_plugin_runs_are_shown_on_request() {
        let mut m = with_plugins(&[("yt", YT)], &[("yt", &grant_of(YT))]);
        let mut p = Panel::open(PanelKind::Plugins, &m);
        let (command, limit) = (find(&p, "yt-dlp -j {url}"), find(&p, "{url} only on x.invalid"));
        reach(&mut p, &mut m, command);
        assert!(!p.form().visible(command) && !p.form().visible(limit), "put away");
        click_on(&mut p, &mut m, "[ ] Show the commands it runs");
        assert!(p.form().visible(command) && p.form().visible(limit), "the user reads the command");
        click_on(&mut p, &mut m, "[x] Show the commands it runs");
        assert!(!p.form().visible(command));
    }

    #[test]
    fn unticking_enabled_greys_the_settings_out_and_ok_turns_it_off() {
        let mut m = with_plugins(&[("yt", YT)], &[("yt", &grant_of(YT))]);
        let mut p = Panel::open(PanelKind::Plugins, &m);
        let field = inputs(&p)[0];
        click_on(&mut p, &mut m, "[x] Enabled");
        let r = reach(&mut p, &mut m, field);
        click(&mut p, &mut m, r);
        type_text(&mut p, &mut m, "!");
        assert_eq!(p.form().text(field), "best");
        assert!(!press(&mut p, &mut m, Key::Enter));
        assert_eq!(m.config.plugins().grant("yt"), "");
    }

    #[test]
    fn a_plugin_without_settings_or_permissions_has_only_its_tick() {
        let plain = "id = \"p\"\nname = \"P\"\nversion = \"1\"\napi = \"1.0\"\nkind = \"dsp\"\n";
        let m = with_plugins(&[("p", plain)], &[("p", "exec=;net=")]);
        let shown = labels(&Panel::open(PanelKind::Plugins, &m));
        assert!(shown.contains(&"[x] Enabled".to_string()), "{shown:?}");
        assert!(!shown.contains(&"Permissions".to_string()) && !shown.contains(&"Settings".to_string()), "{shown:?}");
    }

    const NET: &str = "id = \"n\"\nname = \"N\"\nversion = \"1\"\napi = \"1.0\"\nkind = \"source\"\n[source]\nhosts = [\"x.invalid\"]\n[permissions]\nnet = [\"a.invalid\", \"b.invalid\"]\nfiles = \"read\"\n[[setting]]\nkey = \"own\"\nlabel = \"Own\"\ntype = \"text\"\ndefault = \"d\"\n";

    #[test]
    fn hosts_can_be_unticked_and_folders_chosen_above_the_plugins_own_settings() {
        let mut m = with_plugins(&[("n", NET)], &[]);
        let mut p = Panel::open(PanelKind::Plugins, &m);
        let shown = labels(&p);
        assert!(shown.contains(&"[x] Connect to a.invalid".to_string()), "{shown:?}");
        assert!(shown.contains(&"[x] Connect to b.invalid".to_string()), "{shown:?}");
        let (folders, own) = (inputs(&p)[0], inputs(&p)[1]);
        let f = p.form();
        assert!(f.rect(folders).y < f.rect(own).y, "the permissions come first");
        assert_eq!(f.text(own), "d");
        click_on(&mut p, &mut m, "[x] Connect to b.invalid");
        assert!(labels(&p).contains(&"[x] Connect to b.invalid".to_string()), "greyed out while off");
        click_on(&mut p, &mut m, "[ ] Enabled");
        click_on(&mut p, &mut m, "[x] Connect to b.invalid");
        let field = reach(&mut p, &mut m, folders);
        click(&mut p, &mut m, field);
        type_text(&mut p, &mut m, ";/mnt");
        assert!(!press(&mut p, &mut m, Key::Enter));
        let config = m.config.plugins();
        assert_eq!(config.limit("n", installed::HOSTS_OFF), Some("b.invalid"));
        assert!(config.limit("n", installed::FOLDERS).unwrap().ends_with(";/mnt"));
        assert_eq!(config.value("n", "folders"), None, "not one of its settings");
        let again = Panel::open(PanelKind::Plugins, &m);
        assert!(labels(&again).contains(&"[ ] Connect to b.invalid".to_string()), "kept: {:?}", labels(&again));
    }

    #[test]
    fn an_unusual_port_or_a_local_address_stays_off_until_ticked() {
        let text = NET.replace("\"b.invalid\"", "\"b.invalid:8000\", \"localhost:8000\"");
        let mut m = with_plugins(&[("n", &text)], &[]);
        let mut p = Panel::open(PanelKind::Plugins, &m);
        let shown = labels(&p);
        assert!(shown.contains(&"[x] Connect to a.invalid".to_string()), "{shown:?}");
        assert!(shown.contains(&"[ ] Connect to b.invalid:8000 (unusual port)".to_string()), "{shown:?}");
        assert!(shown.contains(&"[ ] Connect to localhost:8000 (this computer or your network)".to_string()), "{shown:?}");
        click_on(&mut p, &mut m, "[ ] Enabled");
        click_on(&mut p, &mut m, "[ ] Connect to localhost:8000 (this computer or your network)");
        assert!(!press(&mut p, &mut m, Key::Enter));
        let config = m.config.plugins();
        assert_eq!(config.limit("n", installed::HOSTS_ON), Some("localhost:8000"));
        assert_eq!(config.limit("n", installed::HOSTS_OFF), Some(""));
        let again = labels(&Panel::open(PanelKind::Plugins, &m));
        assert!(again.contains(&"[x] Connect to localhost:8000 (this computer or your network)".to_string()), "{again:?}");
        assert!(again.contains(&"[ ] Connect to b.invalid:8000 (unusual port)".to_string()), "{again:?}");
    }

    #[test]
    fn a_plugin_asking_for_more_than_allowed_says_so_and_cancel_changes_nothing() {
        let mut m = with_plugins(&[("yt", YT)], &[("yt", "exec=;net=")]);
        let mut p = Panel::open(PanelKind::Plugins, &m);
        let shown = labels(&p);
        assert!(shown.contains(&"[ ] Enabled".to_string()), "{shown:?}");
        assert!(shown.contains(&"It runs yt-dlp with your rights (more than you allowed).".to_string()), "{shown:?}");
        assert!(!press(&mut p, &mut m, Key::Escape));
        assert_eq!(m.config.plugins().grant("yt"), "exec=;net=");
    }

    #[test]
    fn an_allowed_plugin_that_did_not_load_is_shown_ticked_with_the_reason() {
        let m = with_plugins(&[("yt", YT)], &[("yt", &grant_of(YT))]);
        let shown = labels(&Panel::open(PanelKind::Plugins, &m));
        assert!(shown.contains(&"[x] Enabled".to_string()), "{shown:?}");
        assert!(shown.iter().any(|l| l.starts_with("Did not load: cannot compile")), "{shown:?}");
    }

    #[test]
    fn on_a_low_screen_a_plugin_page_scrolls_above_the_buttons() {
        let mut m = with_plugins(&[("yt", YT)], &[("yt", &grant_of(YT))]);
        let mut p = Panel::open(PanelKind::Plugins, &m);
        p.form_mut().limit_height(260);
        assert_eq!(p.form().h, 260);
        let (limit, show) = (find(&p, "{url} only on x.invalid"), find(&p, "[ ] Show the commands it runs"));
        reach(&mut p, &mut m, limit);
        assert!(p.form().screen_rect(show).y > p.form().h, "below the window");
        p.form_mut().wheel(300, 100, -1000);
        let r = p.form().screen_rect(show);
        click(&mut p, &mut m, r);
        assert!(p.form().visible(limit));
        p.form_mut().wheel(300, 100, 1000);
        let f = p.form();
        let ok = (0..f.count()).find(|&i| matches!(f.widget(i), Widget::Button { action: Act::Ok, .. })).unwrap();
        let ok = f.screen_rect(ok);
        assert!(ok.y + ok.h <= f.h, "OK is in the window: {ok:?}");
        assert!(f.screen_rect(limit).y > ok.y, "the last line is below");
        p.form_mut().wheel(300, 100, -1000);
        let r = p.form().screen_rect(limit);
        assert!(r.y + r.h < ok.y, "scrolled into view: {r:?}");
    }

    /// The question Remove asked, answered yes.
    fn answer_yes(m: &mut Model) {
        let Some(UiReq::Dialog(kind, Request::Question { text, .. })) = m.ui.pop() else { panic!("no question") };
        assert!(text.starts_with("Remove the plug-in YT?"), "{text}");
        m.on_dialog(crate::dialogs::DialogResult { kind, values: vec!["yes".into()] });
    }

    #[test]
    fn remove_deletes_the_plugins_folder_after_asking_and_takes_its_permission_back() {
        let mut m = with_plugins(&[("yt", YT)], &[("yt", &grant_of(YT))]);
        let dir = m.paths.plugins_dir().join("yt");
        let loads = m.plugins.loads;
        let mut p = Panel::open(PanelKind::Plugins, &m);
        let remove =
            controls(&p).into_iter().find(|&i| matches!(p.form().widget(i), Widget::Button { label, .. } if label == "Remove")).unwrap();
        let r = reach(&mut p, &mut m, remove);
        p.form_mut().mouse_down(r.x + 2, r.y + 2, false);
        let ev = p.form_mut().mouse_up(r.x + 2, r.y + 2);
        assert!(p.handle(ev, &mut m));
        assert!(dir.exists(), "not before the answer");
        answer_yes(&mut m);
        assert!(!dir.exists());
        assert_eq!(m.config.plugins().grant("yt"), "", "a new copy asks again");
        assert_eq!(m.plugins.loads, loads + 1, "the plug-ins load anew");
        assert!(p.refresh(&m), "the page says so");
        assert!(p.form().visible(find(&p, "YT is removed.")));
        assert!(!p.form().visible(find(&p, "[x] Enabled")), "nothing left to set");
        assert!(!press(&mut p, &mut m, Key::Enter));
        assert_eq!(m.config.plugins().grant("yt"), "", "OK does not allow it again");
    }

    #[test]
    fn a_folder_outside_the_users_plugins_folder_is_never_removed() {
        let mut m = with_plugins(&[], &[]);
        let elsewhere = m.paths.plugins_dir().parent().unwrap().join("elsewhere");
        std::fs::create_dir_all(&elsewhere).unwrap();
        m.remove_plugin(&elsewhere, None);
        assert!(elsewhere.exists());
        std::fs::create_dir_all(m.paths.plugins_dir()).unwrap();
        m.remove_plugin(&m.paths.plugins_dir(), None);
        assert!(m.paths.plugins_dir().exists(), "nor the plug-ins folder itself");
    }

    #[test]
    fn ok_loads_the_plugins_again_only_when_something_changed() {
        let plain = "id = \"p\"\nname = \"P\"\nversion = \"1\"\napi = \"1.0\"\nkind = \"dsp\"\n";
        let mut m = with_plugins(&[("p", plain)], &[]);
        let loads = m.plugins.loads;
        let mut p = Panel::open(PanelKind::Plugins, &m);
        assert!(!press(&mut p, &mut m, Key::Enter));
        assert_eq!(m.plugins.loads, loads, "nothing changed");
        let mut p = Panel::open(PanelKind::Plugins, &m);
        click_on(&mut p, &mut m, "[ ] Enabled");
        assert!(!press(&mut p, &mut m, Key::Enter));
        assert_eq!(m.plugins.loads, loads + 1);
    }

    #[test]
    fn a_plugin_that_stops_while_playing_says_why() {
        let mut m = Model::for_tests();
        let fixtures = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../crates/popamp-plugins/tests/fixtures");
        let dir = m.paths.plugins_dir().join("test-dsp");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::copy(fixtures.join("test-dsp/plugin.toml"), dir.join("plugin.toml")).unwrap();
        std::fs::copy(fixtures.join("test/plugin.wasm"), dir.join("plugin.wasm")).unwrap();
        m.config.set_plugin_grant("test-dsp", "exec=;net=");
        m.config.set_plugin_value("test-dsp", "mode", "short");
        let list = installed::load_all(&m.paths.plugins_dirs(), &m.config.plugins(), &m.source_context(), &[]);
        let mut fx = installed::effects(&list);
        m.on_plugins(m.plugins.loads, list, Vec::new());
        fx[0].reset(8_000);
        for _ in 0..popamp_plugins::host::FAILURES_TO_STOP {
            fx[0].process(&mut vec![0.0; 8192]);
        }
        let shown = labels(&Panel::open(PanelKind::Plugins, &m));
        let why = "Stopped after failing: a block of 32768 bytes came back as 4. OK starts it again.";
        assert!(shown.join(" ").contains(why), "wrapped: {shown:?}");
        m.on_plugin_stopped("test-dsp", "a block came back short");
        let flashed = m.transient.as_ref().map(|t| t.0.as_str());
        assert_eq!(flashed, Some("PLUG-IN TEST DSP STOPPED: a block came back short"));
    }

    #[test]
    fn an_allowed_plugin_takes_its_links_before_the_built_in_sources() {
        let mut m = Model::for_tests();
        let fixture = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../crates/popamp-plugins/tests/fixtures/test");
        let dir = m.paths.plugins_dir().join("test");
        std::fs::create_dir_all(&dir).unwrap();
        for f in ["plugin.toml", "plugin.wasm"] {
            std::fs::copy(fixture.join(f), dir.join(f)).unwrap();
        }
        let load = |m: &Model| installed::load_all(&m.paths.plugins_dirs(), &m.config.plugins(), &m.source_context(), &[]);
        let list = load(&m);
        m.on_plugins(m.plugins.loads, list, Vec::new());
        assert_eq!(m.sources.of("https://test.invalid/list").id(), "http", "not allowed yet");
        m.config.set_plugin_grant("test", &grant_of(&std::fs::read_to_string(dir.join("plugin.toml")).unwrap()));
        let list = load(&m);
        m.on_plugins(m.plugins.loads, list, Vec::new());
        assert_eq!(m.sources.of("https://test.invalid/list").id(), "test");
        m.add_now(vec!["https://test.invalid/list".into()], AddMode::Append);
        let got: Vec<_> = m.playlist.entries().iter().map(|e| (e.location.clone(), e.file_title.clone())).collect();
        assert_eq!(got[1], ("https://test.invalid/list/2".to_string(), Some("Track 2".to_string())));
        m.on_plugins(m.plugins.loads - 1, Vec::new(), Vec::new());
        assert_eq!(m.plugins.list.len(), 1, "an older answer is dropped");
    }

    #[test]
    fn an_allowed_decoder_plays_its_files_and_offers_them_to_open() {
        let mut m = Model::for_tests();
        let fixtures = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../crates/popamp-plugins/tests/fixtures");
        let dir = m.paths.plugins_dir().join("test-decoder");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::copy(fixtures.join("test-decoder/plugin.toml"), dir.join("plugin.toml")).unwrap();
        std::fs::copy(fixtures.join("test/plugin.wasm"), dir.join("plugin.wasm")).unwrap();
        m.config.set_plugin_grant("test-decoder", &grant_of(&std::fs::read_to_string(dir.join("plugin.toml")).unwrap()));
        let list = installed::load_all(&m.paths.plugins_dirs(), &m.config.plugins(), &m.source_context(), &[]);
        m.on_plugins(m.plugins.loads, list, Vec::new());
        let song = m.paths.plugins_dir().join("song.ptest");
        let mut bytes = 8_000u32.to_le_bytes().to_vec();
        bytes.extend([0u8; 4 * 8_000]);
        std::fs::write(&song, bytes).unwrap();
        m.add_now(vec![song.to_string_lossy().into_owned()], AddMode::Append);
        let entry = &m.playlist.entries()[0];
        let title = entry.meta.as_ref().and_then(|m| m.title.clone());
        assert_eq!((title.as_deref(), entry.length_ms), (Some("A .ptest file"), Some(1_000)));
        m.request_dialog(DialogKind::OpenFiles { play: false });
        let Some(UiReq::Dialog(_, Request::Files { filter, .. })) = m.ui.pop() else { panic!() };
        assert!(filter.contains(&"*.ptest".to_string()), "{filter:?}");
        m.on_plugins(m.plugins.loads, Vec::new(), Vec::new());
        assert!(!m.source_context().decoders.plays("song.ptest"), "a decoder turned off takes its formats along");
    }
}
