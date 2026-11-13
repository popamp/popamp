//! Pop-up menus: model, layout and drawing (windows live in `app.rs`).

use crate::dialogs::DialogKind;
use crate::font::Font;
use crate::pen::{self, Pen};
use popamp_core::Command;
use popamp_skin::bitmap::{mix, rgb};
use popamp_skin::Bitmap;
use std::path::PathBuf;

#[derive(Debug, Clone)]
pub enum MenuAction {
    Cmd(Command),
    Skin(Option<PathBuf>),
    VisMode(i32),
    /// Opens the visual plug-in with this id.
    #[cfg(feature = "visualizers")]
    Visualizer(String),
    SaStyle(i32),
    OscStyle(i32),
    SaPeaks,
    SaThick,
    EqBands(bool),
    EqLimiter,
    ReplayGain(Option<i32>),
    /// Window size in percent; 100 turns "Double Size" off.
    Zoom(i32),
    Scaling(popamp_core::config::Scaling),
    /// The skin's colour set of this name.
    SkinColors(String),
    /// The main window as a bar (true) or a window, in a skin with both.
    MainAsBar(bool),
    BarEdge(popamp_core::config::BarEdge),
    /// A bar's size in percent.
    BarZoom(i32),
    TitleFormat(String),
    EqPreset(usize, bool),
    EqDialog(DialogKind),
    PlaySelected,
    PlayBookmark(usize),
}

#[derive(Debug, Clone)]
pub struct MenuItem {
    pub label: String,
    pub accel: String,
    pub action: Option<MenuAction>,
    pub checked: bool,
    pub sub: Vec<MenuItem>,
    pub separator: bool,
}

impl MenuItem {
    pub fn action(label: &str, accel: &str, action: MenuAction) -> Self {
        Self { label: label.into(), accel: accel.into(), action: Some(action), checked: false, sub: vec![], separator: false }
    }
    pub fn cmd(label: &str, accel: &str, c: Command) -> Self {
        Self::action(label, accel, MenuAction::Cmd(c))
    }
    pub fn sub(label: &str, items: Vec<MenuItem>) -> Self {
        Self { label: label.into(), accel: String::new(), action: None, checked: false, sub: items, separator: false }
    }
    pub fn label(label: &str) -> Self {
        Self { label: label.into(), accel: String::new(), action: None, checked: false, sub: vec![], separator: false }
    }
    pub fn sep() -> Self {
        Self { label: String::new(), accel: String::new(), action: None, checked: false, sub: vec![], separator: true }
    }
    pub fn checked(mut self, on: bool) -> Self {
        self.checked = on;
        self
    }
    pub fn enabled(&self) -> bool {
        !self.separator && (self.action.is_some() || !self.sub.is_empty())
    }
}

pub const FONT_PX: u32 = 12;

/// A tooltip at `pct` percent: black text on pale yellow, as system
/// tooltips used to be.
pub fn tooltip(text: &str, font: &mut Font, pct: i32) -> Bitmap {
    let (w, h) = (pen::text_width(font, text, pct) + 6, 16);
    let mut b = Bitmap::new(pen::screen(pct, w), pen::screen(pct, h), rgb(255, 255, 225));
    let mut pen = Pen::new(&mut b, font, pct);
    pen.bevel(0, 0, w, h, rgb(0, 0, 0), rgb(0, 0, 0));
    pen.text(3, 1, text, rgb(0, 0, 0), 1, w - 2);
    b
}
const ITEM_H: i32 = 20;
const SEP_H: i32 = 7;
const PAD_X: i32 = 22;

pub const BG: u32 = rgb(0x24, 0x28, 0x33);
pub const BORDER: u32 = rgb(0x4a, 0x52, 0x68);
pub const TEXT: u32 = rgb(0xe6, 0xea, 0xf2);
pub const DIM: u32 = rgb(0x8a, 0x92, 0xa6);
pub const HILITE: u32 = rgb(0x3d, 0x5a, 0x99);

/// Geometry of one open menu level, in dialog pixels (see [`Pen`]).
pub struct Layout {
    pub w: i32,
    pub h: i32,
    /// The size it is drawn at, in percent.
    pub pct: i32,
    /// (y, height) per item.
    pub rows: Vec<(i32, i32)>,
}

pub fn layout(items: &[MenuItem], font: &mut Font, pct: i32) -> Layout {
    let mut y = 3;
    let mut rows = Vec::with_capacity(items.len());
    let mut label_w = 0;
    let mut accel_w = 0;
    for it in items {
        let h = if it.separator { SEP_H } else { ITEM_H };
        rows.push((y, h));
        y += h;
        if !it.separator {
            label_w = label_w.max(pen::text_width(font, &it.label, pct));
            accel_w = accel_w.max(pen::text_width(font, &it.accel, pct));
        }
    }
    let w = (PAD_X + label_w + if accel_w > 0 { 24 + accel_w } else { 0 } + 26).max(140);
    Layout { w, h: y + 3, pct, rows }
}

impl Layout {
    /// The size of the menu's window in screen pixels.
    pub fn screen(&self) -> (i32, i32) {
        (pen::screen(self.pct, self.w), pen::screen(self.pct, self.h))
    }
}

/// The item at screen offset (`sx`, `sy`) in the menu's window.
pub fn hit(layout: &Layout, items: &[MenuItem], sx: i32, sy: i32) -> Option<usize> {
    let (x, y) = (pen::dialog(layout.pct, sx), pen::dialog(layout.pct, sy));
    if x < 0 || x >= layout.w {
        return None;
    }
    layout.rows.iter().position(|&(ry, rh)| y >= ry && y < ry + rh).filter(|&i| items[i].enabled())
}

pub fn draw(b: &mut Bitmap, items: &[MenuItem], layout: &Layout, hover: Option<usize>, font: &mut Font) {
    let mut pen = Pen::new(b, font, layout.pct);
    pen.fill(0, 0, layout.w, layout.h, BG);
    pen.bevel(0, 0, layout.w, layout.h, BORDER, mix(BORDER, BG, 0.6));
    for (i, it) in items.iter().enumerate() {
        let (y, h) = layout.rows[i];
        if it.separator {
            pen.hline(6, y + h / 2, layout.w - 12, mix(BORDER, BG, 0.3));
            continue;
        }
        let enabled = it.enabled();
        if hover == Some(i) && enabled {
            pen.fill(2, y, layout.w - 4, h, HILITE);
        }
        let color = if enabled { TEXT } else { DIM };
        let ty = y + (h - FONT_PX as i32) / 2 - 1;
        if it.checked {
            // Check mark.
            for k in 0..3 {
                pen.set(7 + k, y + 10 + k, color);
                pen.set(7 + k, y + 11 + k, color);
            }
            for k in 0..5 {
                pen.set(10 + k, y + 12 - k, color);
                pen.set(10 + k, y + 13 - k, color);
            }
        }
        pen.text(PAD_X, ty, &it.label, color, 0, layout.w);
        if !it.accel.is_empty() {
            let aw = pen.width(&it.accel);
            pen.text(layout.w - 14 - aw, ty, &it.accel, DIM, 0, layout.w);
        }
        if !it.sub.is_empty() {
            let (ax, ay) = (layout.w - 10, y + h / 2);
            for k in 0..4 {
                pen.vline(ax + k, ay - (3 - k), (3 - k) * 2 + 1, color);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const CLOCK: &[u8] = include_bytes!("../../../crates/popamp-skin/tests/fixtures/fonts-shade/fonts/clock.ttf");

    fn items() -> Vec<MenuItem> {
        vec![MenuItem::cmd("1", "", Command::Play), MenuItem::sep(), MenuItem::cmd("2", "", Command::Stop)]
    }

    #[test]
    fn a_menu_grows_with_the_windows_and_finds_items_where_it_draws_them() {
        let mut font = Font::from_data(CLOCK);
        let one = layout(&items(), &mut font, 100);
        let three = layout(&items(), &mut font, 300);
        assert_eq!((one.w, one.h), (three.w, three.h), "the same in dialog pixels");
        assert_eq!(three.screen(), (3 * one.w, 3 * one.h));
        let (y, h) = three.rows[2];
        assert_eq!(hit(&three, &items(), 30, 3 * (y + h / 2)), Some(2));
        assert_eq!(hit(&three, &items(), 30, y + h / 2), Some(0), "where ×1 has the last item, ×3 has the first");
        let mut b = Bitmap::new(three.screen().0, three.screen().1, 0);
        draw(&mut b, &items(), &three, Some(2), &mut font);
        assert_eq!(b.get(10, 3 * y + 3), Some(HILITE), "the hovered row is lit across its whole height");
        assert_eq!(b.get(10, 3 * (y + h) - 1), Some(HILITE));
    }
}
