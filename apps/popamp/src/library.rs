//! The media library window's content, laid out as Winamp's: the views on
//! the left; on the right a search field, the artists and the albums side
//! by side, the track table under them, and a row of buttons. Picking an
//! artist narrows the albums and the table, picking an album narrows the
//! table further. A skin gives the window's frame and the rectangle for
//! all this; the player lays it out there and draws it in the playlist's
//! font and colours, so it fits any skin.
//!
//! Terms:
//! - *view*: a row of the left list: all music, a smart playlist, or one
//!   folder of the library.
//! - *pane*: the list of artists or the list of albums; its first row is
//!   all of them.
//! - *shown*: the tracks in the table, after view, panes and search.
//! - *picked*: the selected tracks, or all the shown ones when none is.

use crate::canvas::Canvas;
use crate::dialogs::DialogKind;
use crate::font::Font;
use crate::model::{AddMode, AppEvent, Model, UiReq};
use crate::panels::PanelKind;
use crate::widgets::Key;
use popamp_audio::source::{Context, Sources};
use popamp_core::library::{is_under, Filter, Group, GroupRow, Library, Sort, Track};
use popamp_core::plain::{self, PlainList};
use popamp_core::replaygain::Stamp;
use popamp_core::smart::{Field, Op, Rule, SmartList, Test};
use popamp_core::stats::Rating;
use popamp_core::text::{escape_field, format_duration_ms, unescape_field};
use popamp_core::Command;
use popamp_skin::bitmap::mix;
use popamp_skin::Bitmap;
use popamp_skin::{PlColors, Rect};
use std::cell::RefCell;
use std::collections::{BTreeSet, HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

/// What a row of the views tree shows.
#[derive(Debug, Clone, PartialEq, Eq)]
enum View {
    All,
    /// A node holding the rows of one kind: a branch. It is no view.
    Branch(Branch),
    /// A smart playlist, by its place in the lists; in its own order and
    /// without the panes.
    Smart(usize),
    /// A playlist of tracks put there by hand, by its place in the lists;
    /// in its own order and without the panes.
    Plain(usize),
    /// A folder of the library, or a folder somewhere in one.
    Folder(String),
}

/// The branches of the views tree.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum Branch {
    Playlists,
    Smart,
    Folders,
}

impl Branch {
    const ALL: [Branch; 3] = [Branch::Playlists, Branch::Smart, Branch::Folders];

    /// Its name in the settings.
    fn key(self) -> &'static str {
        match self {
            Branch::Playlists => "playlists",
            Branch::Smart => "smart",
            Branch::Folders => "folders",
        }
    }

    fn label(self) -> &'static str {
        match self {
            Branch::Playlists => "Playlists",
            Branch::Smart => "Smart playlists",
            Branch::Folders => "Folders",
        }
    }
}

/// A row of the views tree: what it shows, how deep it is, and whether
/// it is open, for a node with rows under it.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Row {
    view: View,
    depth: u8,
    fold: Option<bool>,
}

/// A node of the views tree that opens: a branch, or a folder.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
enum Node {
    Branch(Branch),
    Folder(String),
}

impl Node {
    fn of(view: &View) -> Option<Node> {
        match view {
            View::Branch(b) => Some(Node::Branch(*b)),
            View::Folder(f) => Some(Node::Folder(f.clone())),
            _ => None,
        }
    }
}

/// The lists of the window, in the order Tab goes through them; also
/// where each keeps its scroll.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum List {
    Views,
    Artists,
    Albums,
    Tracks,
}

const LISTS: [List; 4] = [List::Views, List::Artists, List::Albums, List::Tracks];

/// Whose menu the right button opens.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Menu {
    /// The playlists' and the folders', over the views.
    Lists,
    /// The picked tracks'.
    Tracks,
    /// Which columns the table shows, over its header.
    Columns,
}

/// How near a border between two columns of the header a press takes
/// hold of it.
const BORDER_SLACK: i32 = 2;

/// A column of the track table.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Column {
    Artist,
    Title,
    Album,
    Time,
    Genre,
    Year,
    Rating,
    Number,
    File,
    Comment,
    Bitrate,
    Plays,
    LastPlayed,
    Added,
}

/// What sets a column apart.
struct About {
    /// Its name in the settings.
    key: &'static str,
    /// Its name over the table and in the menu of columns.
    header: &'static str,
    menu: &'static str,
    sort: Sort,
    /// Numbers sit at the right of their cell.
    right: bool,
    /// How wide it starts, in tenths of the text height.
    width: u16,
    /// How long it stays in a table too narrow for all: the lowest goes
    /// first.
    keep: u8,
    /// The menu command that shows or hides it.
    toggle: Command,
}

const fn about(key: &'static str, header: &'static str, menu: &'static str, sort: Sort, right: bool) -> About {
    About { key, header, menu, sort, right, width: 60, keep: 0, toggle: Command::LibraryColumnsReset }
}

/// By `Column`, in its order.
const ABOUT: [About; 14] = [
    About { width: 70, keep: 8, toggle: Command::LibraryColumnArtist, ..about("artist", "Artist", "Artist", Sort::Artist, false) },
    About { width: 90, keep: 9, toggle: Command::LibraryColumnTitle, ..about("title", "Title", "Title", Sort::Title, false) },
    About { width: 70, keep: 5, toggle: Command::LibraryColumnAlbum, ..about("album", "Album", "Album", Sort::Album, false) },
    About { width: 40, keep: 7, toggle: Command::LibraryColumnTime, ..about("time", "Time", "Time", Sort::Length, true) },
    About { width: 60, keep: 1, toggle: Command::LibraryColumnGenre, ..about("genre", "Genre", "Genre", Sort::Genre, false) },
    About { width: 40, keep: 3, toggle: Command::LibraryColumnYear, ..about("year", "Year", "Year", Sort::Year, true) },
    About { width: 60, keep: 2, toggle: Command::LibraryColumnRating, ..about("rating", "Rating", "Rating", Sort::Rating, false) },
    About { width: 30, keep: 4, toggle: Command::LibraryColumnNumber, ..about("track", "#", "Track number", Sort::Number, true) },
    About { width: 100, keep: 1, toggle: Command::LibraryColumnFile, ..about("file", "File name", "File name", Sort::File, false) },
    About { width: 90, toggle: Command::LibraryColumnComment, ..about("comment", "Comment", "Comment", Sort::Comment, false) },
    About { width: 50, toggle: Command::LibraryColumnBitrate, ..about("bitrate", "Kbps", "Bit rate", Sort::Bitrate, true) },
    About { width: 40, toggle: Command::LibraryColumnPlays, ..about("plays", "Plays", "Plays", Sort::Plays, true) },
    About { width: 80, toggle: Command::LibraryColumnLastPlayed, ..about("played", "Last played", "Last played", Sort::LastPlayed, false) },
    About { width: 80, toggle: Command::LibraryColumnAdded, ..about("added", "Added", "Added", Sort::Added, false) },
];

impl Column {
    const ALL: [Column; 14] = [
        Column::Artist,
        Column::Title,
        Column::Album,
        Column::Time,
        Column::Genre,
        Column::Year,
        Column::Rating,
        Column::Number,
        Column::File,
        Column::Comment,
        Column::Bitrate,
        Column::Plays,
        Column::LastPlayed,
        Column::Added,
    ];

    fn about(self) -> &'static About {
        &ABOUT[self as usize]
    }

    fn label(self) -> &'static str {
        self.about().header
    }

    fn sort(self) -> Sort {
        self.about().sort
    }

    fn right(self) -> bool {
        self.about().right
    }

    /// The column a menu command shows or hides.
    fn toggled_by(c: Command) -> Option<Column> {
        Column::ALL.into_iter().find(|col| col.about().toggle == c)
    }
}

/// How wide a column is: a share of what the fixed ones leave, or a
/// number of times the text height.
#[derive(Debug, Clone, Copy)]
enum Width {
    Share(i32),
    Px(i32),
}

/// The track table's columns as the user set them: which show, left to
/// right, each with its width in tenths of the text height. Never empty.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Columns(Vec<(Column, u16)>);

/// The columns a new library has.
const FIRST: [Column; 7] = [Column::Artist, Column::Title, Column::Album, Column::Time, Column::Genre, Column::Year, Column::Rating];

impl Default for Columns {
    fn default() -> Self {
        Columns(FIRST.iter().map(|&c| (c, c.about().width)).collect())
    }
}

impl Columns {
    /// The columns of the setting `artist:70,title:90`; a column it does
    /// not name, names twice or gives no width keeps out. None that is
    /// good is the first columns.
    pub fn parse(text: &str) -> Columns {
        let mut cols: Vec<(Column, u16)> = Vec::new();
        for (key, width) in text.split(',').filter_map(|part| part.trim().split_once(':')) {
            let col = Column::ALL.into_iter().find(|c| c.about().key == key.trim());
            if let (Some(col), Ok(w @ 1..)) = (col, width.trim().parse::<u16>()) {
                if !cols.iter().any(|c| c.0 == col) {
                    cols.push((col, w));
                }
            }
        }
        if cols.is_empty() {
            Columns::default()
        } else {
            Columns(cols)
        }
    }

    pub fn to_setting(&self) -> String {
        self.0.iter().map(|(c, w)| format!("{}:{w}", c.about().key)).collect::<Vec<_>>().join(",")
    }

    fn has(&self, col: Column) -> bool {
        self.0.iter().any(|c| c.0 == col)
    }

    /// Hides a shown column, the last one aside; shows a hidden one at the
    /// right.
    fn toggle(&mut self, col: Column) {
        match self.0.iter().position(|c| c.0 == col) {
            Some(i) if self.0.len() > 1 => _ = self.0.remove(i),
            Some(_) => {}
            None => self.0.push((col, col.about().width)),
        }
    }

    /// The columns across `rows`, edge to edge, each as wide as its share
    /// of the widths. A table too narrow for all of them at their widths
    /// leaves out the ones least kept, rightmost first.
    fn lay(&self, rows: Rect, px: i32) -> Vec<(Column, i32, i32)> {
        let mut kept = self.0.clone();
        while kept.len() > 1 && kept.iter().map(|c| c.1 as i32).sum::<i32>() * px > rows.w * 10 {
            let least = (0..kept.len()).rev().min_by_key(|&i| kept[i].0.about().keep).unwrap_or(0);
            kept.remove(least);
        }
        let shares: Vec<(Column, Width)> = kept.iter().map(|&(c, w)| (c, Width::Share(w as i32))).collect();
        lay(&shares, rows, px)
    }

    /// Moves the border right of the column `left` of `shown` (as `lay`
    /// put them) to `x`. Only it and the column right of it change; each
    /// stays at least twice the text height when there is room for that.
    fn resize(&mut self, shown: &[(Column, i32, i32)], left: usize, x: i32, px: i32) {
        let (Some(&(a, ax, aw)), Some(&(b, _, bw))) = (shown.get(left), shown.get(left + 1)) else { return };
        let (Some(ia), Some(ib)) = (self.0.iter().position(|c| c.0 == a), self.0.iter().position(|c| c.0 == b)) else { return };
        let room = aw + bw;
        let least = (2 * px).min(room / 2);
        let to = (x - ax).clamp(least, room - least);
        let pair = (self.0[ia].1 + self.0[ib].1) as i64;
        let wa = (pair * to as i64 / room.max(1) as i64).clamp(1, pair - 1);
        (self.0[ia].1, self.0[ib].1) = (wa as u16, (pair - wa) as u16);
    }
}

/// Places `cols` across `area`, left to right.
fn lay<T: Copy>(cols: &[(T, Width)], area: Rect, px: i32) -> Vec<(T, i32, i32)> {
    let fixed: i32 = cols.iter().map(|c| if let Width::Px(n) = c.1 { n * px } else { 0 }).sum();
    let shares: i32 = cols.iter().map(|c| if let Width::Share(n) = c.1 { n } else { 0 }).sum::<i32>().max(1);
    let room = (area.w - fixed).max(0);
    let (mut x, mut used, mut out) = (area.x, 0, Vec::new());
    for &(col, width) in cols {
        let end = match width {
            Width::Share(n) => {
                used += n;
                // Rounded as a running total, so the shares add up to `room`.
                x + room * used / shares - room * (used - n) / shares
            }
            Width::Px(n) => x + n * px,
        }
        .min(area.x + area.w);
        out.push((col, x, end - x));
        x = end;
    }
    out
}

const BUTTONS: [(&str, Command); 4] = [
    ("Play", Command::LibraryPlay),
    ("Enqueue", Command::LibraryEnqueue),
    ("Add folder", Command::LibraryAddFolder),
    ("Rescan", Command::LibraryRescan),
];

/// Where the library is drawn: the skin's rectangle, the text height, and
/// the size of the skin's scroll thumb, when it gives one.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Place {
    pub r: Rect,
    pub px: i32,
    pub thumb: Option<(i32, i32)>,
}

/// How wide a list's scroll bar is when the skin gives no thumb.
const OWN_BAR: i32 = 6;

/// A list with a header row above its rows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Table {
    header: Rect,
    rows: Rect,
}

/// Where the parts are in the skin's rectangle, in skin pixels.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Parts {
    views: Rect,
    /// The cover of the track at the cursor and what is known of it,
    /// under the views, when the window is tall enough.
    cover: Option<Rect>,
    about: Option<Rect>,
    search: Rect,
    /// The artists and the albums, when the view has them.
    panes: Option<[Table; 2]>,
    table: Table,
    buttons: [Rect; 4],
    status: Rect,
    row_h: i32,
    /// The width of each list's scroll bar, right of its rows, and the
    /// length of the skin's thumb.
    bar_w: i32,
    thumb_h: Option<i32>,
}

const fn rect(x: i32, y: i32, w: i32, h: i32) -> Rect {
    Rect { x, y, w, h }
}

impl Parts {
    /// Lays the parts out for `at`; `panes` leaves room for the artists and
    /// the albums above the table. Each list keeps the right of its room
    /// for its scroll bar.
    fn new(at: Place, panes: bool) -> Parts {
        let Place { r, px, .. } = at;
        let bar_w = at.thumb.map_or(OWN_BAR, |t| t.0);
        let row_h = px + 3;
        let bar_h = row_h + 4;
        let body_h = (r.h - bar_h - 2).max(0);
        let views_w = (r.w * 3 / 10).clamp(60, 120).min(r.w);
        let about_h = ABOUT_LINES as i32 * row_h;
        let panel_h = 3 + views_w + 1 + about_h;
        let panel = body_h - panel_h >= 6 * row_h;
        let views_h = if panel { body_h - panel_h } else { body_h };
        let cover_y = r.y + views_h + 3;
        let (x, w) = (r.x + views_w + 3, (r.w - views_w - 3).max(0));
        let search_h = row_h + 3;
        let (list_y, list_h) = (r.y + search_h + 2, (body_h - search_h - 2).max(0));
        let table = |x: i32, y: i32, w: i32, h: i32| {
            let head = row_h.min(h.max(0));
            Table { header: rect(x, y, w, head), rows: rect(x, y + head, (w - bar_w).max(0), (h - head).max(0)) }
        };
        let panes_h = if panes { list_h * 2 / 5 } else { 0 };
        let half = ((w - 3) / 2).max(0);
        let ty = list_y + if panes { panes_h + 3 } else { 0 };
        let button_w = (px * 5 + 4).min((r.w - 9) / 4).max(1);
        let by = r.y + r.h - bar_h;
        let buttons = [0, 1, 2, 3].map(|i| rect(r.x + i * (button_w + 3), by, button_w, bar_h));
        let after = 4 * (button_w + 3);
        Parts {
            views: rect(r.x, r.y, (views_w - bar_w).max(0), views_h),
            cover: panel.then(|| rect(r.x, cover_y, views_w, views_w)),
            about: panel.then(|| rect(r.x, cover_y + views_w + 1, views_w, about_h)),
            search: rect(x, r.y, w, search_h),
            panes: panes.then(|| [table(x, list_y, half, panes_h), table(x + half + 3, list_y, (w - half - 3).max(0), panes_h)]),
            table: table(x, ty, w, list_y + list_h - ty),
            buttons,
            status: rect(r.x + after, by, (r.w - after).max(0), bar_h),
            row_h,
            bar_w,
            thumb_h: at.thumb.map(|t| t.1),
        }
    }

    /// The scroll bar of a list, right of its rows.
    fn bar(&self, list: List) -> Option<Rect> {
        self.rows(list).map(|r| rect(r.x + r.w, r.y, self.bar_w, r.h))
    }

    /// The rows of a list.
    fn rows(&self, list: List) -> Option<Rect> {
        match list {
            List::Views => Some(self.views),
            List::Artists => self.panes.map(|p| p[0].rows),
            List::Albums => self.panes.map(|p| p[1].rows),
            List::Tracks => Some(self.table.rows),
        }
    }

    /// How many whole rows a list shows; at least one.
    fn page(&self, list: Rect) -> usize {
        (list.h / self.row_h).max(1) as usize
    }

    /// The row of `list` at height `y`, before scrolling.
    fn row(&self, list: Rect, y: i32) -> usize {
        ((y - list.y).max(0) / self.row_h) as usize
    }
}

/// How many lines tell of the track under its cover.
const ABOUT_LINES: usize = 4;

/// The cover on show: the track's, as large as the panel can show it at
/// any zoom, and that shrunk to the size it was last drawn at.
struct Shown {
    path: String,
    art: Option<Bitmap>,
    sized: Option<((i32, i32), Bitmap)>,
}

/// A scan of the folders under way.
#[derive(Debug)]
pub struct Scan {
    id: u64,
    /// Files looked at so far.
    done: usize,
    /// Set to stop it.
    cancel: Arc<AtomicBool>,
    /// The tracks the library had when it began, to find the files that
    /// were renamed or moved meanwhile.
    before: Vec<Track>,
}

/// What a scan running on its own thread reports; the number names the scan.
#[derive(Debug)]
pub enum LibEvent {
    Progress(u64, usize),
    /// What one folder holds now.
    Folder(u64, String, Vec<Track>),
    Done(u64),
    /// A cover was written into these files; into so many more it could
    /// not be.
    Covers(Vec<String>, usize),
}

/// A plain press on a table row: it carries the picked tracks once the
/// mouse moves far enough, else it is a click on `row`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Carry {
    Pressed { x: i32, y: i32, row: usize },
    Moving,
}

/// How far the mouse goes before a press carries.
const CARRY_SLACK: i32 = 4;

/// What the window layer has to do after an input.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Reaction {
    /// Keep sending the mouse here until the button goes up.
    pub capture: bool,
    pub run: Option<Command>,
}

impl Reaction {
    fn run(c: Command) -> Reaction {
        Reaction { capture: false, run: Some(c) }
    }
}

/// A smart playlist being edited: a copy of the one at `index`, or a
/// new one.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Editing {
    pub index: Option<usize>,
    pub list: SmartList,
}

pub struct LibraryPane {
    pub lib: Library,
    /// The smart playlists, from `smart.toml`.
    pub smart: Vec<SmartList>,
    /// The list the smart playlist editor opens with.
    pub editing: Option<Editing>,
    /// The playlists of tracks put there by hand, from the `playlists`
    /// folder.
    pub plain: Vec<PlainList>,
    /// How many tracks each smart and each plain playlist has, by place.
    counts: (Vec<usize>, Vec<usize>),
    /// The plain playlist carried tracks are over, which they go into
    /// when let go.
    drop_on: Option<usize>,
    view: View,
    /// The rows of the views tree as they show, made again when the
    /// library, a playlist or an open node changes.
    tree: Vec<Row>,
    /// The nodes of the tree that are open.
    open: HashSet<Node>,
    /// The shuffle a randomly ordered list is shown in, drawn anew each
    /// time a list is opened.
    seed: u64,
    /// The artist and the album picked in the panes; all when none.
    artist: Option<String>,
    album: Option<String>,
    query: String,
    sort: (Sort, bool),
    /// What the panes list.
    artists: Vec<GroupRow>,
    albums: Vec<GroupRow>,
    /// The table's tracks, as places in `lib.tracks`.
    shown: Vec<usize>,
    selected: Vec<bool>,
    /// The table row the keys move from, and where a Shift range starts.
    cursor: usize,
    anchor: usize,
    /// First visible row of each list, by `List`.
    scroll: [usize; 4],
    /// The list the arrow keys move in.
    focus: List,
    /// Where the right button last went down, which picks the menu.
    menu_for: Menu,
    pub columns: Columns,
    /// The border of the header the mouse holds: the column left of it,
    /// by its place among the shown ones.
    resizing: Option<usize>,
    /// The time zone the dates show in.
    tz: jiff::tz::TimeZone,
    /// Read when first drawn, kept while the same track is at the cursor.
    shown_cover: RefCell<Option<Shown>>,
    /// The button held down by the mouse.
    pressed: Option<usize>,
    /// The list whose thumb the mouse holds, and how far below the
    /// thumb's top it does.
    dragging: Option<(List, i32)>,
    /// The picked tracks held by the mouse, to be let go over the playlist.
    carry: Option<Carry>,
    pub scan: Option<Scan>,
    scans: u64,
}

/// The last part of a folder's path, which names it in the views.
fn folder_label(folder: &str) -> String {
    std::path::Path::new(folder).file_name().map_or_else(|| folder.to_string(), |n| n.to_string_lossy().into_owned())
}

impl LibraryPane {
    pub fn new(lib: Library) -> LibraryPane {
        let mut p = LibraryPane {
            lib,
            smart: Vec::new(),
            editing: None,
            plain: Vec::new(),
            counts: (Vec::new(), Vec::new()),
            drop_on: None,
            view: View::All,
            tree: Vec::new(),
            open: Branch::ALL.map(Node::Branch).into(),
            seed: fastrand::u64(..),
            artist: None,
            album: None,
            query: String::new(),
            sort: (Sort::Artist, false),
            artists: Vec::new(),
            albums: Vec::new(),
            shown: Vec::new(),
            selected: Vec::new(),
            cursor: 0,
            anchor: 0,
            scroll: [0; 4],
            focus: List::Tracks,
            menu_for: Menu::Tracks,
            columns: Columns::default(),
            resizing: None,
            tz: jiff::tz::TimeZone::system(),
            shown_cover: RefCell::new(None),
            pressed: None,
            dragging: None,
            carry: None,
            scan: None,
            scans: 0,
        };
        p.changed();
        p
    }

    /// Makes the rows of the views tree again.
    fn grow_tree(&mut self) {
        let mut rows = vec![Row { view: View::All, depth: 0, fold: None }];
        // The user's own playlists have a place even before there are any,
        // so it is plain where a new one goes.
        if self.grow_branch(&mut rows, Branch::Playlists, true) {
            rows.extend((0..self.plain.len()).map(|n| Row { view: View::Plain(n), depth: 1, fold: None }));
        }
        if self.grow_branch(&mut rows, Branch::Smart, !self.smart.is_empty()) {
            rows.extend((0..self.smart.len()).map(|n| Row { view: View::Smart(n), depth: 1, fold: None }));
        }
        if self.grow_branch(&mut rows, Branch::Folders, !self.lib.folders.is_empty()) {
            for f in &self.lib.folders {
                self.grow_folder(&mut rows, f, 1);
            }
        }
        self.tree = rows;
    }

    /// Adds the row of `branch` when it is `shown`. True when it is shown
    /// open, for the rows under it to follow.
    fn grow_branch(&self, rows: &mut Vec<Row>, branch: Branch, shown: bool) -> bool {
        let open = self.open.contains(&Node::Branch(branch));
        if shown {
            rows.push(Row { view: View::Branch(branch), depth: 0, fold: Some(open) });
        }
        shown && open
    }

    /// Adds the row of `folder` at `depth`, and when it is open, the rows
    /// of the folders in it.
    fn grow_folder(&self, rows: &mut Vec<Row>, folder: &str, depth: u8) {
        let inside = self.folders_in(folder);
        let open = self.open.contains(&Node::Folder(folder.to_string()));
        let fold = (!inside.is_empty()).then_some(open);
        rows.push(Row { view: View::Folder(folder.to_string()), depth, fold });
        if fold == Some(true) {
            for f in inside {
                self.grow_folder(rows, &f, depth + 1);
            }
        }
    }

    /// The folders right inside `folder` that hold tracks, at any depth,
    /// in natural order.
    fn folders_in(&self, folder: &str) -> Vec<String> {
        let base = Path::new(folder);
        let mut names = BTreeSet::new();
        for t in &self.lib.tracks {
            let file = popamp_core::cue::split_location(&t.path).map_or(t.path.as_str(), |(sheet, _)| sheet);
            let Some(rest) = Path::new(file).parent().and_then(|d| d.strip_prefix(base).ok()) else { continue };
            if let Some(first) = rest.components().next() {
                names.insert(base.join(first).to_string_lossy().into_owned());
            }
        }
        let mut out: Vec<String> = names.into_iter().collect();
        out.sort_by(|a, b| popamp_core::text::natural_cmp(a, b));
        out
    }

    /// Whether `v` is still there to show: a playlist, or a folder that
    /// holds tracks of the library, may have gone.
    fn exists(&self, v: &View) -> bool {
        match v {
            View::All => true,
            View::Branch(_) => false,
            View::Smart(n) => *n < self.smart.len(),
            View::Plain(n) => *n < self.plain.len(),
            View::Folder(f) => {
                self.lib.folders.contains(f)
                    || self.lib.folders.iter().any(|r| is_under(f, r)) && self.lib.tracks.iter().any(|t| is_under(&t.path, f))
            }
        }
    }

    /// The tree as the settings keep it: the open nodes, then the view.
    /// Nodes are `b:` and a branch, or `f:` and a folder, split by tabs; a
    /// playlist's view goes by its name, so it is found again when its
    /// place changed.
    pub fn tree_state(&self) -> (String, String) {
        let mut open: Vec<String> = self
            .open
            .iter()
            .map(|n| match n {
                Node::Branch(b) => format!("b:{}", b.key()),
                Node::Folder(f) => format!("f:{}", escape_field(f)),
            })
            .collect();
        open.sort();
        let view = match &self.view {
            View::All | View::Branch(_) => "all".to_string(),
            View::Smart(n) => format!("smart:{}", escape_field(&self.smart[*n].name)),
            View::Plain(n) => format!("plain:{}", escape_field(&self.plain[*n].name)),
            View::Folder(f) => format!("folder:{}", escape_field(f)),
        };
        (open.join("\t"), view)
    }

    /// Opens the nodes and shows the view `tree_state` gave; what is no
    /// longer there is left out, a view that is gone is all music.
    pub fn restore_tree_state(&mut self, open: &str, view: &str) {
        self.open = open
            .split('\t')
            .filter_map(|item| match item.split_once(':')? {
                ("b", key) => Branch::ALL.into_iter().find(|b| b.key() == key).map(Node::Branch),
                ("f", folder) => Some(Node::Folder(unescape_field(folder))),
                _ => None,
            })
            .collect();
        let named = |names: Vec<&String>, want: &str| names.iter().position(|n| **n == unescape_field(want));
        let v = match view.split_once(':') {
            Some(("smart", name)) => named(self.smart.iter().map(|l| &l.name).collect(), name).map(View::Smart),
            Some(("plain", name)) => named(self.plain.iter().map(|l| &l.name).collect(), name).map(View::Plain),
            Some(("folder", f)) => Some(View::Folder(unescape_field(f))),
            _ => None,
        };
        self.view = View::All;
        self.grow_tree();
        self.open_view(v.filter(|v| self.exists(v)).unwrap_or(View::All));
    }

    /// Opens a node of the tree that is shut, shuts one that is open.
    fn fold(&mut self, node: Node) {
        if !self.open.remove(&node) {
            self.open.insert(node);
        }
        self.grow_tree();
    }

    fn view_label(&self, v: &View) -> String {
        match v {
            View::All => "All music".into(),
            View::Branch(b) => b.label().into(),
            View::Smart(n) => format!("{} ({})", self.smart[*n].name, self.counts.0.get(*n).unwrap_or(&0)),
            View::Plain(n) => format!("{} ({})", self.plain[*n].name, self.counts.1.get(*n).unwrap_or(&0)),
            View::Folder(f) => folder_label(f),
        }
    }

    fn smart_view(&self) -> Option<&SmartList> {
        self.smart_index().and_then(|n| self.smart.get(n))
    }

    /// The smart playlist on screen, by its place in the lists.
    pub fn smart_index(&self) -> Option<usize> {
        match self.view {
            View::Smart(n) => Some(n),
            _ => None,
        }
    }

    fn has_panes(&self) -> bool {
        !matches!(self.view, View::Smart(_) | View::Plain(_))
    }

    /// The plain playlist on screen, by its place in the lists.
    pub fn plain_index(&self) -> Option<usize> {
        match self.view {
            View::Plain(n) => Some(n),
            _ => None,
        }
    }

    /// The library or a playlist changed: what each list has is counted
    /// again, and what is on screen found again.
    fn changed(&mut self) {
        self.recount();
        self.refresh();
    }

    /// Counts the tracks of each playlist again, after the library, a
    /// list, a rating or a play changed.
    pub fn recount(&mut self) {
        let now = popamp_core::history::now();
        self.counts.0 = self.smart.iter().map(|l| l.tracks(&self.lib, "", now, self.seed).len()).collect();
        self.counts.1 = self.plain.iter().map(|l| l.tracks(&self.lib, "").len()).collect();
    }

    fn parts(&self, at: Place) -> Parts {
        Parts::new(at, self.has_panes())
    }

    /// Finds the panes' rows and the table's tracks again, after the
    /// library, the view, a pick, the search or the order changed. A pick
    /// that is no longer there goes. So does the selection, unless the
    /// table has the same rows: else they are other tracks now.
    fn refresh(&mut self) {
        self.grow_tree();
        if !self.exists(&self.view) {
            self.view = View::All;
        }
        let folder = match &self.view {
            View::Folder(f) => Some(f.as_str()),
            _ => None,
        };
        let within = Filter { folder, query: &self.query, ..Filter::default() };
        let keep = |pick: &mut Option<String>, rows: &[GroupRow]| {
            if pick.as_ref().is_some_and(|p| !rows.iter().any(|g| &g.name == p)) {
                *pick = None;
            }
        };
        let before = std::mem::take(&mut self.shown);
        let own = match self.view {
            View::Smart(n) => Some(self.smart[n].tracks(&self.lib, &self.query, popamp_core::history::now(), self.seed)),
            View::Plain(n) => Some(self.plain[n].tracks(&self.lib, &self.query)),
            _ => None,
        };
        if let Some(shown) = own {
            self.shown = shown;
            (self.artists, self.albums, self.artist, self.album) = (Vec::new(), Vec::new(), None, None);
        } else {
            self.artists = self.lib.groups(Group::Artist, &within);
            keep(&mut self.artist, &self.artists);
            self.albums = self.lib.groups(Group::Album, &Filter { artist: self.artist.as_deref(), ..within.clone() });
            keep(&mut self.album, &self.albums);
            let filter = Filter { artist: self.artist.as_deref(), album: self.album.as_deref(), ..within };
            self.shown = self.lib.find(&filter, self.sort.0, self.sort.1);
        }
        if self.shown == before {
            return;
        }
        self.selected = vec![false; self.shown.len()];
        (self.cursor, self.anchor) = (0, 0);
        self.scroll[List::Tracks as usize] = 0;
    }

    /// The tracks selected in the table; none when no row is.
    pub fn selected_paths(&self) -> Vec<String> {
        let rows = self.shown.iter().zip(&self.selected).filter(|(_, &on)| on);
        rows.map(|(&i, _)| self.lib.tracks[i].path.clone()).collect()
    }

    /// The paths of the picked tracks, in table order.
    pub fn picked(&self) -> Vec<String> {
        let all = !self.selected.contains(&true);
        let rows = self.shown.iter().zip(&self.selected).filter(|(_, &on)| all || on);
        rows.map(|(&i, _)| self.lib.tracks[i].path.clone()).collect()
    }

    /// The folder of the library the view on screen shows, when it shows
    /// one; not a folder inside it.
    pub fn folder(&self) -> Option<&str> {
        match &self.view {
            View::Folder(f) if self.lib.folders.contains(f) => Some(f),
            _ => None,
        }
    }

    /// What the window's small display says: the shown tracks and how
    /// long they play.
    pub fn status(&self) -> String {
        if let Some(s) = &self.scan {
            return format!("Scanning: {}", s.done);
        }
        let n = self.shown.len();
        let ms: u64 = self.shown.iter().filter_map(|&i| self.lib.tracks[i].length_ms).sum();
        let tracks = if n == 1 { "1 track".to_string() } else { format!("{n} tracks") };
        if ms == 0 {
            tracks
        } else {
            format!("{tracks} [{}]", format_duration_ms(ms))
        }
    }

    // --- the cover ------------------------------------------------------------

    /// The track at the cursor, when it is picked.
    fn focused(&self) -> Option<&Track> {
        self.selected.get(self.cursor).filter(|&&on| on).map(|_| &self.lib.tracks[self.shown[self.cursor]])
    }

    /// The tracks a picture let go at `x`, `y` is the cover of: those
    /// picked, when it falls on the cover or on a picked row; the one
    /// row it falls on else, which is then picked.
    pub fn cover_drop_targets(&mut self, at: Place, x: i32, y: i32) -> Vec<String> {
        let p = self.parts(at);
        if let Some(n) = self.row_at(&p, List::Tracks, x, y) {
            if !self.selected[n] {
                self.select_only(n);
            }
            return self.selected_paths();
        }
        if p.cover.is_some_and(|c| c.contains(x, y)) {
            return self.selected_paths();
        }
        Vec::new()
    }

    /// What the cover panel says of the track at the cursor: its title,
    /// artist, album and year, and its format and bit rate.
    fn about_lines(&self) -> Option<[String; ABOUT_LINES]> {
        let t = self.focused()?;
        let album = match (t.album.as_str(), t.year.as_str()) {
            (album, "") => album.to_string(),
            ("", year) => year.to_string(),
            (album, year) => format!("{album} ({year})"),
        };
        let file = popamp_core::cue::split_location(&t.path).map_or(t.path.as_str(), |(sheet, _)| sheet);
        let kind = Path::new(file).extension().map(|e| e.to_string_lossy().to_uppercase()).unwrap_or_default();
        let format = if t.bitrate_kbps == 0 { kind } else { format!("{kind}, {} kbps", t.bitrate_kbps) };
        Some([t.title.clone(), t.artist.clone(), album, format])
    }

    /// The cover of the track at the cursor, shrunk to fit `size` screen
    /// pixels; read from the file once per track.
    fn cover_sized(&self, size: (i32, i32)) -> Option<Bitmap> {
        let path = self.focused()?.path.clone();
        let mut held = self.shown_cover.borrow_mut();
        let shown = match &mut *held {
            Some(s) if s.path == path => s,
            slot => {
                let file = popamp_core::cue::split_location(&path).map_or(path.as_str(), |(sheet, _)| sheet);
                let most = crate::pen::largest(120);
                let art = crate::fileinfo::cover_of(Path::new(file)).map(|a| crate::widgets::fit(&a, most, most));
                slot.insert(Shown { path, art, sized: None })
            }
        };
        let art = shown.art.as_ref()?;
        match &shown.sized {
            Some((at, b)) if *at == size => Some(b.clone()),
            _ => {
                let b = crate::widgets::fit(art, size.0, size.1);
                shown.sized = Some((size, b.clone()));
                Some(b)
            }
        }
    }

    /// The cover in its frame and what is known of the track under it.
    fn draw_cover(&self, pen: &mut Pen, p: &Parts, c: &PlColors) {
        let (Some(cover), Some(about)) = (p.cover, p.about) else { return };
        pen.b.fill(cover.x, cover.y - 2, cover.w, 1, pen.line);
        let frame = rect(cover.x + 2, cover.y + 2, cover.w - 4, cover.h - 4);
        match self.cover_sized(pen.b.screen_size(frame)) {
            Some(art) => pen.b.picture(&art, frame),
            None => {
                let (x, y, w, h) = (frame.x, frame.y, frame.w, frame.h);
                for (lx, ly, lw, lh) in [(x, y, w, 1), (x, y + h - 1, w, 1), (x, y, 1, h), (x + w - 1, y, 1, h)] {
                    pen.b.fill(lx, ly, lw, lh, pen.line);
                }
                let note = if self.focused().is_some() { "No cover" } else { "" };
                pen.centre(frame, note, pen.dim);
            }
        }
        let Some(lines) = self.about_lines() else { return };
        for (i, text) in lines.iter().enumerate() {
            let row = rect(about.x, about.y + i as i32 * p.row_h, about.w, p.row_h);
            pen.left(row, text, if i == 0 { c.normal } else { pen.dim });
        }
    }

    // --- the panes ------------------------------------------------------------

    fn pane(&self, list: List) -> (&[GroupRow], Option<&String>) {
        match list {
            List::Artists => (&self.artists, self.artist.as_ref()),
            _ => (&self.albums, self.album.as_ref()),
        }
    }

    /// The row of a pane its pick is on; 0 is all of them.
    fn picked_row(&self, list: List) -> usize {
        let (rows, pick) = self.pane(list);
        pick.and_then(|name| rows.iter().position(|g| &g.name == name)).map_or(0, |i| i + 1)
    }

    /// Picks row `n` of a pane; row 0 is all of them. A new artist shows
    /// all their albums.
    fn pick(&mut self, list: List, n: usize) {
        self.focus = list;
        let name = n.checked_sub(1).and_then(|i| self.pane(list).0.get(i)).map(|g| g.name.clone());
        if list == List::Artists {
            if self.artist != name {
                self.album = None;
                self.scroll[List::Albums as usize] = 0;
            }
            self.artist = name;
        } else {
            self.album = name;
        }
        self.refresh();
    }

    /// The rows a list has.
    fn len(&self, list: List) -> usize {
        match list {
            List::Views => self.tree.len(),
            List::Artists => self.artists.len() + 1,
            List::Albums => self.albums.len() + 1,
            List::Tracks => self.shown.len(),
        }
    }

    // --- scrolling ----------------------------------------------------------

    /// The first visible row of `list` and how many it shows.
    fn window(&self, p: &Parts, list: List) -> (usize, usize) {
        let page = p.rows(list).map_or(1, |r| p.page(r));
        (self.scroll[list as usize].min(self.len(list).saturating_sub(page)), page)
    }

    /// Where the thumb of `list`'s bar is: the skin's thumb always, at
    /// the top when all rows show; the player's own, as long as the share
    /// of the rows in sight, only when some are out of it.
    fn thumb(&self, p: &Parts, list: List) -> Option<Rect> {
        let bar = p.bar(list)?;
        let (scroll, page) = self.window(p, list);
        let len = self.len(list);
        let h = match p.thumb_h {
            Some(h) => h,
            None if len > page => (bar.h * page as i32 / len as i32).max(8),
            None => return None,
        }
        .min(bar.h);
        let y = bar.y + if len > page { (bar.h - h) * scroll as i32 / (len - page) as i32 } else { 0 };
        Some(rect(bar.x, y, bar.w, h))
    }

    /// Scrolls `list` so its thumb's top is at `top`.
    fn scroll_thumb_to(&mut self, p: &Parts, list: List, top: i32) {
        let (Some(bar), Some(thumb)) = (p.bar(list), self.thumb(p, list)) else { return };
        let max = self.len(list).saturating_sub(self.window(p, list).1);
        let track = (bar.h - thumb.h).max(1);
        let t = (top - bar.y).clamp(0, track) as usize;
        self.scroll[list as usize] = (2 * t * max + track as usize) / (2 * track as usize);
    }

    /// The thumbs the skin draws, each with whether the mouse holds it.
    pub fn thumbs(&self, at: Place) -> Vec<(Rect, bool)> {
        if at.thumb.is_none() {
            return Vec::new();
        }
        let p = self.parts(at);
        LISTS.into_iter().filter_map(|l| Some((self.thumb(&p, l)?, self.dragging.is_some_and(|d| d.0 == l)))).collect()
    }

    fn show_row(&mut self, p: &Parts, list: List, row: usize) {
        let (scroll, page) = self.window(p, list);
        self.scroll[list as usize] = if row < scroll {
            row
        } else if row >= scroll + page {
            row + 1 - page
        } else {
            scroll
        };
    }

    /// The row of `list` under `x`, `y`, if there is one.
    fn row_at(&self, p: &Parts, list: List, x: i32, y: i32) -> Option<usize> {
        let rows = p.rows(list).filter(|r| r.contains(x, y))?;
        let n = self.window(p, list).0 + p.row(rows, y);
        (n < self.len(list)).then_some(n)
    }

    // --- columns --------------------------------------------------------------

    fn cell(&self, t: &Track, col: Column) -> String {
        let known = |n: u64| if n == 0 { String::new() } else { n.to_string() };
        let date = |secs: u64| match jiff::Timestamp::from_second(secs as i64) {
            Ok(at) if secs > 0 => at.to_zoned(self.tz.clone()).strftime("%Y-%m-%d").to_string(),
            _ => String::new(),
        };
        match col {
            Column::Artist => t.artist.clone(),
            Column::Title => t.title.clone(),
            Column::Album => t.album.clone(),
            Column::Time => t.length_ms.map(format_duration_ms).unwrap_or_default(),
            Column::Genre => t.genre.clone(),
            Column::Year => t.year.clone(),
            Column::Rating => stars(self.lib.stats.get(&t.path).rating),
            Column::Number => known(t.number.into()),
            Column::File => popamp_core::library::file_name(&t.path).to_string(),
            Column::Comment => t.comment.clone(),
            Column::Bitrate => known(t.bitrate_kbps.into()),
            Column::Plays => known(self.lib.stats.get(&t.path).plays.into()),
            Column::LastPlayed => date(self.lib.stats.get(&t.path).last_played),
            Column::Added => date(t.added),
        }
    }

    /// A pane's columns: the name, then two numbers when there is room.
    fn pane_columns(list: List, rows: Rect, px: i32) -> Vec<(&'static str, i32, i32)> {
        let (name, second) = if list == List::Artists { ("Artist", "Albums") } else { ("Album", "Year") };
        let cols = [(name, Width::Share(1)), (second, Width::Px(4)), ("Tracks", Width::Px(4))];
        lay(&cols[..if rows.w >= px * 14 { 3 } else { 1 }], rows, px)
    }

    /// The text of a pane's row `n` in each column.
    fn pane_row(&self, list: List, n: usize) -> [String; 3] {
        let rows = self.pane(list).0;
        let Some(g) = n.checked_sub(1).map(|i| &rows[i]) else {
            let (what, albums) = if list == List::Artists {
                ("artists", rows.iter().map(|g| g.albums).sum::<usize>().to_string())
            } else {
                ("albums", String::new())
            };
            return [format!("All ({} {what})", rows.len()), albums, rows.iter().map(|g| g.tracks).sum::<usize>().to_string()];
        };
        let name = match (g.name.is_empty(), list) {
            (false, _) => g.name.clone(),
            (true, List::Artists) => "(no artist)".into(),
            (true, _) => "(no album)".into(),
        };
        let second = if list == List::Artists { g.albums.to_string() } else { g.year.clone() };
        [name, second, g.tracks.to_string()]
    }

    // --- drawing --------------------------------------------------------------

    /// Draws the pane at `at`. `playing` is the location of the track
    /// that plays. The thumbs of the bars are left to the skin when it
    /// gives one (see `thumbs`).
    pub fn draw(&self, b: &mut Canvas, font: &mut Font, at: Place, c: &PlColors, active: bool, playing: Option<&str>) {
        let (r, px) = (at.r, at.px);
        let p = self.parts(at);
        let (dim, line) = (mix(c.normal, c.normal_bg, 0.45), mix(c.normal, c.normal_bg, 0.7));
        let mut pen = Pen { b, font, px: px as u32, dim, line };
        pen.b.fill(r.x, r.y, r.w, r.h, c.normal_bg);
        pen.b.fill(p.views.x + p.views.w + p.bar_w + 1, p.views.y, 1, p.views.h, line);

        self.draw_tree(&mut pen, &p, c);

        self.draw_cover(&mut pen, &p, c);
        self.draw_search(&mut pen, p.search, c, active);
        if let Some(panes) = p.panes {
            for (list, t) in [(List::Artists, panes[0]), (List::Albums, panes[1])] {
                self.draw_pane(&mut pen, &p, list, t, c, active);
            }
        }
        self.draw_table(&mut pen, &p, c, active, playing);
        for list in LISTS {
            self.draw_bar(&mut pen, &p, list, c, at.thumb.is_none());
        }

        for (i, (label, _)) in BUTTONS.iter().enumerate() {
            let k = p.buttons[i];
            let face = rect(k.x, k.y + 2, k.w, k.h - 2);
            pen.b.fill(face.x, face.y, face.w, face.h, if self.pressed == Some(i) { c.selected_bg } else { line });
            if self.pressed != Some(i) {
                pen.b.fill(face.x + 1, face.y + 1, face.w - 2, face.h - 2, c.normal_bg);
            }
            pen.centre(face, label, c.normal);
        }
        if p.status.w >= px * 4 {
            pen.right(rect(p.status.x, p.status.y + 2, p.status.w, p.status.h - 2), &self.status(), dim);
        }
    }

    /// The rows of the views tree, each indented by its depth, a node
    /// after the mark that opens and shuts it.
    fn draw_tree(&self, pen: &mut Pen, p: &Parts, c: &PlColors) {
        let (scroll, page) = self.window(p, List::Views);
        for (n, r) in self.tree.iter().enumerate().skip(scroll).take(page) {
            let row = rect(p.views.x, p.views.y + (n - scroll) as i32 * p.row_h, p.views.w, p.row_h);
            let target = matches!(r.view, View::Plain(i) if self.dropping() && self.drop_on == Some(i));
            if r.view == self.view || target {
                pen.b.fill(row.x, row.y, row.w, row.h, c.selected_bg);
            }
            let color = match r.view {
                View::Branch(_) => pen.dim,
                _ if r.view == self.view => c.current,
                _ => c.normal,
            };
            let mark = Self::mark(p, row, r.depth);
            if let Some(open) = r.fold {
                Self::draw_mark(pen, mark, open, color);
            }
            let text = rect(mark.x + mark.w - 2, row.y, (row.x + row.w - mark.x - mark.w + 2).max(0), row.h);
            pen.left(text, &self.view_label(&r.view), color);
        }
    }

    /// Where the mark of a tree row at `depth` is; the label follows it.
    fn mark(p: &Parts, row: Rect, depth: u8) -> Rect {
        let w = p.row_h - 3;
        rect(row.x + 2 + depth as i32 * w, row.y, w, row.h)
    }

    /// A small triangle: pointing right for a node that is shut, down for
    /// an open one.
    fn draw_mark(pen: &mut Pen, at: Rect, open: bool, color: u32) {
        let (cx, cy) = (at.x + at.w / 2, at.y + at.h / 2);
        for k in 0..3 {
            if open {
                pen.b.fill(cx - 2 + k, cy - 1 + k, 5 - 2 * k, 1, color);
            } else {
                pen.b.fill(cx - 1 + k, cy - 2 + k, 1, 5 - 2 * k, color);
            }
        }
    }

    fn draw_search(&self, pen: &mut Pen, s: Rect, c: &PlColors, active: bool) {
        pen.b.fill(s.x, s.y, s.w, s.h, if active { c.normal } else { pen.line });
        pen.b.fill(s.x + 1, s.y + 1, s.w - 2, s.h - 2, c.normal_bg);
        let inner = rect(s.x + 1, s.y + 1, s.w - 2, s.h - 2);
        if self.query.is_empty() {
            pen.left(inner, "Search: type here", pen.dim);
        } else {
            pen.left(inner, &self.query, c.normal);
        }
        if active {
            let caret = inner.x + 2 + pen.width(&self.query).min(inner.w - 4);
            pen.b.fill(caret, inner.y + 1, 1, inner.h - 2, c.normal);
        }
    }

    /// Draws the header row of a list: the names of `cols`, grey, and a
    /// line under them.
    fn draw_header<T>(pen: &mut Pen, header: Rect, cols: &[(T, i32, i32)], label: impl Fn(&T) -> (&'static str, bool)) {
        for (col, x, w) in cols {
            let (text, right) = label(col);
            let cell = rect(*x, header.y, *w, header.h);
            if right {
                pen.right(cell, text, pen.dim);
            } else {
                pen.left(cell, text, pen.dim);
            }
        }
        pen.b.fill(header.x, header.y + header.h - 1, header.w, 1, pen.line);
    }

    fn draw_pane(&self, pen: &mut Pen, p: &Parts, list: List, t: Table, c: &PlColors, active: bool) {
        let cols = Self::pane_columns(list, t.rows, pen.px as i32);
        Self::draw_header(pen, t.header, &cols, |name| (*name, *name != "Artist" && *name != "Album"));
        let (scroll, page) = self.window(p, list);
        let picked = self.picked_row(list);
        for n in scroll..self.len(list).min(scroll + page) {
            let row = rect(t.rows.x, t.rows.y + (n - scroll) as i32 * p.row_h, t.rows.w, p.row_h);
            if n == picked {
                let shade = if active && self.focus == list { c.selected_bg } else { mix(c.selected_bg, c.normal_bg, 0.5) };
                pen.b.fill(row.x, row.y, row.w, row.h, shade);
            }
            for (i, (text, &(_, x, w))) in self.pane_row(list, n).iter().zip(&cols).enumerate() {
                let cell = rect(x, row.y, w, row.h);
                if i == 0 {
                    pen.left(cell, text, c.normal);
                } else {
                    pen.right(cell, text, pen.dim);
                }
            }
        }
        pen.b.fill(t.rows.x + t.rows.w + p.bar_w + 1, t.header.y, 1, t.header.h + t.rows.h, pen.line);
    }

    /// A small arrow after the name of the column the table is sorted by:
    /// up for A to Z.
    fn draw_sort_arrow(pen: &mut Pen, header: Rect, (col, x, w): (Column, i32, i32), reverse: bool) {
        let ax = if col.right() { x + 1 } else { (x + 4 + pen.width(col.label())).min(x + w - 6) };
        let mid = header.y + header.h / 2;
        for k in 0..3 {
            let y = if reverse { mid + 1 - k } else { mid - 1 + k };
            pen.b.fill(ax + 2 - k, y, 1 + 2 * k, 1, pen.dim);
        }
    }

    /// The groove of a list's scroll bar, and the player's own thumb in it
    /// when `own_thumb`.
    fn draw_bar(&self, pen: &mut Pen, p: &Parts, list: List, c: &PlColors, own_thumb: bool) {
        let Some(bar) = p.bar(list).filter(|b| b.h > 0) else { return };
        pen.b.fill(bar.x, bar.y, bar.w, bar.h, mix(c.normal, c.normal_bg, 0.88));
        if let Some(t) = self.thumb(p, list).filter(|_| own_thumb) {
            let held = self.dragging.is_some_and(|d| d.0 == list);
            pen.b.fill(t.x + 1, t.y, t.w - 2, t.h, if held { pen.dim } else { pen.line });
        }
    }

    fn draw_table(&self, pen: &mut Pen, p: &Parts, c: &PlColors, active: bool, playing: Option<&str>) {
        let t = p.table;
        let columns = self.columns.lay(t.rows, pen.px as i32);
        Self::draw_header(pen, t.header, &columns, |col| (col.label(), col.right()));
        if self.has_panes() {
            if let Some(&(col, x, w)) = columns.iter().find(|c| c.0.sort() == self.sort.0) {
                Self::draw_sort_arrow(pen, t.header, (col, x, w), self.sort.1);
            }
        }
        if self.lib.folders.is_empty() {
            pen.centre(rect(t.rows.x, t.rows.y + p.row_h, t.rows.w, p.row_h), "The library is empty.", pen.dim);
            pen.centre(rect(t.rows.x, t.rows.y + 2 * p.row_h, t.rows.w, p.row_h), "Add a folder with music.", pen.dim);
            return;
        }
        let (scroll, page) = self.window(p, List::Tracks);
        for n in scroll..self.shown.len().min(scroll + page) {
            let track = &self.lib.tracks[self.shown[n]];
            let row = rect(t.rows.x, t.rows.y + (n - scroll) as i32 * p.row_h, t.rows.w, p.row_h);
            if self.selected[n] {
                pen.b.fill(row.x, row.y, row.w, row.h, c.selected_bg);
            }
            let color = if playing == Some(track.path.as_str()) { c.current } else { c.normal };
            for &(col, x, w) in &columns {
                let cell = rect(x, row.y, w, row.h);
                if col.right() {
                    pen.right(cell, &self.cell(track, col), color);
                } else {
                    pen.left(cell, &self.cell(track, col), color);
                }
            }
            if n == self.cursor && active && self.focus == List::Tracks {
                for x in (row.x..row.x + row.w).step_by(2) {
                    pen.b.set(x, row.y, color);
                    pen.b.set(x, row.y + row.h - 1, color);
                }
            }
        }
    }

    // --- mouse ----------------------------------------------------------------

    /// The right button went down: the menu is for the row under it.
    pub fn press_right(&mut self, at: Place, x: i32, y: i32) {
        let p = self.parts(at);
        self.menu_for = match () {
            _ if p.views.contains(x, y) => Menu::Lists,
            _ if p.table.header.contains(x, y) => Menu::Columns,
            _ => Menu::Tracks,
        };
        if let Some(n) = self.row_at(&p, List::Views, x, y) {
            self.open_view(self.tree[n].view.clone());
        }
        if let Some(n) = self.row_at(&p, List::Tracks, x, y).filter(|&n| !self.selected[n]) {
            self.select_only(n);
        }
    }

    /// Shows view `v`; a heading is no view.
    fn open_view(&mut self, v: View) {
        if matches!(v, View::Branch(_)) || v == self.view {
            return;
        }
        (self.view, self.artist, self.album) = (v, None, None);
        self.seed = fastrand::u64(..);
        self.scroll[List::Artists as usize] = 0;
        self.scroll[List::Albums as usize] = 0;
        self.focus = if self.has_panes() { List::Artists } else { List::Tracks };
        self.refresh();
    }

    fn select_only(&mut self, n: usize) {
        self.selected.fill(false);
        self.selected[n] = true;
        (self.cursor, self.anchor) = (n, n);
    }

    fn select_range(&mut self, to: usize) {
        let (a, b) = (self.anchor.min(to), self.anchor.max(to));
        for (i, on) in self.selected.iter_mut().enumerate() {
            *on = i >= a && i <= b;
        }
        self.cursor = to;
    }

    /// The left button went down at `x`, `y`.
    #[allow(clippy::too_many_arguments)]
    pub fn press(&mut self, at: Place, x: i32, y: i32, shift: bool, ctrl: bool, double: bool) -> Reaction {
        self.carry = None;
        let p = self.parts(at);
        if let Some(i) = p.buttons.iter().position(|k| k.contains(x, y)) {
            self.pressed = Some(i);
            return Reaction { capture: true, run: None };
        }
        if let Some(list) = LISTS.into_iter().find(|&l| p.bar(l).is_some_and(|b| b.contains(x, y))) {
            return self.press_bar(&p, list, y);
        }
        if let Some(n) = self.row_at(&p, List::Views, x, y) {
            self.press_tree(&p, n, x, double);
            return Reaction::default();
        }
        for list in [List::Artists, List::Albums] {
            if let Some(n) = self.row_at(&p, list, x, y) {
                self.pick(list, n);
                return if double { Reaction::run(Command::LibraryOpen) } else { Reaction::default() };
            }
        }
        self.press_table(&p, at.px, (x, y), shift, ctrl, double)
    }

    /// A press on row `n` of the views tree at `x`: a branch, the mark of a
    /// node or a double click opens or shuts it; any other row is shown.
    fn press_tree(&mut self, p: &Parts, n: usize, x: i32, double: bool) {
        let r = self.tree[n].clone();
        let rows = p.rows(List::Views).unwrap_or(p.views);
        let mark = Self::mark(p, rows, r.depth);
        let on_mark = x >= mark.x && x < mark.x + mark.w;
        if !matches!(r.view, View::Branch(_)) && !(on_mark && r.fold.is_some()) {
            self.open_view(r.view.clone());
        }
        if r.fold.is_some() && (matches!(r.view, View::Branch(_)) || on_mark || double) {
            self.fold(Node::of(&r.view).expect("a node that folds"));
        }
    }

    /// A press on a list's scroll bar takes hold of its thumb, which comes
    /// under the mouse when it was not.
    fn press_bar(&mut self, p: &Parts, list: List, y: i32) -> Reaction {
        let Some(t) = self.thumb(p, list) else { return Reaction::default() };
        let grab = if y >= t.y && y < t.y + t.h { y - t.y } else { t.h / 2 };
        self.dragging = Some((list, grab));
        self.scroll_thumb_to(p, list, y - grab);
        Reaction { capture: true, run: None }
    }

    /// The mouse moved to `x`, `y` with the button down: a held thumb
    /// follows it, held rows start being carried. True when that changed
    /// what the window shows.
    pub fn drag(&mut self, at: Place, x: i32, y: i32) -> bool {
        if let Some(i) = self.resizing {
            let p = self.parts(at);
            let shown = self.columns.lay(p.table.rows, at.px);
            self.columns.resize(&shown, i, x, at.px);
            return true;
        }
        if let Some((list, grab)) = self.dragging {
            self.scroll_thumb_to(&self.parts(at), list, y - grab);
            return true;
        }
        match self.carry {
            Some(Carry::Pressed { x: px, y: py, .. }) => {
                let moved = (x - px).abs().max((y - py).abs()) >= CARRY_SLACK;
                if moved {
                    self.carry = Some(Carry::Moving);
                    self.drop_on = self.plain_at(at, x, y);
                }
                moved
            }
            Some(Carry::Moving) => {
                let now = self.plain_at(at, x, y);
                std::mem::replace(&mut self.drop_on, now) != now
            }
            None => false,
        }
    }

    /// The plain playlist whose row is at `x`, `y`.
    fn plain_at(&self, at: Place, x: i32, y: i32) -> Option<usize> {
        let n = self.row_at(&self.parts(at), List::Views, x, y)?;
        match self.tree[n].view {
            View::Plain(i) => Some(i),
            _ => None,
        }
    }

    /// Whether letting go of the carried tracks puts them into a playlist.
    pub fn dropping(&self) -> bool {
        self.carrying() && self.drop_on.is_some()
    }

    /// The plain playlist the carried tracks were let go on, once.
    pub fn take_drop(&mut self) -> Option<usize> {
        self.drop_on.take()
    }

    /// Whether the picked tracks are being carried by the mouse.
    pub fn carrying(&self) -> bool {
        self.carry == Some(Carry::Moving)
    }

    /// True once after the picked tracks were carried out by the mouse,
    /// for the window layer to put them where they were let go.
    pub fn take_carried(&mut self) -> bool {
        let moving = self.carry == Some(Carry::Moving);
        if moving {
            (self.carry, self.drop_on) = (None, None);
        }
        moving
    }

    /// The border of the header at `x`: the column left of it, by its
    /// place among the shown ones. The table's own edges are none.
    fn border_at(shown: &[(Column, i32, i32)], x: i32) -> Option<usize> {
        let inner = shown.len().saturating_sub(1);
        shown[..inner].iter().position(|&(_, cx, cw)| (x - (cx + cw)).abs() <= BORDER_SLACK)
    }

    /// A press on the table's header: a border between two columns takes
    /// hold, the rest sorts by the column under it, but not a playlist
    /// shown in its own order.
    fn press_header(&mut self, p: &Parts, px: i32, x: i32) -> Reaction {
        let shown = self.columns.lay(p.table.rows, px);
        if let Some(i) = Self::border_at(&shown, x) {
            self.resizing = Some(i);
            return Reaction { capture: true, run: None };
        }
        if let Some(&(col, ..)) = shown.iter().find(|&&(_, cx, cw)| x >= cx && x < cx + cw).filter(|_| self.has_panes()) {
            self.sort = if self.sort.0 == col.sort() { (self.sort.0, !self.sort.1) } else { (col.sort(), false) };
            self.refresh();
        }
        Reaction::default()
    }

    /// A press in the table: on its header, or on its rows, which select.
    fn press_table(&mut self, p: &Parts, px: i32, (x, y): (i32, i32), shift: bool, ctrl: bool, double: bool) -> Reaction {
        if p.table.header.contains(x, y) {
            return self.press_header(p, px, x);
        }
        let Some(n) = self.row_at(p, List::Tracks, x, y) else {
            if p.table.rows.contains(x, y) {
                self.selected.fill(false);
            }
            return Reaction::default();
        };
        self.focus = List::Tracks;
        match (double, shift, ctrl) {
            (true, ..) => {
                self.select_only(n);
                return Reaction::run(Command::LibraryOpen);
            }
            (_, true, _) => self.select_range(n),
            (_, _, true) => {
                self.selected[n] = !self.selected[n];
                (self.cursor, self.anchor) = (n, n);
            }
            _ => {
                // A row already picked keeps the others until the button
                // goes up without carrying them.
                if !self.selected[n] {
                    self.select_only(n);
                }
                self.carry = Some(Carry::Pressed { x, y, row: n });
                return Reaction { capture: true, run: None };
            }
        }
        Reaction::default()
    }

    /// The left button went up: a button pressed and let go over itself
    /// runs; a row clicked without carrying is the only one picked.
    pub fn release(&mut self, at: Place, x: i32, y: i32) -> Option<Command> {
        (self.dragging, self.resizing) = (None, None);
        match self.carry.take() {
            Some(Carry::Pressed { row, .. }) => self.select_only(row),
            Some(Carry::Moving) => {
                self.drop_on = self.plain_at(at, x, y);
                return self.drop_on.map(|_| Command::LibraryDropOnPlaylist);
            }
            None => {}
        }
        let i = self.pressed.take()?;
        self.parts(at).buttons[i].contains(x, y).then_some(BUTTONS[i].1)
    }

    /// Scrolls the list under the mouse by `steps` wheel notches (up is
    /// positive); the table when it is over none.
    pub fn wheel(&mut self, at: Place, x: i32, y: i32, steps: i32) {
        let p = self.parts(at);
        let over = |l: List| [p.rows(l), p.bar(l)].into_iter().flatten().any(|r| r.contains(x, y));
        let under = [List::Views, List::Artists, List::Albums].into_iter().find(|&l| over(l));
        let list = under.unwrap_or(List::Tracks);
        let (scroll, page) = self.window(&p, list);
        let max = self.len(list).saturating_sub(page);
        self.scroll[list as usize] = scroll.saturating_add_signed(-steps as isize * 3).min(max);
    }

    // --- keys -----------------------------------------------------------------

    /// A key while the window has the keyboard: letters go to the search,
    /// Tab goes to the next list (`shift`: the one before), the arrows move
    /// in the list with the focus, Enter plays (`shift`: enqueues). False
    /// for a key the pane has no use for.
    pub fn key(&mut self, at: Place, key: Key, shift: bool) -> (bool, Option<Command>) {
        let p = self.parts(at);
        match key {
            Key::Char(c) if !c.is_control() => {
                self.query.push(c);
                self.refresh();
            }
            Key::Backspace if !self.query.is_empty() => {
                self.query.pop();
                self.refresh();
            }
            Key::Escape if !self.query.is_empty() => {
                self.query.clear();
                self.refresh();
            }
            Key::Enter => return (true, Some(if shift { Command::LibraryOpenOther } else { Command::LibraryOpen })),
            Key::Tab if self.has_panes() => {
                let order = [List::Artists, List::Albums, List::Tracks];
                let at = order.iter().position(|&l| l == self.focus).unwrap_or(2);
                self.focus = order[(at + if shift { 2 } else { 1 }) % 3];
            }
            Key::Up | Key::Down | Key::PageUp | Key::PageDown | Key::Home | Key::End => self.navigate(&p, key, shift),
            _ => return (false, None),
        }
        (true, None)
    }

    fn navigate(&mut self, p: &Parts, key: Key, shift: bool) {
        let list = if self.has_panes() { self.focus } else { List::Tracks };
        let (len, page) = (self.len(list), self.window(p, list).1);
        if len == 0 {
            return;
        }
        let cur = if list == List::Tracks { self.cursor } else { self.picked_row(list) };
        let to = match key {
            Key::Up => cur.saturating_sub(1),
            Key::Down => cur + 1,
            Key::PageUp => cur.saturating_sub(page),
            Key::PageDown => cur + page,
            Key::Home => 0,
            _ => len - 1,
        }
        .min(len - 1);
        match list {
            List::Tracks if shift => self.select_range(to),
            List::Tracks => self.select_only(to),
            _ => self.pick(list, to),
        }
        self.show_row(p, list, to);
    }

    pub fn select_all(&mut self) {
        self.selected.fill(true);
    }

    /// Shows the smart playlist at `n`.
    #[cfg(test)]
    pub fn open_view_for_tests(&mut self, n: usize) {
        self.open_view(View::Smart(n));
    }

    #[cfg(test)]
    pub fn set_query_for_tests(&mut self, query: &str) {
        self.query = query.into();
        self.refresh();
    }
}

/// The commands that rate, by their stars.
pub const RATE: [(u8, Command); 6] = [
    (0, Command::LibraryRate0),
    (1, Command::LibraryRate1),
    (2, Command::LibraryRate2),
    (3, Command::LibraryRate3),
    (4, Command::LibraryRate4),
    (5, Command::LibraryRate5),
];

fn rate_label(n: u8) -> String {
    Rating::new(n).map(stars).filter(|s| !s.is_empty()).unwrap_or_else(|| "No rating".into())
}

/// A rating as filled and hollow stars out of five; nothing unrated.
fn stars(r: Rating) -> String {
    if r == Rating::NONE {
        return String::new();
    }
    let n = r.stars() as usize;
    "★".repeat(n) + &"☆".repeat(Rating::MAX as usize - n)
}

/// Text drawing for the pane: one font and size, fitted to its cell.
struct Pen<'a, 'b> {
    b: &'a mut Canvas<'b>,
    font: &'a mut Font,
    px: u32,
    /// Grey text and lines.
    dim: u32,
    line: u32,
}

impl Pen<'_, '_> {
    fn width(&mut self, text: &str) -> i32 {
        self.b.text_width(self.font, text, self.px)
    }

    fn fitted(&mut self, text: &str, w: i32) -> String {
        let s = self.b.scale;
        self.font.fit(text, s.px(self.px as i32) as u32, s.px(w.max(0)))
    }

    fn at(&mut self, cell: Rect, x: i32, text: &str, color: u32) {
        let y = cell.y + (cell.h - self.px as i32 - 1) / 2;
        self.b.text(self.font, x, y, text, self.px, color, cell);
    }

    fn left(&mut self, cell: Rect, text: &str, color: u32) {
        let t = self.fitted(text, cell.w - 4);
        self.at(cell, cell.x + 2, &t, color);
    }

    fn right(&mut self, cell: Rect, text: &str, color: u32) {
        let t = self.fitted(text, cell.w - 4);
        let w = self.width(&t);
        self.at(cell, cell.x + cell.w - 2 - w, &t, color);
    }

    fn centre(&mut self, cell: Rect, text: &str, color: u32) {
        let t = self.fitted(text, cell.w - 2);
        let w = self.width(&t);
        self.at(cell, cell.x + (cell.w - w) / 2, &t, color);
    }
}

/// Writes `picture` into each of `paths` as its front cover, in place of
/// the old one. Returns the files written and how many could not be.
fn write_covers(picture: &popamp_audio::cover::Picture, paths: &[String]) -> (Vec<String>, usize) {
    let changes = popamp_audio::tags::Changes { fields: Vec::new(), cover: popamp_audio::tags::CoverChange::Set(picture.clone()) };
    let (mut done, mut failed) = (Vec::new(), 0);
    for path in paths {
        match popamp_audio::tags::write(Path::new(path), &changes) {
            Ok(()) => done.push(path.clone()),
            Err(e) => {
                log::warn!("cannot write a cover into {path}: {e}");
                failed += 1;
            }
        }
    }
    (done, failed)
}

// --- the library in the player ------------------------------------------------

/// Looks through `folders` for audio files. A file that is as `known` has
/// it keeps what was read from it before; the tags of the others are read.
fn scan(
    id: u64,
    folders: Vec<String>,
    known: HashMap<String, Track>,
    sources: &Sources,
    cx: &Context,
    stop: &AtomicBool,
    report: &dyn Fn(LibEvent) -> bool,
) {
    let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs());
    let mut done = 0;
    for folder in folders {
        let files = match sources.of(&folder).expand(&folder, cx) {
            Ok(Some(items)) => items,
            Ok(None) => Vec::new(),
            Err(e) => {
                log::warn!("library: cannot read {folder}: {e}");
                // What the library had there stays until the folder reads again.
                continue;
            }
        };
        let mut found = Vec::with_capacity(files.len());
        for item in files {
            if stop.load(Ordering::Relaxed) {
                return;
            }
            let path = item.location;
            // A track of a CUE sheet changes with its sheet.
            let file = popamp_core::cue::split_location(&path).map_or(path.as_str(), |(sheet, _)| sheet);
            let Some(stamp) = Stamp::of(std::path::Path::new(file)) else { continue };
            let track = match known.get(&path) {
                Some(old) if old.stamp == stamp => old.clone(),
                old => {
                    let meta = sources.of(&path).describe(&path).and_then(Result::ok).unwrap_or_default();
                    Track::new(path, stamp, old.map_or(now, |o| o.added), &meta)
                }
            };
            found.push(track);
            done += 1;
            if done % 20 == 0 && !report(LibEvent::Progress(id, done)) {
                return;
            }
        }
        if !report(LibEvent::Folder(id, folder, found)) {
            return;
        }
    }
    report(LibEvent::Done(id));
}

impl Model {
    /// Looks through the library's folders again, in the background; a
    /// scan under way is given up for the new one.
    pub fn library_rescan(&mut self) {
        let pane = &mut self.library;
        if let Some(old) = pane.scan.take() {
            old.cancel.store(true, Ordering::Relaxed);
        }
        if pane.lib.folders.is_empty() {
            return;
        }
        pane.scans += 1;
        let (id, cancel) = (pane.scans, Arc::new(AtomicBool::new(false)));
        let folders = pane.lib.folders.clone();
        let known: HashMap<String, Track> = pane.lib.tracks.iter().map(|t| (t.path.clone(), t.clone())).collect();
        let (events, sources, cx, stop) = (self.events.clone(), self.sources.clone(), self.source_context(), cancel.clone());
        let spawned = std::thread::Builder::new()
            .name("popamp-library".into())
            .spawn(move || scan(id, folders, known, &sources, &cx, &stop, &|ev| events.send(AppEvent::Library(ev))));
        match spawned {
            Ok(_) => self.library.scan = Some(Scan { id, done: 0, cancel, before: self.library.lib.tracks.clone() }),
            Err(e) => log::warn!("cannot start a library scan: {e}"),
        }
    }

    pub fn on_library(&mut self, ev: LibEvent) {
        let pane = &mut self.library;
        let id = match &ev {
            LibEvent::Progress(id, _) | LibEvent::Folder(id, ..) | LibEvent::Done(id) => *id,
            LibEvent::Covers(done, failed) => return self.covers_written(done, *failed),
        };
        if pane.scan.as_ref().is_none_or(|s| s.id != id) {
            return;
        }
        match ev {
            LibEvent::Progress(_, done) => pane.scan.as_mut().unwrap().done = done,
            LibEvent::Folder(_, folder, found) => {
                if pane.lib.folders.contains(&folder)
                    && pane.lib.tracks.iter().filter(|t| popamp_core::library::is_under(&t.path, &folder)).ne(&found)
                {
                    pane.lib.replace_folder(&folder, found);
                    pane.changed();
                }
            }
            LibEvent::Done(_) => {
                let before = pane.scan.take().map(|s| s.before).unwrap_or_default();
                self.save_library();
                self.follow_moves(&before);
            }
            LibEvent::Covers(..) => {}
        }
    }

    /// A picture was let go on the library over `targets`: asks before it
    /// goes into their files as their cover. Only the formats Popamp
    /// writes tags into take it; a CUE sheet's tracks do not.
    pub fn library_cover_dropped(&mut self, image: &str, targets: Vec<String>) {
        if std::fs::read(image).ok().and_then(popamp_audio::tags::picture).is_none() {
            return self.flash("LIBRARY: NOT A JPEG, PNG, GIF OR BMP PICTURE");
        }
        let writable = |p: &String| popamp_core::cue::split_location(p).is_none() && popamp_audio::tags::named_editable(Path::new(p));
        let paths: Vec<String> = targets.into_iter().filter(writable).collect();
        if paths.is_empty() {
            return self.flash("LIBRARY: NO COVER CAN GO INTO THESE FILES");
        }
        self.request_dialog(DialogKind::WriteCover { image: image.to_string(), paths });
    }

    /// Writes the picture at `image` into `paths` as their cover, away
    /// from the window; `LibEvent::Covers` tells when it is done.
    pub fn library_write_cover(&mut self, image: &str, paths: Vec<String>) {
        let Some(picture) = std::fs::read(image).ok().and_then(popamp_audio::tags::picture) else {
            return self.flash("LIBRARY: NOT A JPEG, PNG, GIF OR BMP PICTURE");
        };
        let events = self.events.clone();
        let spawned = std::thread::Builder::new().name("popamp-cover".into()).spawn(move || {
            let (done, failed) = write_covers(&picture, &paths);
            events.send(AppEvent::Library(LibEvent::Covers(done, failed)));
        });
        if let Err(e) = spawned {
            log::warn!("cannot start writing covers: {e}");
        }
    }

    /// The files that took a new cover are read again; says how it went.
    fn covers_written(&mut self, done: &[String], failed: usize) {
        for path in done {
            self.tags_changed(path);
        }
        let files = |n: usize| if n == 1 { "1 FILE".to_string() } else { format!("{n} FILES") };
        let text = match failed {
            0 => format!("LIBRARY: COVER WRITTEN TO {}", files(done.len())),
            _ => format!("LIBRARY: COVER WRITTEN TO {}, {failed} FAILED", files(done.len())),
        };
        self.flash(text);
    }

    /// Hands the plays and ratings of the files gone since `before` to
    /// the same files found under new names, and ages those of the rest.
    fn follow_moves(&mut self, before: &[Track]) {
        let now: std::collections::HashSet<&str> = self.library.lib.tracks.iter().map(|t| t.path.as_str()).collect();
        let was: std::collections::HashSet<&str> = before.iter().map(|t| t.path.as_str()).collect();
        let gone: Vec<Track> = before.iter().filter(|t| !now.contains(t.path.as_str())).cloned().collect();
        let arrived: Vec<Track> = self.library.lib.tracks.iter().filter(|t| !was.contains(t.path.as_str())).cloned().collect();
        let moved = popamp_core::library::moves(&gone, &arrived);
        self.tidy_stats(&moved, popamp_core::stats::KEEP_MISSING);
    }

    /// Brings the plays and ratings up to date with the files on disk;
    /// see [`popamp_core::stats::Stats::tidy`].
    fn tidy_stats(&mut self, moved: &[(String, String)], keep: u64) {
        let on_disk = |p: &str| std::path::Path::new(popamp_core::cue::split_location(p).map_or(p, |(file, _)| file)).exists();
        let stats = &mut self.library.lib.stats;
        stats.tidy(moved, on_disk, popamp_core::history::now(), keep);
        if let Err(e) = stats.save(&self.paths.stats_file()) {
            log::warn!("cannot save the play counts: {e}");
        }
    }

    /// Forgets the plays and ratings of every file not there now; how many.
    pub fn forget_missing_stats(&mut self) -> usize {
        let before = self.library.lib.stats.len();
        self.tidy_stats(&[], 0);
        before - self.library.lib.stats.len()
    }

    fn save_library(&self) {
        if let Err(e) = self.library.lib.save(&self.paths.library_file()) {
            log::warn!("cannot save the library: {e}");
        }
    }

    /// Where the folder dialog opens: in the folder that holds the one
    /// added last, so its neighbours are a click away.
    pub(crate) fn library_start(&self) -> Option<PathBuf> {
        let last = Path::new(self.library.lib.folders.last()?);
        last.parent().filter(|p| p.is_dir()).map(Path::to_path_buf)
    }

    /// Takes the picked folders into the library and reads them.
    pub fn library_add_folders(&mut self, picked: &[String]) {
        let picked: Vec<&str> = picked.iter().map(|f| f.trim()).filter(|f| !f.is_empty()).collect();
        if picked.is_empty() {
            return;
        }
        let mut added = false;
        for folder in &picked {
            added |= self.library.lib.add_folder(folder);
        }
        if !added {
            return self.flash(if picked.len() == 1 {
                "LIBRARY: THE FOLDER IS THERE ALREADY"
            } else {
                "LIBRARY: THE FOLDERS ARE THERE ALREADY"
            });
        }
        self.library.changed();
        self.save_library();
        self.library_rescan();
    }

    /// Takes the folder on screen out of the library; its files stay.
    pub fn library_remove_folder(&mut self) {
        let Some(folder) = self.library.folder().map(str::to_string) else { return };
        self.library.lib.remove_folder(&folder);
        self.library.changed();
        self.save_library();
        // A scan under way would bring the folder's tracks back.
        if self.library.scan.is_some() {
            self.library_rescan();
        }
    }

    /// Plays the picked tracks in place of the playlist, or adds them to it.
    pub fn library_play(&mut self, mode: AddMode) {
        let picked = self.library.picked();
        if !picked.is_empty() {
            self.add(picked, mode);
        }
    }

    /// What Enter and a double click run (`other`: Shift+Enter): Play, or
    /// Enqueue when that is the default.
    pub fn library_open(&self, other: bool) -> Command {
        if self.settings().enqueuedef != other {
            Command::LibraryEnqueue
        } else {
            Command::LibraryPlay
        }
    }

    /// The menu of where the right button went down: the views', or the
    /// tracks'.
    pub fn library_menu(&self) -> Vec<crate::menu::MenuItem> {
        match self.library.menu_for {
            Menu::Lists => self.lists_menu(),
            Menu::Tracks => self.tracks_menu(),
            Menu::Columns => self.columns_menu(),
        }
    }

    /// Every column, ticked when the table shows it.
    fn columns_menu(&self) -> Vec<crate::menu::MenuItem> {
        use crate::menu::MenuItem;
        let cols = &self.library.columns;
        let mut items: Vec<MenuItem> =
            Column::ALL.iter().map(|&c| MenuItem::cmd(c.about().menu, "", c.about().toggle).checked(cols.has(c))).collect();
        items.extend([MenuItem::sep(), MenuItem::cmd("Reset columns", "", Command::LibraryColumnsReset)]);
        items
    }

    /// Runs a command of the menu of columns; false for any other.
    pub fn library_column_command(&mut self, c: Command) -> bool {
        match (c, Column::toggled_by(c)) {
            (Command::LibraryColumnsReset, _) => self.library.columns = Columns::default(),
            (_, Some(col)) => self.library.columns.toggle(col),
            _ => return false,
        }
        self.keep_library_columns();
        true
    }

    /// Keeps the table's columns in the settings, once they changed; the
    /// first columns as none.
    pub fn keep_library_columns(&mut self) {
        let cols = &self.library.columns;
        let setting = if *cols == Columns::default() { String::new() } else { cols.to_setting() };
        if self.settings().ml_columns != setting {
            self.settings_mut().ml_columns = setting;
        }
    }

    /// What is done with the picked tracks.
    fn tracks_menu(&self) -> Vec<crate::menu::MenuItem> {
        use crate::menu::MenuItem;
        let key = |c| if self.library_open(false) == c { "Enter" } else { "Shift+Enter" };
        let mut items = vec![
            MenuItem::cmd("Play", key(Command::LibraryPlay), Command::LibraryPlay),
            MenuItem::cmd("Enqueue", key(Command::LibraryEnqueue), Command::LibraryEnqueue),
        ];
        if !self.library.selected_paths().is_empty() {
            items.extend([
                MenuItem::sep(),
                MenuItem::sub("Rate", RATE.iter().rev().map(|&(n, c)| MenuItem::cmd(&rate_label(n), "", c)).collect()),
                MenuItem::cmd("File info…", "", Command::LibraryFileInfo),
            ]);
            if let Some(n) = self.library.plain_index() {
                let name = &self.library.plain[n].name;
                items.push(MenuItem::cmd(&format!("Remove from “{name}”"), "", Command::LibraryPlaylistRemoveTracks));
            }
        }
        items
    }

    /// The folders and the smart playlists.
    fn lists_menu(&self) -> Vec<crate::menu::MenuItem> {
        use crate::menu::MenuItem;
        let mut items = vec![MenuItem::cmd("New playlist…", "", Command::LibraryPlaylistNew)];
        if let Some(n) = self.library.plain_index() {
            let name = &self.library.plain[n].name;
            items.push(MenuItem::cmd(&format!("Rename “{name}”…"), "", Command::LibraryPlaylistRename));
            items.push(MenuItem::cmd(&format!("Delete “{name}”"), "", Command::LibraryPlaylistDelete));
        }
        items.extend([MenuItem::sep(), MenuItem::cmd("New smart playlist…", "", Command::LibrarySmartNew)]);
        if !self.library.query.trim().is_empty() {
            items.push(MenuItem::cmd("Save search as smart playlist…", "", Command::LibrarySmartFromSearch));
        }
        if let Some(list) = self.library.smart_view() {
            items.push(MenuItem::cmd(&format!("Edit “{}”…", list.name), "", Command::LibrarySmartEdit));
            items.push(MenuItem::cmd(&format!("Delete “{}”", list.name), "", Command::LibrarySmartDelete));
        }
        items.extend([
            MenuItem::sep(),
            MenuItem::cmd("Add folder…", "", Command::LibraryAddFolder),
            MenuItem::cmd("Rescan folders", "", Command::LibraryRescan),
        ]);
        if let Some(folder) = self.library.folder() {
            items.push(MenuItem::cmd(&format!("Remove “{}” from the library", folder_label(folder)), "", Command::LibraryRemoveFolder));
        }
        let maximized = MenuItem::cmd("Maximized", "", Command::ToggleLibraryMaximize).checked(self.settings().ml_max);
        items.extend([MenuItem::sep(), maximized]);
        items
    }

    /// Opens File info on the first selected track.
    pub fn library_file_info(&mut self) {
        let Some(path) = self.library.selected_paths().into_iter().next() else { return };
        let mut entry = popamp_core::playlist::Entry::new(path.clone());
        entry.meta = self.sources.of(&path).describe(&path).and_then(Result::ok).map(Arc::new);
        self.info_file = Some(entry);
        self.ui.push(UiReq::Panel(PanelKind::FileInfo));
    }

    /// Popamp changed the tags of the file at `path`: a library track of
    /// it reads them again, keeping when it was added.
    pub(crate) fn library_retag(&mut self, path: &str) {
        let Some(i) = self.library.lib.tracks.iter().position(|t| t.path == path) else { return };
        let Some(stamp) = Stamp::of(std::path::Path::new(path)) else { return };
        let meta = self.sources.of(path).describe(path).and_then(Result::ok).unwrap_or_default();
        let added = self.library.lib.tracks[i].added;
        self.library.lib.tracks[i] = Track::new(path.to_string(), stamp, added, &meta);
        // Its cover may be another now.
        self.library.shown_cover.replace(None);
        self.library.changed();
        self.save_library();
    }

    /// Opens the smart playlist editor on `editing`.
    fn edit_smart(&mut self, editing: Editing) {
        self.library.editing = Some(editing);
        self.ui.push(UiReq::Panel(PanelKind::SmartPlaylist));
    }

    pub fn library_smart_new(&mut self) {
        self.edit_smart(Editing { index: None, list: SmartList { name: "New smart playlist".into(), ..SmartList::default() } });
    }

    /// A new smart playlist of the tracks the search finds.
    pub fn library_smart_from_search(&mut self) {
        let query = self.library.query.trim().to_string();
        let Ok(test) = Test::from_text(Field::Any, Op::Has, &query) else { return };
        let list = SmartList { name: query, rules: vec![Rule::Test(test)], ..SmartList::default() };
        self.edit_smart(Editing { index: None, list });
    }

    pub fn library_smart_edit(&mut self) {
        let Some(n) = self.library.smart_index() else { return };
        self.edit_smart(Editing { index: Some(n), list: self.library.smart[n].clone() });
    }

    pub fn library_smart_delete(&mut self) {
        if let Some(index) = self.library.smart_index() {
            self.request_dialog(DialogKind::DeleteSmart { index });
        }
    }

    /// Puts `list` in place of the smart playlist at `index`, or after the
    /// others, writes them and shows it.
    pub fn save_smart(&mut self, index: Option<usize>, list: SmartList) {
        let smart = &mut self.library.smart;
        let n = match index.filter(|&n| n < smart.len()) {
            Some(n) => {
                smart[n] = list;
                n
            }
            None => {
                smart.push(list);
                smart.len() - 1
            }
        };
        self.write_smart();
        self.library.view = View::Smart(n);
        self.library.changed();
    }

    pub fn library_smart_remove(&mut self, index: usize) {
        if index < self.library.smart.len() {
            self.library.smart.remove(index);
            self.write_smart();
            // The lists after it moved up a place.
            self.library.view = View::All;
            self.library.changed();
        }
    }

    fn write_smart(&self) {
        let file = self.paths.smart_file();
        if let Err(e) = popamp_core::smart::save(&file, &self.library.smart) {
            log::warn!("cannot write {}: {e}", file.display());
        }
    }

    /// Rates the tracks selected in the library. The rows stay where
    /// they are, even in rating order, so the selection holds.
    pub fn library_rate(&mut self, stars: u8) {
        let Some(rating) = Rating::new(stars) else { return };
        let file = self.paths.stats_file();
        for path in self.library.selected_paths() {
            self.library.lib.stats.rate(&file, &path, rating);
        }
        self.library.recount();
    }

    // --- plain playlists ---------------------------------------------------------

    /// Makes an empty playlist called `name` and shows it; none when the
    /// name is no file name or another list has it.
    pub fn library_playlist_new(&mut self, name: &str) {
        let Some(name) = self.free_name(name, None) else { return };
        let list = PlainList { name, tracks: Vec::new() };
        self.write_plain(&list);
        self.library.plain.push(list);
        self.sort_plain(self.library.plain.len() - 1);
    }

    /// The list at `index` is called `name` from now on.
    pub fn library_playlist_rename(&mut self, index: usize, name: &str) {
        let Some(name) = self.free_name(name, Some(index)) else { return };
        let Some(list) = self.library.plain.get_mut(index) else { return };
        let old = list.file(&self.paths.playlists_dir());
        list.name = name;
        let list = list.clone();
        self.write_plain(&list);
        if old != list.file(&self.paths.playlists_dir()) {
            let _ = std::fs::remove_file(old);
        }
        self.sort_plain(index);
    }

    /// `name` cleaned for a file, unless a list other than `index` has it.
    fn free_name(&mut self, name: &str, index: Option<usize>) -> Option<String> {
        let name = plain::clean_name(name)?;
        let taken = self.library.plain.iter().enumerate().any(|(i, l)| Some(i) != index && l.name.eq_ignore_ascii_case(&name));
        if taken {
            self.flash(format!("A PLAYLIST IS CALLED {name} ALREADY"));
            return None;
        }
        Some(name)
    }

    /// Puts the list at `index` in its place by name and shows it.
    fn sort_plain(&mut self, index: usize) {
        let name = self.library.plain[index].name.clone();
        self.library.plain.sort_by_key(|l| l.name.to_lowercase());
        let n = self.library.plain.iter().position(|l| l.name == name).unwrap_or(0);
        self.library.view = View::Plain(n);
        self.library.changed();
    }

    pub fn library_playlist_remove(&mut self, index: usize) {
        if index >= self.library.plain.len() {
            return;
        }
        let list = self.library.plain.remove(index);
        if let Err(e) = std::fs::remove_file(list.file(&self.paths.playlists_dir())) {
            log::warn!("cannot delete the playlist {}: {e}", list.name);
        }
        self.library.view = View::All;
        self.library.changed();
    }

    /// The picked tracks go at the end of the list they were let go on.
    pub fn library_drop_on_playlist(&mut self) {
        let Some(n) = self.library.take_drop() else { return };
        let picked = self.library.picked();
        let Some(list) = self.library.plain.get_mut(n) else { return };
        if list.add(picked) > 0 {
            let list = list.clone();
            self.write_plain(&list);
            // The tracks on screen keep their selection unless they are the list's.
            if self.library.plain_index() == Some(n) {
                self.library.changed();
            } else {
                self.library.recount();
            }
        }
    }

    /// The selected tracks leave the plain playlist on screen; the
    /// library keeps them.
    pub fn library_playlist_remove_tracks(&mut self) {
        let Some(n) = self.library.plain_index() else { return };
        let gone = self.library.selected_paths();
        let list = &mut self.library.plain[n];
        list.remove(&gone);
        let list = list.clone();
        self.write_plain(&list);
        self.library.changed();
    }

    fn write_plain(&self, list: &PlainList) {
        if let Err(e) = list.save(&self.paths.playlists_dir()) {
            log::warn!("cannot write the playlist {}: {e}", list.name);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const R: Rect = rect(0, 0, 400, 300);
    const PX: i32 = 10;
    /// With the classic skin's thumb, 8 by 18.
    const AT: Place = Place { r: R, px: PX, thumb: Some((8, 18)) };

    fn track(path: &str, artist: &str, album: &str, title: &str) -> Track {
        Track { path: path.into(), artist: artist.into(), album: album.into(), title: title.into(), ..Track::default() }
    }

    fn pane() -> LibraryPane {
        LibraryPane::new(Library {
            folders: vec!["/music".into(), "/more".into()],
            tracks: vec![
                track("/music/1.mp3", "Alpha", "First", "One"),
                track("/music/2.mp3", "Alpha", "First", "Two"),
                track("/music/3.mp3", "Beta", "Second", "Three"),
                track("/more/4.mp3", "Gamma", "Third", "Four"),
            ],
            ..Library::default()
        })
    }

    fn titles(p: &LibraryPane) -> Vec<&str> {
        p.shown.iter().map(|&i| p.lib.tracks[i].title.as_str()).collect()
    }

    fn click(p: &mut LibraryPane, at: Rect, row: i32) -> Reaction {
        p.press(AT, at.x + 5, at.y + row * (PX + 3) + 2, false, false, false)
    }

    fn type_text(p: &mut LibraryPane, text: &str) {
        for c in text.chars() {
            p.key(AT, Key::Char(c), false);
        }
    }

    #[test]
    fn the_parts_fill_the_rectangle_without_overlap() {
        let p = Parts::new(AT, true);
        let [artists, albums] = p.panes.unwrap();
        assert!(p.views.x + p.views.w < p.search.x && p.search.y + p.search.h < artists.header.y);
        assert!(artists.rows.x + artists.rows.w < albums.rows.x && albums.rows.y == artists.rows.y, "side by side");
        assert!(artists.rows.y + artists.rows.h < p.table.header.y, "the table under them");
        for list in [List::Views, List::Artists, List::Albums, List::Tracks] {
            let (rows, bar) = (p.rows(list).unwrap(), p.bar(list).unwrap());
            assert_eq!((bar.x, bar.y, bar.w, bar.h), (rows.x + rows.w, rows.y, 8, rows.h), "{list:?}: a bar of its own at its right");
        }
        let bar = |list| p.bar(list).map(|b| b.x + b.w);
        assert_eq!((bar(List::Albums), bar(List::Tracks)), (Some(R.w), Some(R.w)));
        assert!(bar(List::Views).unwrap() < p.search.x && bar(List::Artists).unwrap() < albums.rows.x);
        assert!(p.table.rows.y + p.table.rows.h < p.buttons[0].y && p.buttons[3].x + p.buttons[3].w <= R.w);
        assert_eq!(Parts::new(AT, false).table.header.y, artists.header.y, "without the panes the table takes their room");
        let small = Parts::new(Place { r: rect(0, 0, 20, 10), ..AT }, true);
        assert!(small.table.rows.w >= 0 && small.table.rows.h >= 0 && small.views.h >= 0, "too small a window draws nothing wrong");
    }

    #[test]
    fn a_tall_window_has_the_cover_under_the_views_and_a_short_one_none() {
        let p = Parts::new(AT, true);
        let (cover, about) = (p.cover.unwrap(), p.about.unwrap());
        assert_eq!((cover.x, cover.w, cover.h), (R.x, p.views.w + p.bar_w, p.views.w + p.bar_w), "square, as wide as the views");
        assert!(p.views.y + p.views.h < cover.y && cover.y + cover.h <= about.y, "the views, the cover, what is known of the track");
        assert!(about.y + about.h <= p.buttons[0].y && about.h >= 4 * p.row_h);
        assert!(p.views.h >= 6 * p.row_h, "the views keep some rows");
        let short = Parts::new(Place { r: rect(0, 0, 400, 200), ..AT }, true);
        assert_eq!((short.cover, short.about), (None, None));
        assert_eq!(short.views.h, short.table.rows.y + short.table.rows.h - short.views.y, "the views take its room");
    }

    #[test]
    fn the_cover_panel_tells_of_the_track_at_the_cursor_once_picked() {
        let mut p = pane();
        assert_eq!(p.about_lines(), None, "no track is picked");
        let t = &mut p.lib.tracks[0];
        (t.year, t.bitrate_kbps) = ("1999".into(), 320);
        let rows = p.parts(AT).table.rows;
        click(&mut p, rows, 0);
        p.release(AT, rows.x + 5, rows.y + 2);
        assert_eq!(p.about_lines(), Some(["One".into(), "Alpha".into(), "First (1999)".into(), "MP3, 320 kbps".into()]));
        click(&mut p, rows, 2);
        p.release(AT, rows.x + 5, rows.y + 2 * (PX + 3) + 2);
        assert_eq!(p.about_lines().map(|l| l[3].clone()), Some("MP3".into()), "a bit rate not known is left out");
    }

    #[test]
    fn the_cover_is_the_picture_beside_the_track_shrunk_to_its_frame() {
        let dir = std::env::temp_dir().join(format!("popamp-lib-cover-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        popamp_skin::Bitmap::new(400, 200, 0xff11_2233).save_png(&dir.join("cover.png")).unwrap();
        let song = dir.join("song.mp3").to_string_lossy().into_owned();
        let mut p = LibraryPane::new(Library { tracks: vec![track(&song, "A", "B", "C")], ..Library::default() });
        p.select_only(0);
        let art = p.cover_sized((100, 100)).expect("a cover");
        assert_eq!((art.w, art.h, art.px[0] & 0xff_ffff), (100, 50, 0x11_2233), "shrunk to fit, its shape kept");
        assert_eq!(p.cover_sized((80, 80)).map(|a| (a.w, a.h)), Some((80, 40)), "a new size fits it again");
        p.selected.fill(false);
        assert!(p.cover_sized((80, 80)).is_none());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn columns_share_the_width_and_the_narrow_table_drops_some() {
        let wide = Columns::default().lay(rect(10, 0, 500, 100), PX);
        assert_eq!(wide.iter().map(|c| c.0).collect::<Vec<_>>(), FIRST);
        assert_eq!((wide[0].1, wide.last().map(|c| c.1 + c.2)), (10, Some(510)), "edge to edge");
        assert!(wide.windows(2).all(|w| w[0].1 + w[0].2 == w[1].1), "no gaps");
        let narrow = Columns::default().lay(rect(0, 0, 200, 100), PX);
        assert_eq!(narrow.iter().map(|c| c.0).collect::<Vec<_>>(), [Column::Artist, Column::Title, Column::Time]);
    }

    #[test]
    fn the_column_setting_reads_back_and_what_is_wrong_in_it_keeps_out() {
        let cols = Columns::parse("title:90, plays:45,nonsense:3,title:20,year:0,added");
        assert_eq!(cols.0, [(Column::Title, 90), (Column::Plays, 45)]);
        assert_eq!(Columns::parse(&cols.to_setting()), cols);
        assert_eq!(Columns::parse(""), Columns::default());
        assert_eq!(Columns::parse("year:0"), Columns::default(), "none good is the first columns");
    }

    #[test]
    fn a_column_shows_and_hides_but_the_last_one_stays() {
        let mut cols = Columns::parse("title:90");
        cols.toggle(Column::Bitrate);
        assert_eq!(cols.0, [(Column::Title, 90), (Column::Bitrate, 50)], "a new column comes at the right");
        cols.toggle(Column::Title);
        cols.toggle(Column::Bitrate);
        assert_eq!(cols.0, [(Column::Bitrate, 50)]);
    }

    /// The left edge of each column of the table and its right end.
    fn edges(p: &LibraryPane) -> Vec<(Column, i32, i32)> {
        p.columns.lay(p.parts(AT).table.rows, PX)
    }

    #[test]
    fn a_border_in_the_header_moves_with_the_mouse_and_only_its_two_columns_change() {
        let mut p = pane();
        p.columns = Columns::parse("artist:70,title:90,time:40");
        let header = p.parts(AT).table.header;
        let before = edges(&p);
        let border = before[0].1 + before[0].2;
        let re = p.press(AT, border, header.y + 2, false, false, false);
        assert!(re.capture, "the mouse holds the border");
        assert!(p.drag(AT, border + 30, header.y + 2));
        assert_eq!(p.release(AT, border + 30, header.y + 2), None);
        let after = edges(&p);
        assert!((after[0].2 - before[0].2 - 30).abs() <= 1, "{before:?} → {after:?}");
        assert_eq!(after[1].1 + after[1].2, before[1].1 + before[1].2, "the column on its right gave the room");
        assert_eq!(after[2], before[2]);
        assert_eq!(titles(&p), ["One", "Two", "Three", "Four"], "a press on a border does not sort");
        p.press(AT, border + 30, header.y + 2, false, false, false);
        p.drag(AT, header.x + header.w, header.y + 2);
        assert!(edges(&p)[1].2 >= 2 * PX, "a column keeps some width");
    }

    #[test]
    fn the_new_columns_show_the_number_file_bit_rate_and_plays() {
        let mut p = pane();
        let t = &mut p.lib.tracks[0];
        (t.number, t.bitrate_kbps, t.comment, t.added) = (7, 320, "live".into(), 1_794_144_449);
        p.lib.stats = popamp_core::stats::Stats::parse("P\t1794144449\t/music/1.mp3\nP\t1794144449\t/music/1.mp3\n");
        p.tz = jiff::tz::TimeZone::UTC;
        let t = p.lib.tracks[0].clone();
        let cells: Vec<String> = Column::ALL[7..].iter().map(|&c| p.cell(&t, c)).collect();
        assert_eq!(cells, ["7", "1.mp3", "live", "320", "2", "2026-11-08", "2026-11-08"]);
        let none = p.lib.tracks[1].clone();
        let empty: Vec<String> = Column::ALL[7..].iter().map(|&c| p.cell(&none, c)).collect();
        assert_eq!(empty, ["", "2.mp3", "", "", "", "", ""], "nothing known shows nothing");
    }

    #[test]
    fn a_new_column_sorts_from_its_header() {
        let mut p = pane();
        p.columns = Columns::parse("title:90,plays:40");
        p.lib.stats = popamp_core::stats::Stats::parse("P\t5\t/music/3.mp3\n");
        let header = p.parts(AT).table.header;
        let plays = edges(&p)[1];
        p.press(AT, plays.1 + plays.2 / 2, header.y + 2, false, false, false);
        assert_eq!(titles(&p)[0], "Three", "the most played first");
    }

    #[test]
    fn the_header_has_a_menu_of_the_columns_that_shows_and_hides_them() {
        let mut m = Model::for_tests();
        m.library = pane();
        let header = m.library.parts(AT).table.header;
        m.library.press_right(AT, header.x + 5, header.y + 2);
        let menu = m.library_menu();
        let shown: Vec<&str> = menu.iter().filter(|i| i.checked).map(|i| i.label.as_str()).collect();
        assert_eq!(shown, ["Artist", "Title", "Album", "Time", "Genre", "Year", "Rating"]);
        assert!(menu.iter().any(|i| i.label == "Bit rate" && !i.checked));
        m.exec(Command::LibraryColumnBitrate);
        assert!(m.library.columns.has(Column::Bitrate));
        assert!(m.settings().ml_columns.ends_with(",bitrate:50"), "{}", m.settings().ml_columns);
        m.exec(Command::LibraryColumnsReset);
        assert_eq!((m.library.columns.clone(), m.settings().ml_columns.as_str()), (Columns::default(), ""));
    }

    #[test]
    fn a_moved_border_is_kept_in_the_settings_once_let_go() {
        let mut m = Model::for_tests();
        m.library = pane();
        let header = m.library.parts(AT).table.header;
        let first = edges(&m.library)[0];
        let border = first.1 + first.2;
        m.library.press(AT, border, header.y + 2, false, false, false);
        m.library.drag(AT, border - 20, header.y + 2);
        m.library.release(AT, border - 20, header.y + 2);
        m.keep_library_columns();
        assert_eq!(Columns::parse(&m.settings().ml_columns), m.library.columns);
        assert_ne!(m.library.columns, Columns::default());
    }

    #[test]
    fn typing_searches_and_escape_clears() {
        let mut p = pane();
        type_text(&mut p, "alpha");
        assert_eq!((titles(&p), p.artists.len()), (vec!["One", "Two"], 1), "the panes follow the search");
        p.key(AT, Key::Backspace, false);
        assert_eq!(p.query, "alph");
        p.key(AT, Key::Escape, false);
        assert_eq!(titles(&p).len(), 4);
        assert_eq!(p.key(AT, Key::Escape, false), (false, None), "Escape is left to the window when there is no search");
    }

    #[test]
    fn an_artist_narrows_the_albums_and_an_album_the_tracks() {
        let mut p = pane();
        p.lib.tracks.push(track("/music/5.mp3", "Alpha", "Other", "Five"));
        p.refresh();
        let [artists, albums] = p.parts(AT).panes.unwrap();
        assert_eq!(p.pane_row(List::Artists, 0)[0], "All (3 artists)");
        assert_eq!(p.pane_row(List::Albums, 0)[0], "All (4 albums)");
        click(&mut p, artists.rows, 1);
        assert_eq!(p.pane_row(List::Albums, 0)[0], "All (2 albums)", "Alpha's albums only");
        assert_eq!(p.pane_row(List::Artists, 1), ["Alpha", "2", "3"]);
        assert_eq!(titles(&p), ["One", "Two", "Five"]);
        click(&mut p, albums.rows, 1);
        assert_eq!(titles(&p), ["One", "Two"]);
        click(&mut p, artists.rows, 2);
        assert_eq!((p.album.as_deref(), titles(&p)), (None, vec!["Three"]), "a new artist shows all their albums");
        click(&mut p, artists.rows, 0);
        assert_eq!(titles(&p).len(), 5, "the first row is all of them");
        let play = p.press(AT, albums.rows.x + 5, albums.rows.y + 2 * (PX + 3) + 2, false, false, true);
        assert_eq!((play.run, p.picked().len()), (Some(Command::LibraryOpen), 1), "a double click opens the album");
    }

    #[test]
    fn tab_goes_through_the_lists_and_the_arrows_pick_in_a_pane() {
        let mut p = pane();
        p.key(AT, Key::Tab, false);
        assert_eq!(p.focus, List::Artists);
        p.key(AT, Key::Down, false);
        p.key(AT, Key::Down, false);
        assert_eq!((p.artist.as_deref(), titles(&p)), (Some("Beta"), vec!["Three"]));
        p.key(AT, Key::Tab, false);
        p.key(AT, Key::Down, false);
        assert_eq!(p.album.as_deref(), Some("Second"));
        p.key(AT, Key::Tab, true);
        assert_eq!(p.focus, List::Artists, "Shift+Tab goes back");
    }

    /// The views as the tree shows them: each label indented two spaces a
    /// level, after `+` for a closed node and `-` for an open one.
    fn tree(p: &LibraryPane) -> Vec<String> {
        let mark = |r: &Row| match r.fold {
            Some(true) => "- ",
            Some(false) => "+ ",
            None => "",
        };
        p.tree.iter().map(|r| format!("{}{}{}", "  ".repeat(r.depth as usize), mark(r), p.view_label(&r.view))).collect()
    }

    /// A pane whose first folder has folders in it, two levels deep.
    fn nested() -> LibraryPane {
        let mut p = pane();
        p.lib.tracks.extend([
            track("/music/Rock/Old/5.mp3", "Delta", "Fourth", "Five"),
            track("/music/Rock/6.mp3", "Delta", "Fourth", "Six"),
            track("/music/Jazz/7.mp3", "Epsilon", "Fifth", "Seven"),
            track("/music/Jazz/Live.cue#2", "Epsilon", "Live", "Eight"),
        ]);
        p.changed();
        p
    }

    #[test]
    fn the_views_are_a_tree_whose_groups_fold() {
        let mut p = pane();
        assert_eq!(tree(&p), ["All music", "- Playlists", "- Folders", "  music", "  more"]);
        let (x, y) = view_row(&p, "Folders");
        p.press(AT, x, y, false, false, false);
        assert_eq!(tree(&p), ["All music", "- Playlists", "+ Folders"], "a branch folds and is no view");
        assert_eq!(p.view, View::All);
        p.press(AT, x, y, false, false, false);
        assert_eq!(tree(&p).len(), 5, "and opens again");
    }

    #[test]
    fn a_folder_opens_into_the_folders_in_it_and_each_shows_only_its_own_tracks() {
        let mut p = nested();
        assert_eq!(tree(&p)[3..], ["  + music", "  more"], "a folder without folders in it does not open");
        let (x, y) = view_row(&p, "music");
        p.press(AT, x, y, false, false, true);
        assert_eq!(tree(&p)[3..], ["  - music", "    Jazz", "    + Rock", "  more"], "in order, a CUE sheet's folder too");
        let (x, y) = view_row(&p, "Rock");
        p.press(AT, x, y, false, false, false);
        assert_eq!((p.view.clone(), titles(&p)), (View::Folder("/music/Rock".into()), vec!["Five", "Six"]), "with what is under it");
        assert_eq!(p.folder(), None, "no folder of the library itself");
        let parts = p.parts(AT);
        p.press(AT, parts.views.x + 2 * PX + 2, y, false, false, false);
        assert_eq!(tree(&p)[5..7], ["    - Rock", "      Old"], "its mark opens it without a double click");
        p.lib.replace_folder("/music", vec![track("/music/1.mp3", "Alpha", "First", "One")]);
        p.changed();
        assert_eq!((p.view.clone(), tree(&p)[3].as_str()), (View::All, "  music"), "a folder that is gone takes its view along");
    }

    #[test]
    fn the_tree_keeps_what_is_open_and_shown_and_comes_back_so() {
        let mut p = nested();
        p.fold(Node::Folder("/music".into()));
        p.fold(Node::Folder("/music/Rock".into()));
        p.fold(Node::Branch(Branch::Playlists));
        let (x, y) = view_row(&p, "Old");
        p.press(AT, x, y, false, false, false);
        let (open, view) = p.tree_state();
        let mut back = nested();
        back.restore_tree_state(&open, &view);
        assert_eq!((tree(&back), back.view.clone(), titles(&back)), (tree(&p), p.view.clone(), titles(&p)));
        assert_eq!(back.view, View::Folder("/music/Rock/Old".into()));
    }

    #[test]
    fn a_playlist_comes_back_by_its_name_and_one_that_is_gone_is_all_music() {
        let mut p = pane();
        p.smart = popamp_core::smart::parse("[[smart]]\nname = \"A\"\n[[smart]]\nname = \"B\"\n").0;
        p.plain = vec![PlainList { name: "Mine".into(), tracks: vec![] }];
        p.changed();
        let state = |p: &mut LibraryPane, v: View| {
            p.open_view(v);
            p.tree_state()
        };
        let (open, smart) = state(&mut p, View::Smart(1));
        let (_, plain) = state(&mut p, View::Plain(0));
        let mut back = pane();
        back.smart = popamp_core::smart::parse("[[smart]]\nname = \"B\"\n").0;
        back.plain = vec![PlainList { name: "Mine".into(), tracks: vec![] }];
        back.changed();
        back.restore_tree_state(&open, &smart);
        assert_eq!(back.view, View::Smart(0), "B, though it moved up");
        back.restore_tree_state(&open, &plain);
        assert_eq!(back.view, View::Plain(0));
        back.restore_tree_state(&open, "smart:A");
        assert_eq!(back.view, View::All, "A is gone");
        back.restore_tree_state("", "nonsense");
        assert_eq!((tree(&back)[1].as_str(), back.view.clone()), ("+ Playlists", View::All), "nothing open, a view not known");
    }

    #[test]
    fn the_session_keeps_the_tree_in_the_settings() {
        let mut m = Model::for_tests();
        m.library = nested();
        m.library.fold(Node::Folder("/music".into()));
        m.library.open_view(View::Folder("/music/Jazz".into()));
        m.save_session();
        let s = m.settings();
        let mut back = nested();
        back.restore_tree_state(&s.ml_tree, &s.ml_view);
        assert_eq!((tree(&back), back.view.clone()), (tree(&m.library), View::Folder("/music/Jazz".into())));
    }

    #[test]
    fn a_folder_is_a_view_of_its_tracks() {
        let mut p = pane();
        let (x, y) = view_row(&p, "more");
        p.press(AT, x, y, false, false, false);
        assert_eq!((p.folder(), titles(&p), p.artists.len()), (Some("/more"), vec!["Four"], 1));
        p.lib.remove_folder("/more");
        p.refresh();
        assert_eq!(p.view, View::All, "the folder's view goes with it");
    }

    #[test]
    fn a_smart_playlist_is_a_view_of_its_tracks_in_its_order() {
        let mut p = pane();
        let text = "[[smart]]\nname = \"Alpha\"\nsort = [{ by = \"title\", desc = true }]\nrule = [{ field = \"artist\", op = \"is\", value = \"alpha\" }]\n";
        p.smart = popamp_core::smart::parse(text).0;
        p.changed();
        let parts = p.parts(AT);
        let labels = view_rows(&p);
        assert_eq!(labels, ["All music", "Playlists", "Smart playlists", "Alpha (2)", "Folders", "music", "more"]);
        click(&mut p, parts.views, 3);
        assert_eq!((titles(&p), p.parts(AT).panes), (vec!["Two", "One"], None), "its own order, without the panes");
        type_text(&mut p, "one");
        assert_eq!(titles(&p), ["One"], "the search looks within the list");
        p.smart.clear();
        p.refresh();
        assert_eq!(p.view, View::All, "a list that is gone takes its view with it");
    }

    #[test]
    fn rows_select_as_in_the_playlist_and_what_is_picked_plays() {
        let mut p = pane();
        let rows = p.parts(AT).table.rows;
        let at = |row: i32| (rows.x + 5, rows.y + row * (PX + 3) + 2);
        assert_eq!(p.picked().len(), 4, "nothing selected: all the shown");
        click(&mut p, rows, 0);
        assert_eq!(p.picked(), ["/music/1.mp3"]);
        p.press(AT, at(2).0, at(2).1, true, false, false);
        assert_eq!(p.picked().len(), 3, "Shift takes the range");
        p.press(AT, at(1).0, at(1).1, false, true, false);
        assert_eq!(p.picked().len(), 2, "Ctrl takes one out");
        let double = p.press(AT, at(3).0, at(3).1, false, false, true);
        assert_eq!((double.run, p.picked()), (Some(Command::LibraryOpen), vec!["/more/4.mp3".to_string()]));
        assert_eq!(p.key(AT, Key::Enter, false), (true, Some(Command::LibraryOpen)));
        assert_eq!(p.key(AT, Key::Enter, true), (true, Some(Command::LibraryOpenOther)));
    }

    #[test]
    fn picked_rows_are_carried_once_the_mouse_moves_with_them() {
        let mut p = pane();
        let rows = p.parts(AT).table.rows;
        let (x, y) = (rows.x + 5, rows.y + 2);
        click(&mut p, rows, 0);
        p.press(AT, x, y + PX + 3, true, false, false);
        assert!(p.press(AT, x, y, false, false, false).capture, "a press on a row keeps the mouse");
        assert!(!p.drag(AT, x + 2, y + 2), "a shaky click carries nothing");
        assert!(!p.carrying());
        assert!(p.drag(AT, x, y + 40));
        assert!(p.carrying());
        assert!(p.take_carried() && !p.take_carried() && !p.carrying(), "taken once");
        p.release(AT, x, y + 40);
        assert_eq!(p.picked().len(), 2, "the selection goes along whole");
        p.press(AT, x, y, false, false, false);
        p.release(AT, x, y);
        assert!(!p.take_carried());
        assert_eq!(p.picked(), ["/music/1.mp3"], "a click without moving picks the one row");
    }

    #[test]
    fn the_header_sorts_and_a_second_click_turns_the_order() {
        let mut p = pane();
        let t = p.parts(AT).table;
        let title_x = p.columns.lay(t.rows, PX).iter().find(|c| c.0 == Column::Title).unwrap().1;
        p.press(AT, title_x + 3, t.header.y + 2, false, false, false);
        assert_eq!(titles(&p), ["Four", "One", "Three", "Two"]);
        p.press(AT, title_x + 3, t.header.y + 2, false, false, false);
        assert_eq!(titles(&p), ["Two", "Three", "One", "Four"]);
    }

    #[test]
    fn a_button_runs_when_let_go_over_itself() {
        let mut p = pane();
        let k = p.parts(AT).buttons[2];
        assert!(p.press(AT, k.x + 2, k.y + 5, false, false, false).capture);
        assert_eq!(p.release(AT, k.x + 2, k.y + 5), Some(Command::LibraryAddFolder));
        p.press(AT, k.x + 2, k.y + 5, false, false, false);
        assert_eq!(p.release(AT, 0, 0), None, "let go elsewhere");
    }

    #[test]
    fn the_arrows_move_and_keep_the_row_in_sight() {
        let mut p = pane();
        p.lib.tracks = (0..100).map(|i| track(&format!("/music/{i:03}.mp3"), "A", "B", &format!("{i:03}"))).collect();
        p.refresh();
        p.key(AT, Key::End, false);
        let parts = p.parts(AT);
        let (scroll, page) = p.window(&parts, List::Tracks);
        assert_eq!((p.cursor, scroll), (99, 100 - page));
        p.key(AT, Key::Home, true);
        assert_eq!(p.picked().len(), 100, "Shift+Home takes all above");
        let rows = p.parts(AT).table.rows;
        p.wheel(AT, rows.x + 5, rows.y + 5, -2);
        assert_eq!(p.window(&parts, List::Tracks).0, 6, "three rows a notch");
    }

    #[test]
    fn each_list_scrolls_by_its_own_bar() {
        let mut p = pane();
        p.lib.tracks = (0..100).map(|i| track(&format!("/music/{i:03}.mp3"), &format!("Artist {i:03}"), "B", "T")).collect();
        p.refresh();
        let parts = p.parts(AT);
        let bar = parts.bar(List::Artists).unwrap();
        let thumb = p.thumb(&parts, List::Artists).unwrap();
        assert_eq!((thumb.y, thumb.h), (bar.y, 18), "at the top, as long as the skin's thumb");
        assert!(p.press(AT, bar.x + 2, thumb.y + 5, false, false, false).capture, "the thumb is held");
        assert!(p.drag(AT, bar.x, bar.y + bar.h + 50), "dragged past the end");
        let (scroll, page) = p.window(&parts, List::Artists);
        assert_eq!(scroll, 101 - page, "the last artist in sight");
        assert_eq!((p.scroll[List::Tracks as usize], p.scroll[List::Views as usize]), (0, 0), "the other lists stay");
        assert_eq!(p.artist, None, "scrolling picks nothing");
        p.release(AT, 0, 0);
        assert!(!p.drag(AT, bar.x, bar.y), "let go");

        let table = parts.bar(List::Tracks).unwrap();
        p.press(AT, table.x + 2, table.y + table.h / 2, false, false, false);
        let (scroll, page) = p.window(&parts, List::Tracks);
        assert!((scroll as i32 - (100 - page as i32) / 2).abs() <= 1, "a press on the bar brings the thumb there: {scroll}");
        let own = Place { thumb: None, ..AT };
        let thumb = p.thumb(&p.parts(own), List::Tracks).unwrap();
        assert_eq!(thumb.h, table.h * page as i32 / 100, "the player's own thumb is as long as the share in sight");
        p.lib.tracks.truncate(3);
        p.refresh();
        assert_eq!(p.thumb(&p.parts(own), List::Tracks), None, "and is gone when all rows show");
    }

    #[test]
    fn the_status_counts_the_tracks_and_their_time() {
        let mut p = pane();
        assert_eq!(p.status(), "4 tracks", "no lengths known");
        p.lib.tracks[0].length_ms = Some(3_600_000);
        p.lib.tracks[3].length_ms = Some(61_000);
        p.refresh();
        assert_eq!(p.status(), "4 tracks [1:01:01]");
    }

    #[test]
    fn a_scan_keeps_unchanged_files_and_reads_the_new() {
        let dir = std::env::temp_dir().join(format!("popamp-libscan-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        for name in ["old.mp3", "new.mp3", "notes.txt"] {
            std::fs::write(dir.join(name), b"x").unwrap();
        }
        let folder = dir.to_string_lossy().into_owned();
        let old_path = dir.join("old.mp3").to_string_lossy().into_owned();
        let stamp = Stamp::of(&dir.join("old.mp3")).unwrap();
        let old = Track { stamp, added: 7, ..track(&old_path, "Kept", "", "As it was") };
        let known = HashMap::from([(old_path.clone(), old.clone())]);
        let m = Model::for_tests();
        let events = std::cell::RefCell::new(Vec::new());
        scan(1, vec![folder.clone()], known, &m.sources, &m.source_context(), &AtomicBool::new(false), &|ev| {
            events.borrow_mut().push(ev);
            true
        });
        let events = events.into_inner();
        let [LibEvent::Folder(1, f, found), LibEvent::Done(1)] = events.as_slice() else { panic!("{events:?}") };
        assert_eq!(f, &folder);
        let mut names: Vec<(&str, &str)> = found.iter().map(|t| (t.title.as_str(), t.artist.as_str())).collect();
        names.sort();
        assert_eq!(names, [("As it was", "Kept"), ("new", "")], "the text file is no track; the new one shows its name");
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn a_scan_lists_the_tracks_of_a_cue_sheet() {
        let dir = std::env::temp_dir().join(format!("popamp-libcue-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let fixture = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../crates/popamp-audio/tests/fixtures/silence.wav");
        std::fs::copy(fixture, dir.join("album.wav")).unwrap();
        let sheet = "FILE \"album.flac\" WAVE\n  TRACK 01 AUDIO\n    TITLE \"One\"\n    INDEX 01 00:00:00\n  TRACK 02 AUDIO\n    TITLE \"Two\"\n    INDEX 01 00:00:15\n";
        std::fs::write(dir.join("album.cue"), sheet).unwrap();
        let m = Model::for_tests();
        let events = std::cell::RefCell::new(Vec::new());
        scan(
            1,
            vec![dir.to_string_lossy().into_owned()],
            HashMap::new(),
            &m.sources,
            &m.source_context(),
            &AtomicBool::new(false),
            &|ev| {
                events.borrow_mut().push(ev);
                true
            },
        );
        let events = events.into_inner();
        let [LibEvent::Folder(1, _, found), LibEvent::Done(1)] = events.as_slice() else { panic!("{events:?}") };
        assert_eq!(found.iter().map(|t| t.title.as_str()).collect::<Vec<_>>(), ["One", "Two"]);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn a_scan_result_fills_the_library_and_is_saved() {
        let mut m = Model::for_tests();
        m.library.lib.add_folder("/music");
        m.library.scan = Some(Scan { id: 3, done: 0, cancel: Arc::new(AtomicBool::new(false)), before: Vec::new() });
        m.on_library(LibEvent::Folder(2, "/music".into(), vec![track("/music/x.mp3", "", "", "Stale")]));
        assert!(m.library.lib.tracks.is_empty(), "an older scan's news is dropped");
        m.on_library(LibEvent::Progress(3, 40));
        assert_eq!(m.library.status(), "Scanning: 40");
        m.on_library(LibEvent::Folder(3, "/music".into(), vec![track("/music/a.mp3", "A", "", "Song")]));
        m.on_library(LibEvent::Done(3));
        assert_eq!((m.library.status().as_str(), titles(&m.library)), ("1 track", vec!["Song"]));
        assert_eq!(Library::load(&m.paths.library_file()), m.library.lib);
        m.library.view = View::Folder("/music".into());
        let parts = m.library.parts(AT);
        m.library.show_row(&parts, List::Views, view_rows(&m.library).len() - 1);
        let (x, y) = view_row(&m.library, "music");
        m.library.press_right(AT, x, y);
        assert!(labels(&m).iter().any(|l| l.starts_with("Remove")), "a folder's view offers to remove it");
        m.library_remove_folder();
        assert!(m.library.lib.folders.is_empty() && m.library.lib.tracks.is_empty());
    }

    #[test]
    fn selected_tracks_are_rated_from_the_menu_and_keep_their_stars() {
        let mut m = Model::for_tests();
        m.library = pane();
        let has_rate = |m: &Model| m.library_menu().iter().any(|i| i.label == "Rate");
        assert!(!has_rate(&m), "nothing selected, nothing to rate");
        m.library.select_only(1);
        let rate = m.library_menu().into_iter().find(|i| i.label == "Rate").expect("offered for a selection");
        let labels: Vec<&str> = rate.sub.iter().map(|i| i.label.as_str()).collect();
        assert_eq!(labels, ["★★★★★", "★★★★☆", "★★★☆☆", "★★☆☆☆", "★☆☆☆☆", "No rating"]);
        let picked = m.library.selected_paths();
        m.exec(Command::LibraryRate4);
        assert_eq!(m.library.cell(&m.library.lib.tracks[m.library.shown[1]], Column::Rating), "★★★★☆");
        let kept = popamp_core::stats::Stats::load(&m.paths.stats_file());
        assert_eq!((picked.len(), kept.get(&picked[0]).rating.stars()), (1, 4));
        m.exec(Command::LibraryRate0);
        assert_eq!(m.library.cell(&m.library.lib.tracks[m.library.shown[1]], Column::Rating), "", "unrated shows nothing");
    }

    fn labels(m: &Model) -> Vec<String> {
        m.library_menu().into_iter().map(|i| i.label).filter(|l| !l.is_empty()).collect()
    }

    /// The right button goes down at the bottom of `r`, below the rows a
    /// short list has.
    fn right_click(m: &mut Model, r: Rect) {
        m.library.press_right(AT, r.x + r.w / 2, r.y + r.h - 2);
    }

    #[test]
    fn a_track_has_a_menu_of_its_own_and_the_lists_have_theirs() {
        let mut m = Model::for_tests();
        m.library = pane();
        m.library.view = View::Folder("/music".into());
        m.library.changed();
        let parts = m.library.parts(AT);
        let first = Rect { h: parts.row_h, ..parts.table.rows };
        right_click(&mut m, first);
        let track = labels(&m);
        assert_eq!(m.library.selected_paths(), ["/music/1.mp3"], "the row under the mouse");
        assert_eq!(track, ["Play", "Enqueue", "Rate", "File info…"]);
        right_click(&mut m, parts.views);
        let lists = labels(&m);
        assert!(lists.iter().all(|l| !track.contains(l)), "{lists:?}");
        for want in ["New smart playlist…", "Add folder…", "Rescan folders", "Remove “music” from the library"] {
            assert!(lists.iter().any(|l| l == want), "{want} in {lists:?}");
        }
    }

    #[test]
    fn the_lists_menu_maximizes_the_window() {
        let mut m = Model::for_tests();
        m.library.menu_for = Menu::Lists;
        let maximized = |m: &Model| m.library_menu().into_iter().find(|i| i.label == "Maximized").map(|i| i.checked);
        assert_eq!(maximized(&m), Some(false));
        m.exec(Command::ToggleLibraryMaximize);
        assert_eq!(maximized(&m), Some(true));
    }

    #[test]
    fn file_info_shows_the_library_track_and_its_new_tags_reach_the_library() {
        let dir = std::env::temp_dir().join(format!("popamp-lib-info-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("song.flac");
        let fixture = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../crates/popamp-audio/tests/fixtures/silence.flac");
        std::fs::copy(fixture, &path).unwrap();
        let file = path.to_string_lossy().into_owned();
        let mut m = Model::for_tests();
        m.library = LibraryPane::new(Library { tracks: vec![Track { added: 7, ..track(&file, "", "", "Old") }], ..Library::default() });
        assert!(m.info_entry().is_none(), "nothing in the playlist");
        m.library.select_only(0);
        m.exec(Command::LibraryFileInfo);
        assert!(matches!(m.ui.last(), Some(UiReq::Panel(PanelKind::FileInfo))));
        assert_eq!(m.info_entry().map(|e| e.location.as_str()), Some(file.as_str()), "the library's track, not the playlist's");
        let changes = popamp_audio::tags::Changes {
            fields: vec![(popamp_audio::tags::Field::Title, "New".into())],
            cover: popamp_audio::tags::CoverChange::Keep,
        };
        popamp_audio::tags::write(&path, &changes).unwrap();
        m.tags_changed(&file);
        assert_eq!((titles(&m.library), m.library.lib.tracks[0].added), (vec!["New"], 7), "read again, still added then");
        assert_eq!(Library::load(&m.paths.library_file()).tracks[0].title, "New", "and saved");
        m.exec(Command::FileInfo);
        assert!(m.info_entry().is_none(), "File info from the player is about the playlist again");
        std::fs::remove_dir_all(&dir).unwrap();
    }

    /// A folder of its own for a test, with a copy of the silent FLAC in
    /// it under each of `names`.
    fn flacs(test: &str, names: &[&str]) -> (PathBuf, Vec<String>) {
        let dir = std::env::temp_dir().join(format!("popamp-lib-{test}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let fixture = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../crates/popamp-audio/tests/fixtures/silence.flac");
        let files = names.iter().map(|n| {
            std::fs::copy(&fixture, dir.join(n)).unwrap();
            dir.join(n).to_string_lossy().into_owned()
        });
        let files = files.collect();
        (dir, files)
    }

    /// A small PNG picture in `dir`.
    fn png(dir: &Path) -> String {
        let path = dir.join("art.png");
        Bitmap::new(4, 4, 0xff20_4060).save_png(&path).unwrap();
        path.to_string_lossy().into_owned()
    }

    #[test]
    fn a_picture_dropped_on_a_row_is_for_that_track_or_for_all_the_picked_ones() {
        let mut p = pane();
        let rows = p.parts(AT).table.rows;
        let row = |n: i32| (rows.x + 5, rows.y + n * (PX + 3) + 2);
        let (x, y) = row(2);
        assert_eq!(p.cover_drop_targets(AT, x, y), ["/music/3.mp3"]);
        assert_eq!(p.selected_paths(), ["/music/3.mp3"], "the row it fell on is picked");
        p.selected[0] = true;
        assert_eq!(p.cover_drop_targets(AT, x, y), ["/music/1.mp3", "/music/3.mp3"], "on a picked row: all picked");
        let (x, y) = row(1);
        assert_eq!(p.cover_drop_targets(AT, x, y), ["/music/2.mp3"], "on another row: that one");
        let cover = p.parts(AT).cover.unwrap();
        assert_eq!(p.cover_drop_targets(AT, cover.x + 5, cover.y + 5), ["/music/2.mp3"], "on the cover: the picked tracks");
        let search = p.parts(AT).search;
        assert!(p.cover_drop_targets(AT, search.x + 5, search.y + 2).is_empty());
    }

    #[test]
    fn a_dropped_picture_asks_before_it_is_written_into_the_files() {
        let (dir, files) = flacs("drop-ask", &["a.flac", "b.flac"]);
        let mut m = Model::for_tests();
        let mut targets = files.clone();
        targets.push(dir.join("disc.cue#2").to_string_lossy().into_owned());
        m.library_cover_dropped(&png(&dir), targets);
        let Some(UiReq::Dialog(DialogKind::WriteCover { paths, .. }, crate::dialogs::Request::Question { text, .. })) = m.ui.last() else {
            panic!("asks: {:?}", m.ui.last())
        };
        assert_eq!(paths, &files, "not a track of a CUE sheet");
        assert!(text.contains("2 tracks"), "{text}");
        let ui = m.ui.len();
        m.library_cover_dropped(&dir.join("a.flac").to_string_lossy(), files.clone());
        assert_eq!((m.ui.len(), m.transient.as_ref().map(|t| t.0.as_str())), (ui, Some("LIBRARY: NOT A JPEG, PNG, GIF OR BMP PICTURE")));
        m.library_cover_dropped(&png(&dir), vec!["/m/x.wav".into()]);
        assert_eq!((m.ui.len(), m.transient.as_ref().map(|t| t.0.as_str())), (ui, Some("LIBRARY: NO COVER CAN GO INTO THESE FILES")));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_written_cover_is_in_the_files_and_shows_in_the_library() {
        let (dir, files) = flacs("drop-write", &["a.flac", "b.flac"]);
        let mut m = Model::for_tests();
        m.library = LibraryPane::new(Library { tracks: files.iter().map(|f| track(f, "A", "B", "C")).collect(), ..Library::default() });
        m.library.select_only(1);
        assert!(m.library.cover_sized((10, 10)).is_none(), "no cover yet");
        let picture = popamp_audio::tags::picture(std::fs::read(png(&dir)).unwrap()).unwrap();
        let mut targets = files.clone();
        targets.push(dir.join("gone.flac").to_string_lossy().into_owned());
        let (done, failed) = write_covers(&picture, &targets);
        assert_eq!((done.as_slice(), failed), (files.as_slice(), 1));
        for f in &files {
            assert_eq!(popamp_audio::cover::tagged(Path::new(f)).map(|c| c.data), Some(picture.data.clone()));
        }
        m.on_library(LibEvent::Covers(done, failed));
        assert_eq!(m.library.selected_paths(), [files[1].clone()], "the pick stays");
        assert!(m.library.cover_sized((10, 10)).is_some_and(|c| c.px[0] & 0xff_ffff == 0x20_4060), "the new cover shows");
        assert_eq!(m.transient.as_ref().map(|t| t.0.as_str()), Some("LIBRARY: COVER WRITTEN TO 2 FILES, 1 FAILED"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_change_that_leaves_the_same_rows_keeps_them_picked() {
        let mut p = pane();
        p.select_only(2);
        p.scroll[List::Tracks as usize] = 1;
        p.changed();
        assert_eq!((p.selected_paths(), p.cursor, p.scroll[List::Tracks as usize]), (vec!["/music/3.mp3".to_string()], 2, 1));
        type_text(&mut p, "alpha");
        assert!(p.selected_paths().is_empty(), "other rows: none picked");
    }

    /// What the views list says, row by row.
    fn view_rows(p: &LibraryPane) -> Vec<String> {
        p.tree.iter().map(|r| p.view_label(&r.view)).collect()
    }

    /// The middle of the views row that says `label`, as scrolled.
    fn view_row(p: &LibraryPane, label: &str) -> (i32, i32) {
        let parts = p.parts(AT);
        let n = view_rows(p).iter().position(|l| l == label).unwrap_or_else(|| panic!("{label} in {:?}", view_rows(p)));
        let n = n - p.window(&parts, List::Views).0;
        (parts.views.x + 5, parts.views.y + n as i32 * parts.row_h + parts.row_h / 2)
    }

    fn answer(m: &mut Model, kind: DialogKind, value: &str) {
        m.on_dialog(crate::dialogs::DialogResult { kind, values: vec![value.into()] });
    }

    #[test]
    fn every_playlist_says_how_many_tracks_it_has() {
        let mut m = Model::for_tests();
        m.library = pane();
        m.library.smart = vec![SmartList { name: "Everything".into(), ..SmartList::default() }];
        m.library.plain = vec![PlainList { name: "Mine".into(), tracks: vec!["/music/2.mp3".into(), "/gone.mp3".into()] }];
        m.library.changed();
        let rows = view_rows(&m.library);
        assert_eq!(rows[..5], ["All music", "Playlists", "Mine (1)", "Smart playlists", "Everything (4)"], "{rows:?}");
    }

    #[test]
    fn the_playlists_heading_is_there_before_the_first_playlist() {
        let mut m = Model::for_tests();
        m.library = pane();
        m.library.smart = vec![SmartList { name: "Everything".into(), ..SmartList::default() }];
        m.library.plain.clear();
        m.library.changed();
        let rows = view_rows(&m.library);
        assert_eq!(rows[..3], ["All music", "Playlists", "Smart playlists"], "{rows:?}");
    }

    #[test]
    fn tracks_carried_onto_a_playlist_go_into_it_and_stay_there() {
        let mut m = Model::for_tests();
        m.library = pane();
        m.exec(Command::LibraryPlaylistNew);
        assert!(matches!(m.ui.last(), Some(UiReq::Dialog(DialogKind::NewPlaylist, _))), "asks for a name");
        answer(&mut m, DialogKind::NewPlaylist, " Road/trip ");
        assert_eq!(popamp_core::plain::load(&m.paths.playlists_dir())[0].name, "Road_trip", "kept as a file of that name");
        // Back to all music, then two tracks are carried onto the list.
        let (x, y) = view_row(&m.library, "All music");
        m.library.press(AT, x, y, false, false, false);
        let rows = m.library.parts(AT).table.rows;
        let (tx, ty) = (rows.x + 5, rows.y + 2);
        m.library.press(AT, tx, ty, false, false, false);
        m.library.press(AT, tx, ty + PX + 3, true, false, false);
        m.library.release(AT, tx, ty + PX + 3);
        m.library.press(AT, tx, ty, false, false, false);
        let (x, y) = view_row(&m.library, "Road_trip (0)");
        assert!(m.library.drag(AT, x, y));
        assert!(m.library.dropping(), "the cursor can say it goes in");
        let run = m.library.release(AT, x, y).expect("drops");
        m.exec(run);
        assert!(!m.library.carrying());
        // Read back, a path without a drive gets one on Windows: the names tell.
        let kept = popamp_core::plain::load(&m.paths.playlists_dir()).remove(0).tracks;
        let names: Vec<_> = kept.iter().filter_map(|t| std::path::Path::new(t).file_name()).collect();
        assert_eq!(names, ["1.mp3", "2.mp3"]);
        let (x, y) = view_row(&m.library, "Road_trip (2)");
        m.library.press(AT, x, y, false, false, false);
        assert_eq!(titles(&m.library), ["One", "Two"], "in the order put");
        // Removed from the list, the track stays in the library.
        m.library.select_only(0);
        m.library.menu_for = Menu::Tracks;
        assert!(labels(&m).contains(&"Remove from “Road_trip”".to_string()));
        m.exec(Command::LibraryPlaylistRemoveTracks);
        assert_eq!((titles(&m.library), m.library.lib.tracks.len()), (vec!["Two"], 4));
        m.exec(Command::LibraryPlaylistRename);
        answer(&mut m, DialogKind::RenamePlaylist { index: 0 }, "Trip");
        assert_eq!(
            popamp_core::plain::load(&m.paths.playlists_dir()).iter().map(|l| (l.name.as_str(), l.tracks.len())).collect::<Vec<_>>(),
            [("Trip", 1)]
        );
        assert!(view_rows(&m.library).contains(&"Trip (1)".to_string()));
        m.exec(Command::LibraryPlaylistDelete);
        assert!(matches!(m.ui.last(), Some(UiReq::Dialog(DialogKind::DeletePlaylist { index: 0 }, _))), "asks first");
        answer(&mut m, DialogKind::DeletePlaylist { index: 0 }, "yes");
        assert!(popamp_core::plain::load(&m.paths.playlists_dir()).is_empty() && m.library.plain.is_empty());
    }

    #[test]
    fn a_new_playlist_does_not_take_the_name_of_another() {
        let mut m = Model::for_tests();
        m.library = pane();
        answer(&mut m, DialogKind::NewPlaylist, "Mine");
        answer(&mut m, DialogKind::NewPlaylist, "mine");
        answer(&mut m, DialogKind::NewPlaylist, " .. ");
        assert_eq!(m.library.plain.iter().map(|l| l.name.as_str()).collect::<Vec<_>>(), ["Mine"]);
    }

    #[test]
    fn a_rescan_hands_the_rating_to_the_file_under_its_new_name() {
        let mut m = Model::for_tests();
        let file = |path: &str| Track { stamp: Stamp { size: 1234, modified: 1 }, length_ms: Some(5000), ..track(path, "A", "", "Song") };
        m.library.lib.add_folder("/music");
        m.library.lib.tracks = vec![file("/music/a.mp3")];
        m.library.lib.stats.rate(&m.paths.stats_file(), "/music/a.mp3", Rating::new(4).unwrap());
        m.library.scan = Some(Scan { id: 3, done: 0, cancel: Arc::new(AtomicBool::new(false)), before: m.library.lib.tracks.clone() });
        m.on_library(LibEvent::Folder(3, "/music".into(), vec![file("/music/sorted/01 Song.mp3")]));
        m.on_library(LibEvent::Done(3));
        let kept = popamp_core::stats::Stats::load(&m.paths.stats_file());
        assert_eq!((kept.get("/music/a.mp3").rating.stars(), kept.get("/music/sorted/01 Song.mp3").rating.stars()), (0, 4));
    }

    #[test]
    fn enter_enqueues_by_default_and_shift_enter_plays() {
        let mut m = Model::for_tests();
        let keys = |m: &Model| m.library_menu().into_iter().take(2).map(|i| (i.label, i.accel)).collect::<Vec<_>>();
        // As the menu shows the keys: ⇧Enter on a Mac.
        let shown = |k: &str| if cfg!(target_os = "macos") { crate::mac_keys::label(k) } else { k.to_string() };
        let pair = |a: &str, b: &str| vec![("Play".to_string(), shown(a)), ("Enqueue".to_string(), shown(b))];
        assert_eq!((m.library_open(false), m.library_open(true)), (Command::LibraryEnqueue, Command::LibraryPlay));
        assert_eq!(keys(&m), pair("Shift+Enter", "Enter"), "the menu tells which key does what");
        m.settings_mut().enqueuedef = false;
        assert_eq!((m.library_open(false), m.library_open(true)), (Command::LibraryPlay, Command::LibraryEnqueue));
        assert_eq!(keys(&m), pair("Enter", "Shift+Enter"));
    }

    #[test]
    fn several_folders_go_in_at_once_and_the_next_pick_starts_beside_them() {
        use crate::dialogs::{DialogResult, Request};
        let music = std::env::temp_dir().join(format!("popamp-lib-folders-{}", std::process::id()));
        let at = |name: &str| music.join(name).to_string_lossy().into_owned();
        for name in ["A", "B", "C"] {
            std::fs::create_dir_all(music.join(name)).unwrap();
        }
        let mut m = Model::for_tests();
        let pick = |m: &mut Model| {
            m.exec(Command::LibraryAddFolder);
            let Some(UiReq::Dialog(DialogKind::LibraryFolder, Request::Folder { multiple, start, .. })) = m.ui.last() else {
                panic!("asks for folders: {:?}", m.ui.last())
            };
            assert!(*multiple, "several at once");
            start.clone()
        };
        pick(&mut m);
        m.on_dialog(DialogResult { kind: DialogKind::LibraryFolder, values: vec![at("A"), at("B"), " ".into()] });
        assert_eq!(m.library.lib.folders, [at("A"), at("B")]);
        assert_eq!(pick(&mut m), Some(music.clone()), "where the last ones were picked");
        m.on_dialog(DialogResult { kind: DialogKind::LibraryFolder, values: vec![at("B"), at("C")] });
        assert_eq!(m.library.lib.folders, [at("A"), at("B"), at("C")], "one that is there already is left alone");
        m.on_dialog(DialogResult { kind: DialogKind::LibraryFolder, values: vec![at("C")] });
        assert!(m.transient.as_ref().is_some_and(|(t, _)| t.contains("THERE ALREADY")));
        assert_eq!(Library::load(&m.paths.library_file()).folders, m.library.lib.folders, "kept");
        std::fs::remove_dir_all(&music).unwrap();
        assert_ne!(pick(&mut m), Some(music), "not a folder that is gone");
    }
}
