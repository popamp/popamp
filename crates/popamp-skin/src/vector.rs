//! Vector skins: a folder or zip archive with `skin.toml` and SVG
//! pictures. The file describes the windows and elements of a [`Layout`];
//! docs/skin-format.md explains it for skin authors.

use crate::colors::{Palette, SetsFile};
use crate::cursors::CursorKind;
use crate::layout::*;
use crate::names::{Named, N};
use crate::sprites::{Rect, Sheet};
use crate::Skin;
use popamp_core::config::BarEdge;
use popamp_core::Command;
use serde::de::{self, Deserializer};
use serde::Deserialize;
use std::collections::HashMap;
use toml::Spanned;

pub const FILE: &str = "skin.toml";
const FORMAT_VERSION: u32 = 1;

/// Builds a skin from its files (path inside the skin -> bytes). Windows
/// the file leaves out come from the built-in skin.
/// `colors` is the colour set wanted, by name.
pub fn load(files: &HashMap<String, Vec<u8>>, colors: Option<&str>) -> Result<Skin, String> {
    load_onto(Skin::builtin_vector(), files, colors)
}

/// Builds a skin over `base`, which gives what the file leaves out.
pub(crate) fn load_onto(base: Skin, files: &HashMap<String, Vec<u8>>, colors: Option<&str>) -> Result<Skin, String> {
    let text = files.get(FILE).ok_or(format!("no {FILE}"))?;
    let text = std::str::from_utf8(text).map_err(|e| format!("{FILE}: not UTF-8 text: {e}"))?;
    check_head(text)?;
    let file: SkinFile = toml::from_str(text).map_err(|e| at(text, e.span(), e.message()))?;
    let palette = Palette::read(&file.colors, colors).map_err(|(span, msg)| at(text, Some(span), &msg))?;
    let mut build = Builder { text, files, skin: base, svg_ids: HashMap::new(), css: palette.css(), palette };
    build.skin.name = file.meta.name.clone();
    build.skin.color_sets = build.palette.sets.clone();
    build.skin.color_set = build.palette.chosen;
    build.skin.vector = true;
    build.sheets(&file.sheets)?;
    build.shapes(&file.shapes)?;
    build.texts(&file)?;
    for w in &file.windows {
        build.window(w)?;
    }
    build.skin.layout.windows.sort_by_key(|w| w.key.index());
    Ok(build.skin)
}

/// Whether this player may read the file at all, from `min_popamp` and
/// `format_version`, read before the rest and leniently: a skin for a newer
/// Popamp is refused for that, not for a key this one does not know. A file
/// this cannot read gets its error from the full reading.
fn check_head(text: &str) -> Result<(), String> {
    let Ok(Head { meta: Some(meta) }) = toml::from_str::<Head>(text) else { return Ok(()) };
    if let Some(min) = meta.min_popamp {
        popamp_core::version::check_min(min.get_ref()).map_err(|e| at(text, Some(min.span()), &e))?;
    }
    match meta.format_version {
        Some(v) if *v.get_ref() != FORMAT_VERSION => {
            Err(at(text, Some(v.span()), &format!("this player reads format_version {FORMAT_VERSION}")))
        }
        _ => Ok(()),
    }
}

#[derive(Deserialize)]
struct Head {
    meta: Option<HeadMeta>,
}

#[derive(Deserialize)]
struct HeadMeta {
    min_popamp: Option<Spanned<String>>,
    format_version: Option<Spanned<u32>>,
}

/// `skin.toml, line 12: message`.
fn at(text: &str, span: Option<std::ops::Range<usize>>, msg: &str) -> String {
    match span {
        Some(s) => format!("{FILE}, line {}: {msg}", text[..s.start.min(text.len())].matches('\n').count() + 1),
        None => format!("{FILE}: {msg}"),
    }
}

// --- the file -------------------------------------------------------------

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct SkinFile {
    meta: Meta,
    /// SVG drawings of classic skin sheets (`main = "main.svg"`), drawn
    /// with the classic layout wherever the sheet would be.
    #[serde(default)]
    sheets: HashMap<String, Spanned<String>>,
    /// Window shapes for the classic layout (`main`, `main_shade`, `eq`...).
    #[serde(default)]
    shapes: HashMap<String, Spanned<PaintFile>>,
    /// Colour sets: named colours for the pictures and the keys below.
    #[serde(default)]
    colors: SetsFile,
    text: Option<TextFile>,
    clock: Option<TextFile>,
    playlist: Option<PlaylistFile>,
    vis: Option<VisFile>,
    #[serde(default, rename = "window")]
    windows: Vec<WindowFile>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Meta {
    name: String,
    #[serde(default)]
    #[allow(dead_code)] // Shown by the skin browser later.
    author: String,
    #[allow(dead_code)] // Checked by `check_head`.
    format_version: Spanned<u32>,
    #[serde(default)]
    #[allow(dead_code)] // Checked by `check_head`.
    min_popamp: Option<String>,
}

/// A text style; without `font` the system's sans-serif font is used.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct TextFile {
    font: Option<Spanned<String>>,
    size: i32,
    color: ColorFile,
}

/// Playlist colours; the classic ones stand in for those left out.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct PlaylistFile {
    font: Option<Spanned<String>>,
    normal: Option<ColorFile>,
    current: Option<ColorFile>,
    normal_bg: Option<ColorFile>,
    selected_bg: Option<ColorFile>,
}

/// The visualization colours in classic `viscolor.txt` order: background,
/// dots, 16 spectrum colours from the top, 5 scope colours, peaks.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct VisFile {
    colors: [ColorFile; 24],
}

/// A colour written `#rrggbb` or by its name in the skin's colour sets.
type ColorFile = Spanned<String>;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct WindowFile {
    key: N<WindowKey>,
    /// `free` when left out.
    placement: Option<N<Placement>>,
    /// `move` when left out.
    empty_press: Option<N<EmptyPress>>,
    /// Volume change per wheel step over the window; 8 when left out.
    wheel_volume: Option<i32>,
    /// The wheel scrolls the window's list instead.
    wheel_scrolls_list: Option<bool>,
    /// Without it the window keeps the classic layout, and only `bar` can
    /// be given.
    normal: Option<ModeFile>,
    shade: Option<ModeFile>,
    /// The main window as a bar, which the user can switch to.
    bar: Option<Spanned<ModeFile>>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ModeFile {
    /// The screen edge a bar is at by default; for `[window.bar]` only.
    edge: Option<Spanned<N<BarEdge>>>,
    size: [i32; 2],
    /// Makes the window resizable in these steps.
    resize_step: Option<[i32; 2]>,
    cursor: Option<N<CursorKind>>,
    /// A picture whose opaque part is the visible window.
    shape: Option<Spanned<PaintFile>>,
    /// The window shows only where its images are opaque.
    #[serde(default)]
    see_through: bool,
    #[serde(default, rename = "element")]
    elements: Vec<Spanned<ElementFile>>,
}

/// Every field an element may have; which apply depends on its `type`.
#[derive(Deserialize, Default)]
#[serde(deny_unknown_fields)]
struct ElementFile {
    #[serde(rename = "type")]
    kind: Option<String>,
    rect: Option<[i32; 4]>,
    edges: Option<[PosFile; 4]>,
    // Pictures by state.
    normal: Option<Spanned<PaintFile>>,
    pressed: Option<Spanned<PaintFile>>,
    on: Option<Spanned<PaintFile>>,
    on_pressed: Option<Spanned<PaintFile>>,
    inactive: Option<Spanned<PaintFile>>,
    fill: Option<N<Fill>>,
    lit: Option<N<Flag>>,
    // What it does.
    click: Option<N<Command>>,
    shift: Option<N<Command>>,
    ctrl: Option<N<Command>>,
    press: Option<N<Command>>,
    menu_press: Option<N<MenuRef>>,
    menu_release: Option<N<MenuRef>>,
    double: Option<N<Command>>,
    flag: Option<N<Flag>>,
    // Sliders.
    value: Option<N<Value>>,
    vertical: Option<bool>,
    track: Option<Spanned<FramesFile>>,
    thumb: Option<LookFile>,
    thumb_len: Option<i32>,
    live: Option<bool>,
    reset: Option<i64>,
    wheel_step: Option<i64>,
    detent: Option<i64>,
    // Texts.
    text: Option<N<Text>>,
    area: Option<[i32; 4]>,
    align: Option<N<Align>>,
    font_size: Option<i32>,
    // Popups.
    #[serde(default, rename = "item")]
    items: Vec<ItemFile>,
    bar: Option<Spanned<PaintFile>>,
    #[serde(default)]
    above: bool,
    // When and how it shows.
    show: Option<N<Flag>>,
    hide: Option<N<Flag>>,
    min_width: Option<i32>,
    cursor: Option<N<CursorKind>>,
    tip: Option<String>,
    menu: Option<N<MenuRef>>,
    hit: Option<Spanned<PaintFile>>,
}

impl ElementFile {
    /// The fields that are set, besides the ones every element may have.
    fn set_fields(&self) -> Vec<&'static str> {
        let fields = [
            ("normal", self.normal.is_some()),
            ("pressed", self.pressed.is_some()),
            ("on", self.on.is_some()),
            ("on_pressed", self.on_pressed.is_some()),
            ("inactive", self.inactive.is_some()),
            ("fill", self.fill.is_some()),
            ("lit", self.lit.is_some()),
            ("click", self.click.is_some()),
            ("shift", self.shift.is_some()),
            ("ctrl", self.ctrl.is_some()),
            ("press", self.press.is_some()),
            ("menu_press", self.menu_press.is_some()),
            ("menu_release", self.menu_release.is_some()),
            ("double", self.double.is_some()),
            ("flag", self.flag.is_some()),
            ("value", self.value.is_some()),
            ("vertical", self.vertical.is_some()),
            ("track", self.track.is_some()),
            ("thumb", self.thumb.is_some()),
            ("thumb_len", self.thumb_len.is_some()),
            ("live", self.live.is_some()),
            ("reset", self.reset.is_some()),
            ("wheel_step", self.wheel_step.is_some()),
            ("detent", self.detent.is_some()),
            ("text", self.text.is_some()),
            ("area", self.area.is_some()),
            ("align", self.align.is_some()),
            ("font_size", self.font_size.is_some()),
            ("item", !self.items.is_empty()),
            ("bar", self.bar.is_some()),
        ];
        fields.into_iter().filter(|f| f.1).map(|f| f.0).collect()
    }
}

/// The fields each element type uses (besides rect, edges, show, hide,
/// min_width, cursor, tip, menu and hit).
const KINDS: &[(&str, &[&str])] = &[
    ("image", &["normal", "on", "inactive", "fill", "lit"]),
    ("drag", &["double"]),
    ("button", &["normal", "pressed", "inactive", "click", "shift", "ctrl", "press", "menu_press", "menu_release"]),
    ("toggle", &["normal", "pressed", "on", "on_pressed", "flag", "click", "shift", "ctrl", "press", "menu_press", "menu_release"]),
    ("slider", &["value", "vertical", "track", "thumb", "thumb_len", "live", "reset", "wheel_step", "detent"]),
    ("text", &["text"]),
    ("marquee", &["area", "double"]),
    ("clock", &["align", "font_size"]),
    ("vis", &[]),
    ("eq-graph", &[]),
    ("list", &[]),
    ("list-scroll", &["thumb"]),
    ("popup", &["item", "bar"]),
    ("resize", &[]),
];

#[derive(Deserialize, Default)]
#[serde(deny_unknown_fields)]
struct LookFile {
    normal: Option<Spanned<PaintFile>>,
    pressed: Option<Spanned<PaintFile>>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ItemFile {
    normal: Option<Spanned<PaintFile>>,
    pressed: Option<Spanned<PaintFile>>,
    click: Option<N<Command>>,
    shift: Option<N<Command>>,
    ctrl: Option<N<Command>>,
    press: Option<N<Command>>,
    menu_press: Option<N<MenuRef>>,
    menu_release: Option<N<MenuRef>>,
}

/// A whole SVG file, or an area of it shifted by `offset`.
#[derive(Deserialize)]
#[serde(untagged, expecting = "a file name, or a table with `file` and optional `area` and `offset`")]
enum PaintFile {
    Whole(String),
    Part { file: String, area: Option<[i32; 4]>, offset: Option<[i32; 2]> },
}

/// One picture per value step, laid out in a grid in one SVG file.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct FramesFile {
    file: String,
    first: [i32; 4],
    #[serde(default)]
    step: [i32; 2],
    #[serde(default = "one")]
    count: i32,
    #[serde(default = "one")]
    per_row: i32,
    #[serde(default = "rising")]
    map: N<FrameMap>,
}

fn one() -> i32 {
    1
}

fn rising() -> N<FrameMap> {
    N(FrameMap::Rising)
}

/// An edge: a number counts from the window's start, `"end-20"` from its
/// end, `"center+5"` from its middle.
struct PosFile(Pos);

impl<'de> Deserialize<'de> for PosFile {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        struct V;
        impl de::Visitor<'_> for V {
            type Value = PosFile;
            fn expecting(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
                f.write_str("a number, or \"end-N\" or \"center+N\"")
            }
            fn visit_i64<E: de::Error>(self, v: i64) -> Result<PosFile, E> {
                Ok(PosFile(Pos::Start(v as i32)))
            }
            fn visit_str<E: de::Error>(self, v: &str) -> Result<PosFile, E> {
                let (from, rest) = v.split_at(v.find(['+', '-']).unwrap_or(v.len()));
                let n: i32 = if rest.is_empty() {
                    0
                } else {
                    rest.trim_start_matches('+').parse().map_err(|_| E::custom(format!("bad offset in `{v}`")))?
                };
                match from {
                    "end" => Ok(PosFile(Pos::End(n))),
                    "center" => Ok(PosFile(Pos::Center(n))),
                    _ => Err(E::custom(format!("`{v}`: an edge is a number, or \"end-N\" or \"center+N\""))),
                }
            }
        }
        d.deserialize_any(V)
    }
}

// --- from the file to the layout -------------------------------------------

/// A mistake in the file, and where it is when that is more exact than
/// the element it is in.
struct Fault {
    span: Option<std::ops::Range<usize>>,
    msg: String,
}

impl From<String> for Fault {
    fn from(msg: String) -> Fault {
        Fault { span: None, msg }
    }
}

impl From<&str> for Fault {
    fn from(msg: &str) -> Fault {
        msg.to_string().into()
    }
}

struct Builder<'a> {
    text: &'a str,
    files: &'a HashMap<String, Vec<u8>>,
    skin: Skin,
    svg_ids: HashMap<String, u16>,
    palette: Palette,
    /// The colour set as a style sheet for the pictures.
    css: Option<String>,
}

impl Builder<'_> {
    fn sheets(&mut self, sheets: &HashMap<String, Spanned<String>>) -> Result<(), String> {
        for (name, file) in sheets {
            let fail = |msg: String| at(self.text, Some(file.span()), &msg);
            let sheet = Sheet::all().into_iter().find(|s| s.name() == *name).ok_or_else(|| {
                let names: Vec<_> = Sheet::all().into_iter().map(Sheet::name).collect();
                fail(format!("unknown sheet `{name}`; it can be one of: {}", names.join(", ")))
            })?;
            let (id, (w, h)) = self.svg(file.get_ref()).map_err(fail)?;
            self.skin.sheet_svgs.insert(sheet, id);
            // The x1 picture too, for what reads sheet pixels directly.
            if let Some(r) = self.skin.svgs.raster(id, Rect { x: 0, y: 0, w, h }, w, h) {
                let mut b = crate::Bitmap::new(w, h, 0);
                b.overlay(&r, 0, 0, (0, 0, w, h));
                self.skin.sheets.insert(sheet, b);
            }
        }
        self.skin = std::mem::replace(&mut self.skin, Skin::bare()).with_layout(crate::Regions::default());
        Ok(())
    }

    fn shapes(&mut self, shapes: &HashMap<String, Spanned<PaintFile>>) -> Result<(), String> {
        for (name, file) in shapes {
            let fail = |msg: &str| at(self.text, Some(file.span()), msg);
            let (key, shade) = match name.strip_suffix("_shade") {
                Some(k) => (k, true),
                None => (name.as_str(), false),
            };
            let key = WindowKey::all().into_iter().find(|w| w.name() == key);
            let window = key.and_then(|k| self.skin.layout.windows.iter().position(|w| w.key == k));
            let Some(window) = window else {
                return Err(fail(&format!("unknown window `{name}`; it can be main, eq, playlist, or one of them with _shade")));
            };
            let paint = self.paint(Some(file)).map_err(|e| at(self.text, e.span, &e.msg))?.map(Shape::Picture);
            let w = &mut self.skin.layout.windows[window];
            match (shade, &mut w.shade) {
                (false, _) => w.normal.shape = paint,
                (true, Some(mode)) => mode.shape = paint,
                (true, None) => return Err(fail(&format!("window `{}` has no shade mode", key.map(Named::name).unwrap_or_default()))),
            }
        }
        Ok(())
    }

    fn texts(&mut self, file: &SkinFile) -> Result<(), String> {
        self.skin.text = file.text.as_ref().map(|t| self.style(t)).transpose()?;
        self.skin.clock = file.clock.as_ref().map(|t| self.style(t)).transpose()?;
        if let Some(pl) = &file.playlist {
            let classic = crate::PlColors::default();
            self.skin.pl_font = pl.font.as_ref().map(|f| self.font(f)).transpose()?;
            let pick = |c: &Option<ColorFile>, or: u32| c.as_ref().map_or(Ok(or), |c| self.color(c));
            self.skin.pl = crate::PlColors {
                normal: pick(&pl.normal, classic.normal)?,
                current: pick(&pl.current, classic.current)?,
                normal_bg: pick(&pl.normal_bg, classic.normal_bg)?,
                selected_bg: pick(&pl.selected_bg, classic.selected_bg)?,
                font: "sans-serif".into(),
            };
        }
        if let Some(vis) = &file.vis {
            for (i, c) in vis.colors.iter().enumerate() {
                self.skin.viscolor[i] = self.color(c)?;
            }
        }
        Ok(())
    }

    fn style(&self, t: &TextFile) -> Result<crate::TextStyle, String> {
        Ok(crate::TextStyle { font: t.font.as_ref().map(|f| self.font(f)).transpose()?, size: t.size, color: self.color(&t.color)? })
    }

    fn color(&self, c: &ColorFile) -> Result<u32, String> {
        self.palette.color(c.get_ref()).map_err(|msg| at(self.text, Some(c.span()), &msg))
    }

    /// The bytes of a font file in the skin, checked to be a font.
    fn font(&self, file: &Spanned<String>) -> Result<Vec<u8>, String> {
        let fail = |msg: String| at(self.text, Some(file.span()), &msg);
        let name = file.get_ref();
        let data = self.files.get(name).ok_or_else(|| fail(format!("no file `{name}` in the skin")))?;
        fontdue::Font::from_bytes(data.as_slice(), fontdue::FontSettings::default())
            .map_err(|e| fail(format!("{name}: not a font: {e}")))?;
        Ok(data.clone())
    }

    /// Puts the window into the layout: in place of the one it describes
    /// anew, or as a bar view added to the classic one.
    fn window(&mut self, w: &WindowFile) -> Result<(), String> {
        let name = w.key.0.name();
        let bar = w.bar.as_ref().map(|b| self.bar(w, b)).transpose()?;
        let Some(normal) = &w.normal else {
            let described = w.placement.is_some() || w.empty_press.is_some() || w.wheel_volume.is_some() || w.wheel_scrolls_list.is_some();
            if bar.is_none() || described || w.shade.is_some() {
                return Err(format!("{name}: a window needs `normal`; without it only `bar` can be given, beside the classic layout"));
            }
            let classic = self.skin.layout.windows.iter_mut().find(|old| old.key == w.key.0);
            classic.ok_or(format!("{name}: there is no classic window to add a bar to"))?.bar = bar;
            return Ok(());
        };
        let mut normal = self.mode(normal)?;
        let placement = w.placement.map_or(Placement::Free, |p| p.0);
        if let Placement::Bar(_) = placement {
            if w.key.0 != WindowKey::Main {
                return Err(format!("{name}: only the main window can be a bar"));
            }
            normal = spanning_the_screen(normal);
        }
        let def = WindowDef {
            key: w.key.0,
            placement,
            normal,
            shade: w.shade.as_ref().map(|m| self.mode(m)).transpose()?,
            bar,
            empty_press: w.empty_press.map_or(EmptyPress::Move, |e| e.0),
            wheel: match w.wheel_scrolls_list {
                Some(true) => Wheel::ScrollList,
                _ => Wheel::Volume(w.wheel_volume.unwrap_or(8)),
            },
        };
        self.skin.layout.windows.retain(|old| old.key != def.key);
        self.skin.layout.windows.push(def);
        Ok(())
    }

    /// The bar view of a window that is also described as a window.
    fn bar(&mut self, w: &WindowFile, b: &Spanned<ModeFile>) -> Result<Bar, String> {
        let fail = |msg: &str| Err(at(self.text, Some(b.span()), msg));
        if w.key.0 != WindowKey::Main {
            return fail("only the main window can be a bar");
        }
        if let Some(N(Placement::Bar(_))) = w.placement {
            return fail("this window is always a bar by its `placement`; describe the bar in `normal`");
        }
        let edge = b.get_ref().edge.as_ref().map_or(BarEdge::Bottom, |e| e.get_ref().0);
        Ok(Bar { edge, mode: spanning_the_screen(self.view(b.get_ref())?) })
    }

    /// A normal or shade mode, which has no screen edge.
    fn mode(&mut self, m: &ModeFile) -> Result<Mode, String> {
        match &m.edge {
            Some(e) => Err(at(self.text, Some(e.span()), "`edge` is for `[window.bar]` only")),
            None => self.view(m),
        }
    }

    fn view(&mut self, m: &ModeFile) -> Result<Mode, String> {
        let dim = |i: usize| match m.resize_step {
            Some(step) => Dim::Free { min: m.size[i], step: step[i] },
            None => Dim::Fixed(m.size[i]),
        };
        let elements = m.elements.iter().map(|e| self.element(e)).collect::<Result<_, _>>()?;
        let shape = match (self.paint(m.shape.as_ref()).map_err(|e| at(self.text, e.span, &e.msg))?, m.see_through) {
            (Some(_), true) => return Err(at(self.text, m.shape.as_ref().map(|s| s.span()), "use `shape` or `see_through`, not both")),
            (Some(p), false) => Some(Shape::Picture(p)),
            (None, true) => Some(Shape::Pictures),
            (None, false) => None,
        };
        Ok(Mode { width: dim(0), height: dim(1), cursor: m.cursor.map_or(CursorKind::Normal, |c| c.0), elements, shape })
    }

    fn element(&mut self, e: &Spanned<ElementFile>) -> Result<Element, String> {
        let fail = |msg: String| at(self.text, Some(e.span()), &msg);
        let f = e.get_ref();
        let kind_name = check_fields(f).map_err(fail)?;
        let place = place(f).map_err(fail)?;
        let kind = self.kind(kind_name, f).map_err(|x| at(self.text, x.span.or(Some(e.span())), &x.msg))?;
        let show = show(f).map_err(fail)?;
        let hit = self.paint(f.hit.as_ref()).map_err(|x| at(self.text, x.span, &x.msg))?;
        Ok(Element { at: place, kind, show, cursor: f.cursor.map(|c| c.0), tip: f.tip.clone(), menu: f.menu.map(|m| m.0), hit })
    }

    fn kind(&mut self, name: &str, f: &ElementFile) -> Result<Kind, Fault> {
        Ok(match name {
            "image" | "button" | "toggle" => self.pictured(name, f)?,
            "slider" => Kind::Slider(self.slider(f)?),
            "text" => Kind::Text(f.text.ok_or("a text needs `text`: what it shows")?.0),
            "marquee" => Kind::Marquee { area: f.area.map(|[x, y, w, h]| Rect { x, y, w, h }), double: f.double.map(|x| x.0) },
            "list-scroll" => Kind::ListScroll { thumb: self.thumb(f)? },
            "popup" => {
                let items = f.items.iter().map(|i| self.item(i)).collect::<Result<_, _>>()?;
                Kind::Popup { items, bar: self.paint(f.bar.as_ref())?, above: f.above }
            }
            "drag" => Kind::Drag { double: f.double.map(|x| x.0) },
            "clock" => Kind::Clock { align: f.align.map_or(Align::Left, |a| a.0), font_size: f.font_size },
            "vis" => Kind::Vis,
            "eq-graph" => Kind::EqGraph,
            "list" => Kind::List,
            _ => Kind::Resize,
        })
    }

    /// Images, buttons and toggles: elements drawn from pictures by state.
    fn pictured(&mut self, name: &str, f: &ElementFile) -> Result<Kind, Fault> {
        let look = self.look(f)?;
        if name == "image" {
            return Ok(Kind::Image { look, fill: f.fill.map_or(Fill::Once, |x| x.0), lit: f.lit.map(|x| x.0) });
        }
        let action = action(f.click, f.shift, f.ctrl, f.press, f.menu_press, f.menu_release)?;
        Ok(match f.flag {
            Some(flag) => Kind::Toggle { look, flag: flag.0, action },
            None if name == "toggle" => return Err("a toggle needs a `flag` to show".into()),
            None => Kind::Button { look, action },
        })
    }

    fn look(&mut self, f: &ElementFile) -> Result<Look, Fault> {
        Ok(Look {
            normal: self.paint(f.normal.as_ref())?,
            pressed: self.paint(f.pressed.as_ref())?,
            on: self.paint(f.on.as_ref())?,
            on_pressed: self.paint(f.on_pressed.as_ref())?,
            inactive: self.paint(f.inactive.as_ref())?,
        })
    }

    fn slider(&mut self, f: &ElementFile) -> Result<Slider, Fault> {
        let value = f.value.ok_or("a slider needs a `value`")?.0;
        let vertical = f.vertical.unwrap_or(false);
        let thumb = self.thumb(f)?;
        let thumb_size = thumb.normal.map_or(0, |p| if vertical { p.src.h } else { p.src.w });
        let track = match &f.track {
            Some(t) => Some(self.frames(t)?),
            None => None,
        };
        Ok(Slider {
            value,
            vertical,
            track,
            thumb,
            thumb_len: f.thumb_len.unwrap_or(thumb_size),
            live: f.live.unwrap_or(true),
            reset: f.reset,
            wheel: f.wheel_step.unwrap_or(0),
            detent: f.detent.unwrap_or(0),
        })
    }

    fn thumb(&mut self, f: &ElementFile) -> Result<Look, Fault> {
        let Some(t) = &f.thumb else { return Ok(Look::default()) };
        Ok(Look { normal: self.paint(t.normal.as_ref())?, pressed: self.paint(t.pressed.as_ref())?, ..Look::default() })
    }

    fn item(&mut self, i: &ItemFile) -> Result<PopupItem, Fault> {
        let look = Look { normal: self.paint(i.normal.as_ref())?, pressed: self.paint(i.pressed.as_ref())?, ..Look::default() };
        let action = action(i.click, i.shift, i.ctrl, i.press, i.menu_press, i.menu_release)?;
        Ok(PopupItem { look, action })
    }

    fn frames(&mut self, t: &Spanned<FramesFile>) -> Result<Frames, Fault> {
        let f = t.get_ref();
        let (id, _) = self.svg(&f.file).map_err(|msg| Fault { span: Some(t.span()), msg })?;
        let [x, y, w, h] = f.first;
        Ok(Frames {
            picture: Picture::Svg(id),
            first: Rect { x, y, w, h },
            step: (f.step[0], f.step[1]),
            count: f.count,
            per_row: f.per_row,
            map: f.map.0,
        })
    }

    fn paint(&mut self, p: Option<&Spanned<PaintFile>>) -> Result<Option<Paint>, Fault> {
        let Some(p) = p else { return Ok(None) };
        let (file, area, offset) = match p.get_ref() {
            PaintFile::Whole(file) => (file, None, None),
            PaintFile::Part { file, area, offset } => (file, *area, *offset),
        };
        let (id, (w, h)) = self.svg(file).map_err(|msg| Fault { span: Some(p.span()), msg })?;
        let src = area.map_or(Rect { x: 0, y: 0, w, h }, |[x, y, w, h]| Rect { x, y, w, h });
        let [dx, dy] = offset.unwrap_or([0, 0]);
        Ok(Some(Paint { picture: Picture::Svg(id), src, dx, dy }))
    }

    /// Parses an SVG file once; returns its number and size in whole units.
    fn svg(&mut self, file: &str) -> Result<(u16, (i32, i32)), String> {
        let id = match self.svg_ids.get(file) {
            Some(&id) => id,
            None => {
                let data = self.files.get(file).ok_or_else(|| format!("no file `{file}` in the skin"))?;
                let id = self.skin.svgs.add(data, self.css.as_deref()).map_err(|e| format!("{file}: {e}"))?;
                self.svg_ids.insert(file.to_string(), id);
                id
            }
        };
        let (w, h) = self.skin.svgs.size(id).unwrap_or_default();
        Ok((id, (w.ceil() as i32, h.ceil() as i32)))
    }
}

/// A bar's mode: as wide as the screen; its `size` is the narrowest it gets.
fn spanning_the_screen(mode: Mode) -> Mode {
    let min = match mode.width {
        Dim::Fixed(min) | Dim::Free { min, .. } => min,
    };
    Mode { width: Dim::Free { min, step: 1 }, ..mode }
}

/// Checks the element's `type` and that its fields fit it; returns the type.
fn check_fields(f: &ElementFile) -> Result<&str, String> {
    let name = f.kind.as_deref().ok_or("an element needs a `type`")?;
    let Some((_, allowed)) = KINDS.iter().find(|k| k.0 == name) else {
        let names: Vec<_> = KINDS.iter().map(|k| k.0).collect();
        return Err(format!("unknown element type `{name}`; it can be one of: {}", names.join(", ")));
    };
    match f.set_fields().into_iter().find(|n| !allowed.contains(n)) {
        Some(extra) => Err(format!("`{extra}` does not apply to type `{name}`")),
        None => Ok(name),
    }
}

fn show(f: &ElementFile) -> Result<Cond, String> {
    match (f.show, f.hide, f.min_width) {
        (None, None, None) => Ok(Cond::Always),
        (Some(s), None, None) => Ok(Cond::If(s.0)),
        (None, Some(h), None) => Ok(Cond::Unless(h.0)),
        (None, None, Some(w)) => Ok(Cond::MinWidth(w)),
        _ => Err("use only one of `show`, `hide` and `min_width`".into()),
    }
}

fn place(f: &ElementFile) -> Result<Place, String> {
    match (f.rect, &f.edges) {
        (Some([x, y, w, h]), None) if w >= 0 && h >= 0 => Ok(Place::fixed(x, y, w, h)),
        (Some(_), None) => Err("`rect` is x, y, width, height; width and height cannot be negative".into()),
        (None, Some([l, t, r, b])) => Ok(Place { left: l.0, top: t.0, right: r.0, bottom: b.0 }),
        (Some(_), Some(_)) => Err("give either `rect` or `edges`, not both".into()),
        (None, None) => Err("an element needs a `rect` (x, y, width, height) or `edges`".into()),
    }
}

fn action(
    click: Option<N<Command>>,
    shift: Option<N<Command>>,
    ctrl: Option<N<Command>>,
    press: Option<N<Command>>,
    menu_press: Option<N<MenuRef>>,
    menu_release: Option<N<MenuRef>>,
) -> Result<Action, String> {
    let get = |c: Option<N<Command>>| c.map(|c| c.0);
    match (click, press, menu_press, menu_release) {
        (Some(c), None, None, None) => Ok(Action::Click { plain: c.0, shift: get(shift), ctrl: get(ctrl) }),
        _ if shift.is_some() || ctrl.is_some() => Err("`shift` and `ctrl` go with `click`".into()),
        (None, Some(c), None, None) => Ok(Action::Press(c.0)),
        (None, None, Some(m), None) => Ok(Action::MenuOnPress(m.0)),
        (None, None, None, Some(m)) => Ok(Action::MenuOnRelease(m.0)),
        (None, None, None, None) => Ok(Action::Nothing),
        _ => Err("use only one of `click`, `press`, `menu_press` and `menu_release`".into()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SQUARE: &[u8] =
        br##"<svg xmlns="http://www.w3.org/2000/svg" width="23" height="18"><rect width="23" height="18" fill="#0f0"/></svg>"##;

    const DOT: &[u8] =
        br##"<svg xmlns="http://www.w3.org/2000/svg" width="20" height="20"><circle cx="10" cy="10" r="10" fill="#0f0"/></svg>"##;

    fn skin(toml: &str) -> Result<Skin, String> {
        let files = HashMap::from([
            (FILE.to_string(), toml.as_bytes().to_vec()),
            ("play.svg".to_string(), SQUARE.to_vec()),
            ("dot.svg".to_string(), DOT.to_vec()),
        ]);
        load(&files, None)
    }

    const HEAD: &str = "[meta]\nname = \"Test\"\nformat_version = 1\n";

    #[test]
    fn a_window_with_a_button() {
        let s = skin(&format!(
            "{HEAD}
[[window]]
key = \"main\"
normal.size = [275, 116]

[[window.normal.element]]
type = \"button\"
rect = [39, 88, 23, 18]
normal = \"play.svg\"
pressed = {{ file = \"play.svg\", area = [0, 0, 23, 9] }}
click = \"play\"
shift = \"stop\"
"
        ))
        .unwrap();
        assert_eq!(s.name, "Test");
        let main = s.layout.window(WindowKey::Main).unwrap();
        let el = &main.normal.elements[0];
        assert_eq!(el.at.resolve(275, 116), Rect { x: 39, y: 88, w: 23, h: 18 });
        let Kind::Button { look, action } = &el.kind else { panic!("{:?}", el.kind) };
        assert_eq!(look.normal.unwrap().src, Rect { x: 0, y: 0, w: 23, h: 18 });
        assert_eq!(look.pressed.unwrap().src, Rect { x: 0, y: 0, w: 23, h: 9 });
        assert_eq!(look.normal.unwrap().picture, look.pressed.unwrap().picture, "one file is parsed once");
        assert_eq!(*action, Action::Click { plain: Command::Play, shift: Some(Command::Stop), ctrl: None });
        assert!(s.layout.window(WindowKey::Eq).is_some(), "missing windows come from the built-in skin");
    }

    #[test]
    fn a_clock_is_aligned_left_unless_the_skin_says_otherwise() {
        let clock = |extra: &str| {
            skin(&format!(
                "{HEAD}[[window]]
key = \"main\"
normal.size = [275, 116]
[[window.normal.element]]
type = \"clock\"
rect = [26, 21, 62, 18]
{extra}"
            ))
            .map(|s| s.layout.window(WindowKey::Main).unwrap().normal.elements[0].kind.clone())
        };
        assert!(matches!(clock(""), Ok(Kind::Clock { align: Align::Left, font_size: None })));
        assert!(matches!(
            clock(
                "align = \"right\"
"
            ),
            Ok(Kind::Clock { align: Align::Right, font_size: None })
        ));
        assert!(matches!(
            clock(
                "align = \"center\"
"
            ),
            Ok(Kind::Clock { align: Align::Center, font_size: None })
        ));
        assert!(matches!(clock("font_size = 10\n"), Ok(Kind::Clock { align: Align::Left, font_size: Some(10) })), "smaller in a bar");
        assert!(clock(
            "align = \"middle\"
"
        )
        .unwrap_err()
        .contains("unknown alignment `middle`; it can be one of: left, center, right"));
        let text = skin(&format!(
            "{HEAD}[[window]]
key = \"main\"
normal.size = [275, 116]
[[window.normal.element]]
type = \"text\"
rect = [0, 0, 9, 9]
text = \"kbps\"
align = \"right\"
"
        ));
        assert!(text.err().unwrap().contains("`align` does not apply to type `text`"));
    }

    #[test]
    fn only_a_skin_with_nums_ex_draws_its_clock_with_it() {
        assert!(!skin(&format!("{HEAD}[sheets]\nmain = \"play.svg\"\n")).unwrap().has_nums_ex());
        assert!(skin(&format!("{HEAD}[sheets]\nnums_ex = \"play.svg\"\n")).unwrap().has_nums_ex());
    }

    #[test]
    fn a_round_button_takes_the_mouse_on_its_shape_only() {
        let s = skin(&format!(
            "{HEAD}
[[window]]
key = \"main\"
normal.size = [275, 116]

[[window.normal.element]]
type = \"button\"
rect = [10, 10, 20, 20]
normal = \"dot.svg\"
hit = \"dot.svg\"
click = \"play\"
"
        ))
        .unwrap();
        let el = &s.layout.window(WindowKey::Main).unwrap().normal.elements[0];
        let hit = el.hit.as_ref().unwrap();
        assert!(s.opaque(hit, 10, 10), "the middle");
        assert!(!s.opaque(hit, 1, 1), "a corner");
        assert!(!s.opaque(hit, 25, 10), "beside the picture");
    }

    #[test]
    fn a_see_through_window_is_shaped_by_its_pictures() {
        let mode = |extra: &str| format!("{HEAD}[[window]]\nkey = \"playlist\"\n[window.normal]\nsize = [275, 116]\n{extra}");
        let s = skin(&mode("see_through = true\n")).unwrap();
        assert_eq!(s.layout.window(WindowKey::Playlist).unwrap().normal.shape, Some(Shape::Pictures));
        let e = skin(&mode("see_through = true\nshape = \"play.svg\"\n")).err().unwrap();
        assert!(e.contains("`shape` or `see_through`"), "{e}");
    }

    #[test]
    fn sheets_replace_classic_bitmaps() {
        let s = skin(&format!("{HEAD}[sheets]\nmain = \"play.svg\"\n[shapes]\nmain_shade = \"play.svg\"\n")).unwrap();
        assert!(s.vector);
        let builtin = Skin::builtin_vector();
        assert_ne!(s.sheet_svgs[&Sheet::Main], builtin.sheet_svgs[&Sheet::Main]);
        assert_eq!(s.sheet_svgs.len(), builtin.sheet_svgs.len(), "sheets left out are the built-in skin's drawings");
        assert_eq!((s.sheet(Sheet::Main).w, s.sheet(Sheet::Main).px[0]), (23, 0x00ff00), "an x1 copy for pixel reads");
        let main = s.layout.window(WindowKey::Main).unwrap();
        assert!(matches!(main.mode(View::Shade).shape, Some(Shape::Picture(_))));
        assert_eq!(main.normal.shape, None);
        let e = skin(&format!("{HEAD}[sheets]\nmian = \"play.svg\"\n")).err().unwrap();
        assert!(e.starts_with("skin.toml, line 5: unknown sheet `mian`; it can be one of: main, cbuttons"), "{e}");
        let e = skin(&format!("{HEAD}[shapes]\nlist = \"play.svg\"\n")).err().unwrap();
        assert!(e.starts_with("skin.toml, line 5: unknown window `list`"), "{e}");
    }

    #[test]
    fn shapes_come_from_pictures() {
        let s = skin(&format!("{HEAD}\n[[window]]\nkey = \"eq\"\nnormal.size = [275, 116]\nnormal.shape = \"play.svg\"\n")).unwrap();
        let eq = s.layout.window(WindowKey::Eq).unwrap();
        let Some(Shape::Picture(p)) = &eq.normal.shape else { panic!("{:?}", eq.normal.shape) };
        assert_eq!(p.src, Rect { x: 0, y: 0, w: 23, h: 18 });
    }

    #[test]
    fn edges_follow_the_window() {
        let s = skin(&format!(
            "{HEAD}
[[window]]
key = \"playlist\"
placement = \"docked\"
[window.normal]
size = [275, 116]
resize_step = [25, 29]
[[window.normal.element]]
type = \"list\"
edges = [12, 20, \"end-20\", \"end-38\"]
"
        ))
        .unwrap();
        let pl = s.layout.window(WindowKey::Playlist).unwrap();
        assert_eq!(pl.normal.width, Dim::Free { min: 275, step: 25 });
        assert_eq!(pl.normal.elements[0].at.resolve(300, 145), Rect { x: 12, y: 20, w: 268, h: 87 });
    }

    #[test]
    fn a_bar_spans_the_screen_and_only_the_main_window_is_one() {
        let bar = |key: &str| skin(&format!("{HEAD}[[window]]\nkey = \"{key}\"\nplacement = \"bar-top\"\nnormal.size = [400, 28]\n"));
        let s = bar("main").unwrap();
        let main = s.layout.window(WindowKey::Main).unwrap();
        assert_eq!(main.placement, Placement::Bar(popamp_core::config::BarEdge::Top));
        assert_eq!(main.normal.width, Dim::Free { min: 400, step: 1 }, "as wide as the screen, at least 400");
        assert_eq!(main.normal.height, Dim::Fixed(28));
        let e = bar("eq").err().unwrap();
        assert!(e.contains("only the main window can be a bar"), "{e}");
    }

    const BOTH: &str = "[[window]]\nkey = \"main\"\nnormal.size = [275, 116]\n";

    #[test]
    fn a_main_window_can_be_a_window_and_a_bar() {
        let s = skin(&format!("{HEAD}{BOTH}[window.bar]\nedge = \"top\"\nsize = [400, 20]\n")).unwrap();
        let main = s.layout.window(WindowKey::Main).unwrap();
        assert_eq!(main.placement, Placement::Free, "a window until the user asks for the bar");
        assert_eq!(main.view(false, true), View::Shade);
        assert_eq!(main.view(true, true), View::Bar);
        assert_eq!(main.placement_in(View::Bar), Placement::Bar(BarEdge::Top));
        assert_eq!(main.mode(View::Bar).width, Dim::Free { min: 400, step: 1 });
        assert_eq!(main.mode(View::Normal).width, Dim::Fixed(275));
        let s = skin(&format!("{HEAD}{BOTH}bar.size = [400, 20]\n")).unwrap();
        assert_eq!(s.layout.window(WindowKey::Main).unwrap().placement_in(View::Bar), Placement::Bar(BarEdge::Bottom));
    }

    #[test]
    fn the_classic_window_can_get_a_bar_view() {
        let s = skin(&format!("{HEAD}[[window]]\nkey = \"main\"\nbar.size = [640, 20]\n")).unwrap();
        let main = s.layout.window(WindowKey::Main).unwrap();
        assert_eq!((main.normal.width, main.normal.elements.len() > 20), (Dim::Fixed(275), true), "the classic layout stays");
        assert!(main.shade.is_some());
        assert_eq!(main.mode(View::Bar).width, Dim::Free { min: 640, step: 1 });
        let e = |toml: &str| skin(&format!("{HEAD}[[window]]\nkey = \"main\"\n{toml}")).err().unwrap();
        assert!(e("").contains("main: a window needs `normal`"));
        assert!(e("bar.size = [640, 20]\nplacement = \"docked\"\n").contains("without it only `bar` can be given"));
    }

    #[test]
    fn a_skin_without_a_bar_view_stays_a_window() {
        let s = skin(&format!("{HEAD}{BOTH}")).unwrap();
        let main = s.layout.window(WindowKey::Main).unwrap();
        assert_eq!(main.view(true, false), View::Normal);
        assert_eq!(main.placement_in(View::Bar), Placement::Free);
    }

    #[test]
    fn a_bar_view_is_refused_where_it_makes_no_sense() {
        let bad = |toml: &str| skin(&format!("{HEAD}{toml}")).err().unwrap();
        let e = bad("[[window]]\nkey = \"eq\"\nnormal.size = [275, 116]\nbar.size = [400, 20]\n");
        assert_eq!(e, "skin.toml, line 7: only the main window can be a bar");
        let e = bad("[[window]]\nkey = \"main\"\nplacement = \"bar-top\"\nnormal.size = [400, 20]\nbar.size = [400, 20]\n");
        assert!(e.contains("always a bar"), "{e}");
        let e = bad("[[window]]\nkey = \"main\"\nnormal.size = [275, 116]\nnormal.edge = \"top\"\n");
        assert_eq!(e, "skin.toml, line 7: `edge` is for `[window.bar]` only");
        let e = bad(&format!("{BOTH}bar.size = [400, 20]\nbar.edge = \"left\"\n"));
        assert!(e.contains("unknown edge `left`; it can be one of: top, bottom"), "{e}");
    }

    fn error(body: &str) -> String {
        skin(&format!("{HEAD}\n[[window]]\nkey = \"main\"\nnormal.size = [275, 116]\n[[window.normal.element]]\n{body}")).err().unwrap()
    }

    #[test]
    fn mistakes_say_where_and_what() {
        assert_eq!(
            error("type = \"button\"\nrect = [0, 0, 1, 1]\nclick = \"plya\"").split(';').next().unwrap(),
            "skin.toml, line 11: unknown command `plya`"
        );
        assert_eq!(
            error("type = \"image\"\nrect = [0, 0, 1, 1]\nclick = \"play\""),
            "skin.toml, line 8: `click` does not apply to type `image`"
        );
        assert_eq!(
            error("type = \"image\"\nrect = [0, 0, 1, 1]\nnormal = \"none.svg\""),
            "skin.toml, line 11: no file `none.svg` in the skin"
        );
        assert_eq!(error("type = \"toggle\"\nrect = [0, 0, 1, 1]"), "skin.toml, line 8: a toggle needs a `flag` to show");
        assert!(error("type = \"image\"\nrect = [0, 0, 1, 1]\ncolour = 1").starts_with("skin.toml, line 11: unknown field `colour`"));
    }

    #[test]
    fn text_styles_and_colours() {
        let s = skin(&format!(
            "{HEAD}
[text]
size = 7
color = \"#00e000\"
[playlist]
current = \"#ffffff\"
[vis]
colors = [{}]
",
            vec!["\"#010203\""; 24].join(", ")
        ))
        .unwrap();
        assert_eq!(s.text, Some(crate::TextStyle { font: None, size: 7, color: 0x00e000 }));
        assert_eq!(s.clock, None);
        assert_eq!((s.pl.current, s.pl.normal), (0xffffff, crate::PlColors::default().normal));
        assert_eq!(s.viscolor, [0x010203; 24]);
    }

    const TINTED: &[u8] = br##"<svg xmlns="http://www.w3.org/2000/svg" width="2" height="2">
        <rect class="back" width="2" height="2" fill="#ff0000"/></svg>"##;

    fn tinted(colors: Option<&str>) -> Skin {
        let toml = format!(
            "{HEAD}[colors.day]\nback = \"#ffffff\"\nink = \"#000000\"\n[colors.night]\nback = \"#000020\"\n\
            [text]\nsize = 7\ncolor = \"ink\"\n[playlist]\nnormal_bg = \"back\"\n\
            [[window]]\nkey = \"main\"\nnormal.size = [2, 2]\n\
            [[window.normal.element]]\ntype = \"image\"\nrect = [0, 0, 2, 2]\nnormal = \"tinted.svg\"\n"
        );
        let files = HashMap::from([(FILE.to_string(), toml.into_bytes()), ("tinted.svg".to_string(), TINTED.to_vec())]);
        load(&files, colors).unwrap()
    }

    fn first_pixel(s: &Skin) -> u32 {
        let Kind::Image { look, .. } = &s.layout.window(WindowKey::Main).unwrap().normal.elements[0].kind else { panic!() };
        let Some(Paint { picture: Picture::Svg(id), src, .. }) = look.normal else { panic!() };
        s.svgs.raster(id, src, 2, 2).unwrap().px[0]
    }

    #[test]
    fn a_colour_set_paints_the_pictures_and_the_texts() {
        let day = tinted(None);
        assert_eq!((day.color_sets.as_slice(), day.color_set), (["day".to_string(), "night".to_string()].as_slice(), 0));
        assert_eq!((first_pixel(&day), day.pl.normal_bg, day.text.unwrap().color), (0xffffffff, 0xffffff, 0));
        let night = tinted(Some("night"));
        assert_eq!(night.color_set, 1);
        assert_eq!((first_pixel(&night), night.pl.normal_bg, night.text.unwrap().color), (0xff000020, 0x000020, 0));
    }

    #[test]
    fn a_skin_without_colour_sets_keeps_its_pictures_as_drawn() {
        let s = skin(HEAD).unwrap();
        assert!(s.color_sets.is_empty());
        let e = skin(&format!("{HEAD}[colors.a]\nink = \"black\"\n")).err().unwrap();
        assert_eq!(e, "skin.toml, line 5: `black` is not a colour; write it #rrggbb");
    }

    #[test]
    fn bad_colours_and_fonts_are_errors() {
        let e = skin(&format!("{HEAD}[text]\nsize = 7\ncolor = \"green\"\n")).err().unwrap();
        assert_eq!(e, "skin.toml, line 6: `green` is not a colour; write it #rrggbb");
        let e = skin(&format!("{HEAD}[clock]\nfont = \"play.svg\"\nsize = 13\ncolor = \"#00ff00\"\n")).err().unwrap();
        assert!(e.starts_with("skin.toml, line 5: play.svg: not a font"), "{e}");
    }

    #[test]
    fn a_skin_for_a_newer_popamp_says_so_before_anything_else() {
        // A key this Popamp does not know, in a skin that asks for a newer one.
        let e = skin(
            "[meta]
name = \"X\"
format_version = 1
min_popamp = \"99.0.0\"
[future]
key = 1
",
        )
        .err()
        .unwrap();
        assert_eq!(e, format!("skin.toml, line 4: needs Popamp 99.0.0 or newer; this is {}", popamp_core::version::POPAMP));
        assert!(skin(&format!(
            "[meta]
name = \"X\"
format_version = 1
min_popamp = \"{}\"
",
            popamp_core::version::POPAMP
        ))
        .is_ok());
        assert!(skin(
            "[meta]
name = \"X\"
format_version = 1
min_popamp = \"0.8\"
"
        )
        .err()
        .unwrap()
        .contains("is not a version"));
    }

    #[test]
    fn other_format_versions_are_refused() {
        let e = skin("[meta]\nname = \"X\"\nformat_version = 2\n").err().unwrap();
        assert_eq!(e, "skin.toml, line 3: this player reads format_version 1");
    }
}
