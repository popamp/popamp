//! Skin layout: the windows a skin shows and the elements inside them,
//! independent of the skin's file format. The player draws and runs any
//! layout the same way; a skin format only has to build one.
//!
//! Terms:
//! - *window*: one top-level window of a skin (main, equalizer, playlist,
//!   media library; or a bar along a screen edge). It has a normal mode and may have a shade
//!   mode, each with its own size and elements.
//! - *element*: a rectangle in a window that draws something and may react
//!   to the mouse: an image, button, toggle, slider, text...
//! - *binding*: what an element shows or changes in the player: a
//!   [`Command`] it runs, a [`Flag`] it shows, a [`Value`] it edits, a
//!   [`Text`] it prints. The player resolves them; the skin only names them.
//! - *anchor*: which window edge an element edge follows when the window
//!   is resized ([`Pos`]).
//! - *paint*: how an element looks in one state ([`Paint`]).
//! - *picture*: where a paint's pixels come from: a classic skin bitmap
//!   or an SVG file of a vector skin ([`Picture`]).

use crate::cursors::CursorKind;
use crate::sprites::{Rect, Sheet};
use popamp_core::config::BarEdge;
use popamp_core::Command;

#[derive(Debug, Clone, Default)]
pub struct Layout {
    pub windows: Vec<WindowDef>,
}

impl Layout {
    pub fn window(&self, key: WindowKey) -> Option<&WindowDef> {
        self.windows.iter().find(|w| w.key == key)
    }
}

/// Which of the player's windows a skin window is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum WindowKey {
    Main,
    Eq,
    Playlist,
    /// The media library.
    Library,
}

impl WindowKey {
    pub const ALL: [WindowKey; 4] = [WindowKey::Main, WindowKey::Eq, WindowKey::Playlist, WindowKey::Library];

    /// The key's place in [`WindowKey::ALL`].
    pub fn index(self) -> usize {
        self as usize
    }
}

#[derive(Debug, Clone)]
pub struct WindowDef {
    pub key: WindowKey,
    pub placement: Placement,
    pub normal: Mode,
    pub shade: Option<Mode>,
    /// The main window as a bar, in a skin that describes it both ways.
    pub bar: Option<Bar>,
    /// What a press on a spot without an element does.
    pub empty_press: EmptyPress,
    /// What the mouse wheel does over elements that do not use it.
    pub wheel: Wheel,
}

impl WindowDef {
    pub fn mode(&self, view: View) -> &Mode {
        match (view, &self.shade, &self.bar) {
            (View::Shade, Some(s), _) => s,
            (View::Bar, _, Some(b)) => &b.mode,
            _ => &self.normal,
        }
    }

    /// The view on screen: the bar when the user wants one and the skin
    /// describes it, else the shade mode or the normal one.
    pub fn view(&self, as_bar: bool, shade: bool) -> View {
        match (as_bar && self.bar.is_some(), shade) {
            (true, _) => View::Bar,
            (false, true) => View::Shade,
            (false, false) => View::Normal,
        }
    }

    /// Where the window goes while it shows `view`.
    pub fn placement_in(&self, view: View) -> Placement {
        match (view, &self.bar) {
            (View::Bar, Some(b)) => Placement::Bar(b.edge),
            _ => self.placement,
        }
    }
}

/// Which of a window's looks is meant.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum View {
    Normal,
    Shade,
    Bar,
}

/// A main window's bar view, beside its normal one.
#[derive(Debug, Clone)]
pub struct Bar {
    /// The screen edge it is at by default.
    pub edge: BarEdge,
    pub mode: Mode,
}

/// Where a window goes on the screen.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Placement {
    /// Placed by the user; windows that touch it move along with it.
    Free,
    /// Placed by the user; joins the group of the window it touches.
    Docked,
    /// A strip across the screen at this edge, by default; other windows
    /// keep out of it, like a taskbar's. Equalizer and playlist sit
    /// against it.
    Bar(BarEdge),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EmptyPress {
    Nothing,
    Move,
    /// Moves the window when the "easy move" option is on.
    MoveIfEasy,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Wheel {
    /// Changes the volume by this much per step.
    Volume(i32),
    /// Scrolls the window's list.
    ScrollList,
}

/// One look of a window: its size, default cursor and elements. Elements
/// are drawn in order; the mouse goes to the last one under it.
#[derive(Debug, Clone)]
pub struct Mode {
    pub width: Dim,
    pub height: Dim,
    pub cursor: CursorKind,
    pub elements: Vec<Element>,
    /// The visible part of the window; all of it when `None`.
    pub shape: Option<Shape>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Shape {
    /// Classic `region.txt` polygons in skin pixels.
    Polygons(Vec<Vec<(i32, i32)>>),
    /// The opaque part of a picture stretched over the window.
    Picture(Paint),
    /// Where the mode's images are at least half opaque, at any size.
    Pictures,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Dim {
    Fixed(i32),
    /// Resizable from `min` in steps of `step`.
    Free {
        min: i32,
        step: i32,
    },
}

impl Dim {
    /// The nearest allowed length to `want`.
    pub fn snap(self, want: i32) -> i32 {
        match self {
            Dim::Fixed(n) => n,
            Dim::Free { min, step } => {
                let steps = ((want - min) as f32 / step.max(1) as f32).round().max(0.0) as i32;
                min + steps * step
            }
        }
    }
}

#[derive(Debug, Clone)]
pub struct Element {
    pub at: Place,
    pub kind: Kind,
    /// The element is drawn and takes the mouse only while this holds.
    pub show: Cond,
    /// Overrides the mode's cursor over this element.
    pub cursor: Option<CursorKind>,
    pub tip: Option<String>,
    /// Right-click menu; the window's main menu if none.
    pub menu: Option<MenuRef>,
    /// Takes the mouse only where this picture is opaque (a round
    /// button); the whole rectangle if none.
    pub hit: Option<Paint>,
}

impl Element {
    pub fn new(at: Place, kind: Kind) -> Element {
        Element { at, kind, show: Cond::Always, cursor: None, tip: None, menu: None, hit: None }
    }

    pub fn show(mut self, c: Cond) -> Element {
        self.show = c;
        self
    }

    pub fn cursor(mut self, c: CursorKind) -> Element {
        self.cursor = Some(c);
        self
    }

    pub fn tip(mut self, t: &str) -> Element {
        self.tip = Some(t.to_string());
        self
    }

    pub fn menu(mut self, m: MenuRef) -> Element {
        self.menu = Some(m);
        self
    }

    /// Takes part in mouse handling (the rest only draw).
    pub fn interactive(&self) -> bool {
        !matches!(self.kind, Kind::Image { .. } | Kind::Text { .. } | Kind::Clock { .. } | Kind::Vis | Kind::EqGraph)
    }
}

/// An element's rectangle, with each edge anchored to a window edge.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Place {
    pub left: Pos,
    pub top: Pos,
    pub right: Pos,
    pub bottom: Pos,
}

/// A coordinate measured from the window's start, centre or end.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Pos {
    Start(i32),
    Center(i32),
    End(i32),
}

impl Pos {
    pub fn resolve(self, len: i32) -> i32 {
        match self {
            Pos::Start(o) => o,
            Pos::Center(o) => len / 2 + o,
            Pos::End(o) => len + o,
        }
    }
}

impl Place {
    /// A rectangle that keeps its place when the window is resized.
    pub const fn fixed(x: i32, y: i32, w: i32, h: i32) -> Place {
        Place { left: Pos::Start(x), top: Pos::Start(y), right: Pos::Start(x + w), bottom: Pos::Start(y + h) }
    }

    pub fn resolve(&self, w: i32, h: i32) -> Rect {
        let (x0, y0) = (self.left.resolve(w), self.top.resolve(h));
        Rect { x: x0, y: y0, w: self.right.resolve(w) - x0, h: self.bottom.resolve(h) - y0 }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Picture {
    Sheet(Sheet),
    /// A vector skin's SVG file, by its place in the skin's list.
    Svg(u16),
}

/// Area `src` of a picture, drawn at the element's top-left corner plus
/// `(dx, dy)`. For an SVG, `src` is in the SVG's own units.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Paint {
    pub picture: Picture,
    pub src: Rect,
    pub dx: i32,
    pub dy: i32,
}

impl Paint {
    pub const fn new(sheet: Sheet, src: Rect) -> Paint {
        Paint { picture: Picture::Sheet(sheet), src, dx: 0, dy: 0 }
    }

    pub const fn offset(self, dx: i32, dy: i32) -> Paint {
        Paint { dx, dy, ..self }
    }
}

/// An element's paints by state; a missing paint draws nothing.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Look {
    pub normal: Option<Paint>,
    pub pressed: Option<Paint>,
    pub on: Option<Paint>,
    pub on_pressed: Option<Paint>,
    /// Used instead of `normal` while the window is not focused.
    pub inactive: Option<Paint>,
}

impl Look {
    pub fn plain(p: Paint) -> Look {
        Look { normal: Some(p), ..Look::default() }
    }

    pub fn button(normal: Paint, pressed: Paint) -> Look {
        Look { normal: Some(normal), pressed: Some(pressed), ..Look::default() }
    }

    pub fn toggle(off: Paint, off_pressed: Paint, on: Paint, on_pressed: Paint) -> Look {
        Look { normal: Some(off), pressed: Some(off_pressed), on: Some(on), on_pressed: Some(on_pressed), inactive: None }
    }

    pub fn focus(active: Paint, inactive: Paint) -> Look {
        Look { normal: Some(active), inactive: Some(inactive), ..Look::default() }
    }

    pub fn pick(&self, on: bool, pressed: bool, active: bool) -> Option<Paint> {
        match (on, pressed) {
            (true, true) => self.on_pressed.or(self.on),
            (true, false) => self.on,
            (false, true) => self.pressed.or(self.normal),
            (false, false) if !active => self.inactive.or(self.normal),
            (false, false) => self.normal,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Fill {
    Once,
    /// Repeats the paint across the element, clipped to it.
    Tile,
}

/// Where a text sits across its element.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Align {
    #[default]
    Left,
    Center,
    Right,
}

#[derive(Debug, Clone)]
pub enum Kind {
    /// Decoration; `lit` switches to the `on` paint.
    Image {
        look: Look,
        fill: Fill,
        lit: Option<Flag>,
    },
    /// Moves the window; a double click runs `double`.
    Drag {
        double: Option<Command>,
    },
    Button {
        look: Look,
        action: Action,
    },
    /// A button that shows `flag` as its on state.
    Toggle {
        look: Look,
        flag: Flag,
        action: Action,
    },
    Slider(Slider),
    /// Text in the skin's bitmap font, clipped to the element.
    Text(Text),
    /// The song title, scrolling and draggable, drawn in `area` (relative
    /// to the element), or in all of it at any size.
    Marquee {
        area: Option<Rect>,
        double: Option<Command>,
    },
    /// The big `m:ss` play time in the skin's digits; `align` places a
    /// font's time across the element (the classic digits have fixed places).
    Clock {
        align: Align,
        /// The font's height here, where it is not the skin's `[clock]` size.
        font_size: Option<i32>,
    },
    Vis,
    /// The equalizer curve.
    EqGraph,
    /// The playlist's track list.
    List,
    /// The media library: its views, search field, panes, track table
    /// and buttons, which the player lays out and draws in the playlist's
    /// colours. Each of its lists has a scroll bar as wide as `thumb`,
    /// whose picture is the bar's thumb; without one the player draws a
    /// narrow bar of its own.
    Library {
        thumb: Look,
    },
    /// The vertical scroll bar of the playlist's list.
    ListScroll {
        thumb: Look,
    },
    /// A button that opens a strip of picture buttons stacked upward from
    /// its own top-left corner; `bar` is drawn beside them.
    Popup {
        items: Vec<PopupItem>,
        bar: Option<Paint>,
        /// The column starts above the button instead of covering it.
        above: bool,
    },
    /// Resizes the window.
    Resize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PopupItem {
    /// `normal`, and `pressed` while the mouse is over it.
    pub look: Look,
    pub action: Action,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    /// Runs on release over the element; Shift and Ctrl pick alternatives.
    Click { plain: Command, shift: Option<Command>, ctrl: Option<Command> },
    /// Runs as soon as the button goes down.
    Press(Command),
    /// Opens a menu as soon as the button goes down.
    MenuOnPress(MenuRef),
    /// Opens a menu on release over the element.
    MenuOnRelease(MenuRef),
    /// Only shows the pressed look.
    Nothing,
}

impl Action {
    pub const fn click(c: Command) -> Action {
        Action::Click { plain: c, shift: None, ctrl: None }
    }
}

#[derive(Debug, Clone)]
pub struct Slider {
    pub value: Value,
    pub vertical: bool,
    /// Background that changes with the value.
    pub track: Option<Frames>,
    pub thumb: Look,
    /// Thumb length along the slider, also when the skin has no thumb.
    pub thumb_len: i32,
    /// Changes the value while dragging; otherwise only on release.
    pub live: bool,
    /// Value a double click sets.
    pub reset: Option<i64>,
    /// Value change per wheel step; 0 leaves the wheel to the window.
    pub wheel: i64,
    /// Values this close to the middle of the range snap to it.
    pub detent: i64,
}

/// A grid of background pictures, one per step of the value.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Frames {
    pub picture: Picture,
    pub first: Rect,
    pub step: (i32, i32),
    pub count: i32,
    pub per_row: i32,
    pub map: FrameMap,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FrameMap {
    /// First frame at the minimum.
    Rising,
    /// First frame at the maximum.
    Falling,
    /// First frame in the middle, last at either end.
    FromMiddle,
}

impl Frames {
    /// The frame for `v` in `min..=max`, rounded to the nearest.
    pub fn frame(&self, v: i64, min: i64, max: i64) -> Rect {
        let (n, d) = match self.map {
            FrameMap::Rising => (v - min, max - min),
            FrameMap::Falling => (max - v, max - min),
            FrameMap::FromMiddle => ((2 * v - min - max).abs(), max - min),
        };
        let d = d.max(1);
        let last = (self.count - 1).max(0) as i64;
        let k = ((n.clamp(0, d) * last + d / 2) / d) as i32;
        let per_row = self.per_row.max(1);
        Rect {
            x: self.first.x + (k % per_row) * self.step.0,
            y: self.first.y + (k / per_row) * self.step.1,
            w: self.first.w,
            h: self.first.h,
        }
    }
}

/// When an element is shown.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Cond {
    Always,
    If(Flag),
    Unless(Flag),
    /// The window is at least this wide.
    MinWidth(i32),
}

/// A yes/no state of the player.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Flag {
    Playing,
    Paused,
    Stopped,
    /// Playing or paused a track of known length.
    Seekable,
    /// Playing or paused, with two or more channels.
    Stereo,
    /// Playing or paused, with one channel.
    Mono,
    Shuffle,
    Repeat,
    EqWindow,
    PlaylistWindow,
    LibraryWindow,
    AlwaysOnTop,
    DoubleSize,
    EqOn,
    EqAutoload,
}

/// A number the player shows and lets the user change.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Value {
    Volume,
    Balance,
    /// Play position.
    Position,
    Preamp,
    EqBand(u8),
}

/// A text the player shows.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Text {
    Kbps,
    Khz,
    /// `m:ss` for the shaded main window.
    ShortTime,
    Title,
    /// Selected and total length of the playlist.
    PlaylistTime,
    /// Play time for the playlist's corner.
    PlaylistClock,
    /// The media library's name, for a title bar without one.
    LibraryTitle,
    /// How many tracks the media library shows, or how its scan goes.
    LibraryStatus,
}

/// A menu the player builds.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MenuRef {
    Main,
    Options,
    Vis,
    EqPresets,
    PlaylistRow,
    PlaylistMisc,
    PlaylistSort,
    PlaylistRemoveMisc,
    /// What can be done with the media library and the tracks picked in it.
    Library,
    Previous,
    Play,
    Pause,
    Stop,
    Next,
    Eject,
    Seek,
    Repeat,
    Shuffle,
    Time,
    SongTitle,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn places_follow_their_anchors() {
        let p = Place { left: Pos::Start(12), top: Pos::Start(20), right: Pos::End(-20), bottom: Pos::End(-38) };
        assert_eq!(p.resolve(275, 232), Rect { x: 12, y: 20, w: 243, h: 174 });
        assert_eq!(p.resolve(450, 116), Rect { x: 12, y: 20, w: 418, h: 58 });
        let title = Place { left: Pos::Center(-50), top: Pos::Start(0), right: Pos::Center(50), bottom: Pos::Start(20) };
        assert_eq!(title.resolve(275, 116).x, (275 - 100) / 2);
        assert_eq!(Place::fixed(3, 4, 5, 6).resolve(999, 999), Rect { x: 3, y: 4, w: 5, h: 6 });
    }

    #[test]
    fn free_dimensions_snap_to_steps() {
        let d = Dim::Free { min: 275, step: 25 };
        assert_eq!(d.snap(100), 275);
        assert_eq!(d.snap(287), 275);
        assert_eq!(d.snap(288), 300);
        assert_eq!(Dim::Fixed(14).snap(500), 14);
    }

    fn p(x: i32) -> Paint {
        Paint::new(Sheet::Main, Rect { x, y: 0, w: 1, h: 1 })
    }

    #[test]
    fn looks_fall_back_sensibly() {
        let t = Look::toggle(p(0), p(1), p(2), p(3));
        assert_eq!(
            [t.pick(false, false, true), t.pick(false, true, true), t.pick(true, false, true), t.pick(true, true, true)],
            [Some(p(0)), Some(p(1)), Some(p(2)), Some(p(3))]
        );
        let title = Look::focus(p(0), p(1));
        assert_eq!(title.pick(false, false, false), Some(p(1)));
        assert_eq!(title.pick(false, true, false), Some(p(0)), "pressed shows the normal paint");
        let overlay = Look { pressed: Some(p(5)), ..Look::default() };
        assert_eq!(overlay.pick(false, false, true), None);
        assert_eq!(overlay.pick(false, true, true), Some(p(5)));
        let b = Look::button(p(0), p(1));
        assert_eq!(b.pick(false, false, false), Some(p(0)), "no inactive paint: normal");
    }

    fn frames(map: FrameMap) -> Frames {
        Frames {
            picture: Picture::Sheet(Sheet::Volume),
            first: Rect { x: 0, y: 0, w: 68, h: 13 },
            step: (0, 15),
            count: 28,
            per_row: 1,
            map,
        }
    }

    #[test]
    fn frames_follow_the_value() {
        // The classic formulas: volume (v*27+127)/255, balance (|b|*27+63)/127,
        // equalizer ((63-v)*27+31)/63.
        for v in 0..=255 {
            assert_eq!(frames(FrameMap::Rising).frame(v, 0, 255).y, (v as i32 * 27 + 127) / 255 * 15);
        }
        for b in -127..=127i64 {
            assert_eq!(frames(FrameMap::FromMiddle).frame(b, -127, 127).y, (b.abs() as i32 * 27 + 63) / 127 * 15, "{b}");
        }
        let eq = Frames { first: Rect { x: 13, y: 164, w: 14, h: 63 }, step: (15, 65), per_row: 14, ..frames(FrameMap::Falling) };
        for v in 0..=63 {
            let k = ((63 - v as i32) * 27 + 31) / 63;
            let want = (13 + (k % 14) * 15, 164 + (k / 14) * 65);
            let got = eq.frame(v, 0, 63);
            assert_eq!((got.x, got.y), want, "{v}");
        }
    }
}
