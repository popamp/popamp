//! A skinned window run from its layout: draws the elements of the current
//! mode and turns mouse input into commands and value changes. The same
//! code serves every window of every skin.

use crate::bind;
use crate::canvas::Canvas;
use crate::font::Fonts;
use crate::model::Model;
use crate::platform::Mask;
use crate::render::{self, picture, sprite};
use crate::scale::Scale;
use crate::ui::{Input, Out};
use popamp_audio::PlayState;
use popamp_core::text::format_duration_ms;
use popamp_core::Command;
use popamp_skin::layout::{
    Action, Align, Element, EmptyPress, Fill, Kind, Look, MenuRef, Mode, Paint, PopupItem, Slider, Value, View, Wheel, WindowDef, WindowKey,
};
use popamp_skin::sprites::{eq as e, Rect};
use popamp_skin::{Bitmap, CursorKind, Sheet, Skin};
use std::time::{Duration, Instant};

const MARQUEE_STEP: Duration = Duration::from_millis(180);

pub struct SkinWin {
    key: WindowKey,
    /// The element under the held mouse button: (shade mode, index).
    pressed: Option<(View, usize)>,
    /// Where on the thumb a slider was taken, along the slider.
    grab: i32,
    /// The value of a slider that applies on release, while it is dragged.
    preview: Option<i64>,
    marquee_offset: i32,
    marquee_drag: Option<(i32, i32)>,
    last_scroll: Instant,
    list: ListState,
    /// The open popup: (view, element index), and the item under the mouse.
    popup: Option<((View, usize), Option<usize>)>,
}

#[derive(Default)]
struct ListState {
    /// First visible row.
    scroll: usize,
    /// Where shift-selection starts.
    anchor: usize,
    /// Row-drag reordering: the row the drag is currently at.
    drag_row: Option<usize>,
}

#[derive(Debug, Clone, Copy)]
pub enum Nav {
    Up,
    Down,
    PageUp,
    PageDown,
    Home,
    End,
}

/// The window's current mode with its size.
struct Current<'a> {
    mode: &'a Mode,
    w: i32,
    h: i32,
}

impl SkinWin {
    pub fn new(key: WindowKey) -> Self {
        Self {
            key,
            pressed: None,
            grab: 0,
            preview: None,
            marquee_offset: 0,
            marquee_drag: None,
            last_scroll: Instant::now(),
            list: ListState::default(),
            popup: None,
        }
    }

    fn def<'a>(&self, model: &'a Model) -> Option<&'a WindowDef> {
        model.skin.layout.window(self.key)
    }

    fn current<'a>(&self, model: &'a Model) -> Option<Current<'a>> {
        let mode = self.def(model)?.mode(bind::view(model, self.key));
        let (w, h) = bind::window_size(model, self.key);
        Some(Current { mode, w, h })
    }

    /// The pressed element, if it belongs to the mode on screen.
    fn pressed_index(&self, model: &Model) -> Option<usize> {
        let (shade, i) = self.pressed?;
        (shade == bind::view(model, self.key)).then_some(i)
    }

    /// The last element under the point that takes the mouse.
    fn hit(&self, model: &Model, x: i32, y: i32) -> Option<usize> {
        let c = self.current(model)?;
        c.mode.elements.iter().enumerate().rev().find_map(|(i, el)| {
            let r = el.at.resolve(c.w, c.h);
            let on_shape = || el.hit.as_ref().is_none_or(|p| model.skin.opaque(p, x - r.x, y - r.y));
            let takes = el.interactive() && bind::holds(model, el.show, c.w) && r.contains(x, y) && on_shape();
            takes.then_some(i)
        })
    }

    fn element(&self, model: &Model, i: usize) -> Option<(Element, Rect)> {
        let c = self.current(model)?;
        let el = c.mode.elements.get(i)?;
        Some((el.clone(), el.at.resolve(c.w, c.h)))
    }

    /// The skin cursor for the element under the mouse, or the one being dragged.
    pub fn cursor(&self, model: &Model, x: i32, y: i32) -> CursorKind {
        let Some(c) = self.current(model) else { return CursorKind::Normal };
        self.pressed_index(model)
            .or_else(|| self.hit(model, x, y))
            .and_then(|i| c.mode.elements.get(i))
            .and_then(|el| el.cursor)
            .unwrap_or(c.mode.cursor)
    }

    pub fn tooltip(&self, model: &Model, x: i32, y: i32) -> Option<String> {
        let c = self.current(model)?;
        self.hit(model, x, y).and_then(|i| c.mode.elements[i].tip.clone())
    }

    /// The play position being chosen on a slider, if any.
    fn seek_preview(&self, model: &Model) -> Option<i64> {
        let (el, _) = self.element(model, self.pressed_index(model)?)?;
        match el.kind {
            Kind::Slider(Slider { value: Value::Position, live: false, .. }) => self.preview,
            _ => None,
        }
    }

    fn marquee_text(&self, model: &Model) -> String {
        if let Some(ms) = self.seek_preview(model) {
            let len = model.engine.shared().length_ms().unwrap_or(1).max(1);
            let ms = ms.max(0) as u64;
            return format!("SEEK TO: {}/{} ({}%)", format_duration_ms(ms), format_duration_ms(len), ms * 100 / len);
        }
        if let Some((t, until)) = &model.transient {
            if Instant::now() < *until {
                return t.clone();
            }
        }
        let title = model.current_title();
        if title.is_empty() {
            crate::edition::title().to_uppercase()
        } else {
            title
        }
    }

    fn marquee_width(&self, model: &Model) -> Option<i32> {
        let c = self.current(model)?;
        c.mode.elements.iter().find_map(|el| match el.kind {
            Kind::Marquee { area, .. } => Some(marquee_rect(area, el.at.resolve(c.w, c.h)).w),
            _ => None,
        })
    }

    /// Advances the marquee; returns true if it moved.
    pub fn tick(&mut self, model: &Model, fonts: &mut Fonts) -> bool {
        let Some(width) = self.marquee_width(model) else { return false };
        if self.marquee_drag.is_some() || !model.settings().autoscrollname {
            return false;
        }
        if render::text_width(&model.skin, fonts, &self.marquee_text(model)) <= width {
            self.marquee_offset = 0;
            return false;
        }
        if self.last_scroll.elapsed() >= MARQUEE_STEP {
            self.last_scroll = Instant::now();
            self.marquee_offset += 5;
            return true;
        }
        false
    }

    // --- track list ----------------------------------------------------------

    fn list_area(&self, model: &Model) -> Option<Rect> {
        let c = self.current(model)?;
        c.mode.elements.iter().find(|el| matches!(el.kind, Kind::List)).map(|el| el.at.resolve(c.w, c.h))
    }

    fn row_h(model: &Model) -> i32 {
        model.settings().pe_fontsize + 3
    }

    fn visible_rows(&self, model: &Model) -> usize {
        self.list_area(model).map_or(1, |r| (r.h / Self::row_h(model)).max(1) as usize)
    }

    /// The first visible row, kept within the list.
    fn scroll(&self, model: &Model) -> usize {
        self.list.scroll.min(model.playlist.len().saturating_sub(self.visible_rows(model)))
    }

    /// The row at window height `y` (past the end when below the last one).
    pub fn row_at(&self, model: &Model, y: i32) -> usize {
        let top = self.list_area(model).map_or(0, |r| r.y);
        self.scroll(model) + ((y - top).max(0) / Self::row_h(model)) as usize
    }

    /// Scrolls so `row` is visible.
    pub fn ensure_visible(&mut self, model: &Model, row: usize) {
        let vis = self.visible_rows(model);
        let scroll = self.scroll(model);
        self.list.scroll = if row < scroll {
            row
        } else if row >= scroll + vis {
            row + 1 - vis
        } else {
            scroll
        };
    }

    /// Keyboard navigation in the list.
    pub fn key_nav(&mut self, model: &mut Model, key: Nav, shift: bool) {
        let len = model.playlist.len();
        if len == 0 {
            return;
        }
        let page = (self.visible_rows(model) / 5).max(1);
        let cur = model.playlist.cursor.min(len - 1);
        let target = match key {
            Nav::Up => cur.saturating_sub(1),
            Nav::Down => (cur + 1).min(len - 1),
            Nav::PageUp => cur.saturating_sub(page),
            Nav::PageDown => (cur + page).min(len - 1),
            Nav::Home => 0,
            Nav::End => len - 1,
        };
        if shift {
            model.playlist.select_range(self.list.anchor.min(len - 1), target);
        } else {
            model.playlist.select_only(target);
            self.list.anchor = target;
        }
        model.playlist.cursor = target;
        self.ensure_visible(model, target);
        model.pl_dirty += 1;
    }

    /// A press on the list: selects, plays on double click, or starts a row drag.
    fn press_row(&mut self, model: &mut Model, ev: &Input) -> bool {
        let i = self.row_at(model, ev.y);
        let len = model.playlist.len();
        if i >= len {
            model.playlist.select_all(false);
            return false;
        }
        if ev.double {
            model.play_index(i);
            return false;
        }
        if ev.shift {
            model.playlist.select_range(self.list.anchor.min(len - 1), i);
        } else if ev.ctrl {
            model.playlist.toggle_selected(i);
            self.list.anchor = i;
        } else {
            if !model.playlist.entry(i).is_some_and(|e| e.selected) {
                model.playlist.select_only(i);
            }
            self.list.anchor = i;
            self.list.drag_row = Some(i);
        }
        model.playlist.cursor = i;
        model.pl_dirty += 1;
        true
    }

    /// Moves the selected rows with a row drag; returns true if they moved.
    fn drag_rows(&mut self, model: &mut Model, y: i32) -> bool {
        let Some(from) = self.list.drag_row else { return false };
        let top = self.list_area(model).map_or(0, |r| r.y);
        let row = (self.scroll(model) as i32 + (y - top).div_euclid(Self::row_h(model))).max(0) as usize;
        if row == from {
            return false;
        }
        let moved = model.playlist.move_selected(row as isize - from as isize);
        self.list.drag_row = Some((from as isize + moved) as usize);
        model.pl_dirty += 1;
        model.update_next();
        self.ensure_visible(model, row.min(model.playlist.len().saturating_sub(1)));
        true
    }

    /// Scrolls so the thumb's middle follows `y` on the scroll bar `r`.
    fn scroll_to(&mut self, model: &Model, r: Rect, thumb: &Look, y: i32) {
        let th = thumb_h(thumb);
        let track = (r.h - th).max(1);
        let max = model.playlist.len().saturating_sub(self.visible_rows(model));
        let frac = ((y - r.y - th / 2) as f32 / track as f32).clamp(0.0, 1.0);
        self.list.scroll = (frac * max as f32).round() as usize;
    }

    // --- popups --------------------------------------------------------------

    pub fn popup_open(&self) -> bool {
        self.popup.is_some()
    }

    pub fn close_popup(&mut self) {
        self.popup = None;
    }

    /// The open popup's items with their places.
    fn popup_items(&self, model: &Model) -> Vec<(PopupItem, Rect)> {
        let Some(((shade, i), _)) = self.popup else { return Vec::new() };
        if shade != bind::view(model, self.key) {
            return Vec::new();
        }
        let Some((Element { kind: Kind::Popup { items, above, .. }, .. }, r)) = self.element(model, i) else { return Vec::new() };
        let mut y = r.y;
        items
            .into_iter()
            .enumerate()
            .map(|(k, item)| {
                let src = item.look.normal.map(|p| p.src).unwrap_or(Rect { x: 0, y: 0, w: 0, h: 0 });
                if k > 0 || above {
                    y -= src.h;
                }
                (item, Rect { x: r.x, y, w: src.w, h: src.h })
            })
            .collect()
    }

    fn popup_hit(&self, model: &Model, x: i32, y: i32) -> Option<(usize, PopupItem)> {
        self.popup_items(model).into_iter().enumerate().find(|(_, (_, r))| r.contains(x, y)).map(|(k, (item, _))| (k, item))
    }

    fn run_popup_item(&mut self, model: &mut Model, item: PopupItem, ev: &Input) -> Out {
        self.close_popup();
        match item.action {
            Action::Click { plain, shift, ctrl } => model.exec(clicked(plain, shift, ctrl, ev)),
            Action::Press(c) => model.exec(c),
            Action::MenuOnPress(menu) | Action::MenuOnRelease(menu) => return Out::Menu(bind::menu(model, menu)),
            Action::Nothing => {}
        }
        Out::Redraw
    }

    fn draw_popup(&self, model: &Model, b: &mut Canvas) {
        let Some(((_, i), hover)) = self.popup else { return };
        let items = self.popup_items(model);
        for (k, (item, r)) in items.iter().enumerate() {
            if let Some(p) = item.look.pick(false, hover == Some(k), true) {
                draw_paint(b, &model.skin, p, *r, Fill::Once);
            }
        }
        if let (false, Some((Element { kind: Kind::Popup { bar: Some(bar), .. }, .. }, r))) = (items.is_empty(), self.element(model, i)) {
            draw_paint(b, &model.skin, bar, r, Fill::Once);
        }
    }

    // --- drawing -------------------------------------------------------------

    pub fn draw(&self, model: &Model, fonts: &mut Fonts, b: &mut Canvas, active: bool) {
        let Some(c) = self.current(model) else { return };
        let active = active || !model.settings().dim_inactive;
        let pressed = self.pressed_index(model);
        for (i, el) in c.mode.elements.iter().enumerate() {
            if !bind::holds(model, el.show, c.w) {
                continue;
            }
            let r = el.at.resolve(c.w, c.h);
            let held = pressed == Some(i);
            let skin = &model.skin;
            match &el.kind {
                Kind::Image { look, fill, lit } => {
                    let on = lit.is_some_and(|f| bind::flag(model, f));
                    if let Some(p) = look.pick(on, false, active) {
                        draw_paint(b, skin, p, r, *fill);
                    }
                }
                Kind::Button { look, .. } => {
                    if let Some(p) = look.pick(false, held, active) {
                        draw_paint(b, skin, p, r, Fill::Once);
                    }
                }
                Kind::Toggle { look, flag, .. } => {
                    if let Some(p) = look.pick(bind::flag(model, *flag), held, active) {
                        draw_paint(b, skin, p, r, Fill::Once);
                    }
                }
                Kind::Slider(s) => self.draw_slider(model, b, s, r, held),
                Kind::Text(t) => {
                    if let Some(text) = bind::text(model, *t) {
                        render::text(b, skin, fonts, r, &text, 0);
                    }
                }
                Kind::Marquee { area, .. } => self.draw_marquee(model, fonts, b, marquee_rect(*area, r)),
                Kind::Clock { align, font_size } => self.draw_clock(model, fonts, b, r, (*align, *font_size)),
                Kind::Vis => model.vis.draw(b, r.x, r.y, r.w, r.h, model.settings(), &skin.viscolor),
                Kind::EqGraph => draw_eq_graph(model, b, r),
                Kind::List => self.draw_list(model, &mut fonts.list, b, r, active),
                Kind::ListScroll { thumb } => {
                    let track = r.h - thumb_h(thumb);
                    let max = model.playlist.len().saturating_sub(self.visible_rows(model));
                    let ty = if max == 0 { 0 } else { (self.scroll(model) as i32 * track) / max as i32 };
                    if let Some(p) = thumb.pick(false, held, true) {
                        draw_paint(b, skin, p, Rect { y: r.y + ty, ..r }, Fill::Once);
                    }
                }
                Kind::Drag { .. } | Kind::Resize | Kind::Popup { .. } => {}
            }
        }
        self.draw_popup(model, b);
    }

    fn draw_list(&self, model: &Model, font: &mut crate::font::Font, b: &mut Canvas, list: Rect, active: bool) {
        let colors = &model.skin.pl;
        b.fill(list.x, list.y, list.w, list.h, colors.normal_bg);
        let s = model.settings();
        let row_h = Self::row_h(model);
        let px = s.pe_fontsize as u32;
        let digits = model.playlist.len().max(1).to_string().len();
        let current = model.playlist.current();
        let scroll = self.scroll(model);
        for vi in 0..self.visible_rows(model) {
            let i = scroll + vi;
            let Some(e) = model.playlist.entry(i) else { break };
            let ry = list.y + vi as i32 * row_h;
            if e.selected {
                b.fill(list.x, ry, list.w, row_h, colors.selected_bg);
            }
            let color = if Some(i) == current { colors.current } else { colors.normal };
            let time = e.length_ms.map(format_duration_ms).unwrap_or_default();
            let tw = b.text_width(font, &time, px);
            let label = match (s.shownumsinpl, s.zeropadplnum) {
                (true, true) => format!("{:0digits$}. {}", i + 1, e.display),
                (true, false) => format!("{}. {}", i + 1, e.display),
                _ => e.display.clone(),
            };
            let fitted = font.fit(&label, b.scale.px(px as i32) as u32, b.scale.px(list.w - tw - 8));
            b.text(font, list.x + 2, ry + 1, &fitted, px, color, list);
            b.text(font, list.x + list.w - tw - 2, ry + 1, &time, px, color, list);
            if i == model.playlist.cursor && active {
                // Focus rectangle (dotted).
                for xx in (list.x..list.x + list.w).step_by(2) {
                    b.set(xx, ry, color);
                    b.set(xx, ry + row_h - 1, color);
                }
            }
        }
    }

    fn draw_slider(&self, model: &Model, b: &mut Canvas, s: &Slider, r: Rect, held: bool) {
        let Some((v, min, max)) = bind::value(model, s.value) else { return };
        let shown = if held && !s.live { self.preview.unwrap_or(v) } else { v };
        if let Some(f) = &s.track {
            let src = f.frame(shown, min, max);
            picture(b, &model.skin, f.picture, src, Rect { x: r.x, y: r.y, ..src });
        }
        if let Some(p) = s.thumb.pick(false, held, true) {
            let off = thumb_offset(s, r, shown, min, max);
            let (x, y) = if s.vertical { (r.x, r.y + off) } else { (r.x + off, r.y) };
            draw_paint(b, &model.skin, p, Rect { x, y, ..r }, Fill::Once);
        }
    }

    fn draw_marquee(&self, model: &Model, fonts: &mut Fonts, b: &mut Canvas, area: Rect) {
        let text = self.marquee_text(model);
        let skin = &model.skin;
        if render::text_width(skin, fonts, &text) > area.w {
            let full = format!("{text}  ***  ");
            let period = render::text_width(skin, fonts, &full);
            let off = self.marquee_offset.rem_euclid(period.max(1));
            render::text(b, skin, fonts, area, &format!("{full}{full}"), off);
        } else {
            render::text(b, skin, fonts, area, &text, 0);
        }
    }

    /// The big time display; paused time blinks once a second.
    fn draw_clock(&self, model: &Model, fonts: &mut Fonts, b: &mut Canvas, r: Rect, how: (Align, Option<i32>)) {
        let sh = model.engine.shared();
        let blink_off = sh.state() == PlayState::Paused
            && (std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_millis() / 500) % 2 == 1;
        if blink_off {
            return;
        }
        let shown = self.seek_preview(model).map(|p| p.max(0) as u64).unwrap_or_else(|| model.engine.position_ms());
        let (secs, neg) = match (model.settings().timeleftmode, sh.length_ms()) {
            (true, Some(l)) => (l.saturating_sub(shown) / 1000, true),
            _ => (shown / 1000, false),
        };
        render::clock(b, &model.skin, fonts, r, secs, neg, how);
    }

    // --- input ---------------------------------------------------------------

    pub fn mouse_down(&mut self, model: &mut Model, ev: &Input) -> Out {
        if self.popup.is_some() {
            if let Some((_, item)) = self.popup_hit(model, ev.x, ev.y) {
                return self.run_popup_item(model, item, ev);
            }
            self.close_popup();
            return Out::Redraw;
        }
        let hit = self.hit(model, ev.x, ev.y);
        let found = hit.and_then(|i| self.element(model, i));
        if ev.right {
            if let Some((Element { kind: Kind::List, .. }, _)) = found {
                // The row menu acts on the row under the mouse.
                let i = self.row_at(model, ev.y);
                if i < model.playlist.len() && !model.playlist.entry(i).is_some_and(|e| e.selected) {
                    model.playlist.select_only(i);
                }
            }
            let menu = found.and_then(|(el, _)| el.menu).unwrap_or(MenuRef::Main);
            return Out::Menu(bind::menu(model, menu));
        }
        let (Some(i), Some((el, r))) = (hit, found) else { return self.empty_press(model) };
        let shade = bind::view(model, self.key);
        match el.kind {
            Kind::Drag { double } => {
                if let (true, Some(c)) = (ev.double, double) {
                    model.exec(c);
                    return Out::None;
                }
                Out::BeginMove
            }
            Kind::Button { action, .. } | Kind::Toggle { action, .. } => match action {
                Action::Press(c) => {
                    model.exec(c);
                    Out::Redraw
                }
                Action::MenuOnPress(menu) => Out::Menu(bind::menu(model, menu)),
                _ => {
                    self.pressed = Some((shade, i));
                    Out::Capture
                }
            },
            Kind::Slider(s) => {
                if let (true, Some(v)) = (ev.double, s.reset) {
                    bind::set_value(model, s.value, v);
                    return Out::Redraw;
                }
                let Some((v, min, max)) = bind::value(model, s.value) else { return Out::None };
                let (p, start) = if s.vertical { (ev.y, r.y) } else { (ev.x, r.x) };
                let thumb = start + thumb_offset(&s, r, v, min, max);
                self.grab = if p >= thumb && p < thumb + s.thumb_len { p - thumb } else { s.thumb_len / 2 };
                self.pressed = Some((shade, i));
                self.slide(model, &s, r, ev.x, ev.y);
                Out::Capture
            }
            Kind::Marquee { double, .. } => {
                if let (true, Some(c)) = (ev.double, double) {
                    model.exec(c);
                    return Out::None;
                }
                self.marquee_drag = Some((ev.x, self.marquee_offset));
                self.pressed = Some((shade, i));
                Out::Capture
            }
            Kind::List => {
                if !self.press_row(model, ev) {
                    return Out::Redraw;
                }
                self.pressed = Some((shade, i));
                Out::Capture
            }
            Kind::ListScroll { thumb } => {
                self.pressed = Some((shade, i));
                self.scroll_to(model, r, &thumb, ev.y);
                Out::Capture
            }
            Kind::Popup { .. } => {
                self.popup = Some(((shade, i), None));
                self.pressed = Some((shade, i));
                Out::Capture
            }
            Kind::Resize => Out::BeginResize,
            _ => Out::None,
        }
    }

    fn empty_press(&self, model: &Model) -> Out {
        // The window's choice is for the window; its bar view has its place.
        if bind::view(model, self.key) == View::Bar {
            return Out::None;
        }
        match self.def(model).map(|d| d.empty_press) {
            Some(EmptyPress::Move) => Out::BeginMove,
            Some(EmptyPress::MoveIfEasy) if model.settings().easymove => Out::BeginMove,
            _ => Out::None,
        }
    }

    /// Moves a slider to the pointer.
    fn slide(&mut self, model: &mut Model, s: &Slider, r: Rect, x: i32, y: i32) {
        let Some((v, min, max)) = bind::value(model, s.value) else { return };
        let (p, start, len) = if s.vertical { (y, r.y, r.h) } else { (x, r.x, r.w) };
        let range = (len - s.thumb_len).max(1) as i64;
        let t = ((p - start - self.grab) as i64).clamp(0, range);
        let mut to = min + (2 * t * (max - min) + range) / (2 * range);
        let mid = (min + max) / 2;
        if (to - mid).abs() < s.detent {
            to = mid;
        }
        if s.live {
            if to != v {
                bind::set_value(model, s.value, to);
            }
        } else {
            self.preview = Some(to);
        }
    }

    /// Follows a drag; returns true if the window needs drawing.
    pub fn mouse_move(&mut self, model: &mut Model, x: i32, y: i32) -> bool {
        if let Some((at, hover)) = self.popup {
            let now = self.popup_hit(model, x, y).map(|(k, _)| k);
            self.popup = Some((at, now));
            return now != hover;
        }
        let Some((el, r)) = self.pressed_index(model).and_then(|i| self.element(model, i)) else { return false };
        match el.kind {
            Kind::Slider(s) => self.slide(model, &s, r, x, y),
            Kind::Marquee { .. } => {
                let Some((sx, so)) = self.marquee_drag else { return false };
                self.marquee_offset = so - (x - sx);
            }
            Kind::List => return self.drag_rows(model, y),
            Kind::ListScroll { thumb } => self.scroll_to(model, r, &thumb, y),
            _ => return false,
        }
        true
    }

    pub fn mouse_up(&mut self, model: &mut Model, ev: &Input) -> Out {
        let pressed = self.pressed_index(model);
        self.pressed = None;
        self.list.drag_row = None;
        if let Some(((_, at), _)) = self.popup {
            // Released on the button that opened it, even under an item that
            // covers it: a click, so the popup stays open.
            if pressed == Some(at) && self.hit(model, ev.x, ev.y) == Some(at) {
                return Out::Redraw;
            }
            if let Some((_, item)) = self.popup_hit(model, ev.x, ev.y) {
                return self.run_popup_item(model, item, ev);
            }
            self.close_popup();
            return Out::Redraw;
        }
        let Some(i) = pressed else { return Out::None };
        let over = self.hit(model, ev.x, ev.y) == Some(i);
        let Some((el, _)) = self.element(model, i) else { return Out::Redraw };
        match el.kind {
            Kind::Slider(s) => {
                if let Some(v) = self.preview.take() {
                    bind::set_value(model, s.value, v);
                }
                Out::Redraw
            }
            Kind::Marquee { .. } => {
                self.marquee_drag = None;
                self.last_scroll = Instant::now() + Duration::from_secs(1);
                Out::Redraw
            }
            Kind::Button { action, .. } | Kind::Toggle { action, .. } if over => match action {
                Action::Click { plain, shift, ctrl } => {
                    model.exec(clicked(plain, shift, ctrl, ev));
                    Out::Redraw
                }
                Action::MenuOnRelease(menu) => Out::Menu(bind::menu(model, menu)),
                _ => Out::Redraw,
            },
            _ => Out::Redraw,
        }
    }

    pub fn wheel(&mut self, model: &mut Model, x: i32, y: i32, steps: i32) {
        if let Some((Element { kind: Kind::Slider(s), .. }, _)) = self.hit(model, x, y).and_then(|i| self.element(model, i)) {
            if s.wheel != 0 {
                if let Some((v, _, _)) = bind::value(model, s.value) {
                    bind::set_value(model, s.value, v + s.wheel * steps as i64);
                }
                return;
            }
        }
        match self.def(model).map(|d| d.wheel) {
            Some(Wheel::Volume(step)) => model.set_volume(model.settings().volume + steps * step),
            Some(Wheel::ScrollList) => self.list.scroll = self.scroll(model).saturating_add_signed(-steps as isize * 3),
            None => {}
        }
    }
}

/// The command a click runs with the held modifiers.
fn clicked(plain: Command, shift: Option<Command>, ctrl: Option<Command>, ev: &Input) -> Command {
    match (ev.ctrl, ev.shift) {
        (true, _) => ctrl.unwrap_or(plain),
        (false, true) => shift.unwrap_or(plain),
        _ => plain,
    }
}

fn thumb_h(thumb: &Look) -> i32 {
    thumb.normal.map_or(0, |p| p.src.h)
}

/// How far the thumb sits from the slider's start for value `v`.
fn thumb_offset(s: &Slider, r: Rect, v: i64, min: i64, max: i64) -> i32 {
    let len = if s.vertical { r.h } else { r.w };
    let range = (len - s.thumb_len).max(0) as i64;
    ((v - min).clamp(0, (max - min).max(0)) * range / (max - min).max(1)) as i32
}

/// Where `mode` covers a `size` window at least half, drawn at `drawn` for
/// a window shown at `shown`: its images, and the list, vis and EQ graph,
/// which paint all of their rectangle. The images are drawn on black and on
/// white: a pixel they cover comes out (nearly) the same on both.
pub fn image_mask(model: &Model, mode: &Mode, size: (i32, i32), drawn: Scale, shown: Scale) -> Mask {
    let (w, h) = size;
    let paint = |bg: u32| {
        let mut bitmap = Bitmap::new(drawn.px(w), drawn.px(h), bg);
        let mut b = Canvas::new(&mut bitmap, drawn);
        for el in mode.elements.iter().filter(|el| bind::holds(model, el.show, w)) {
            let r = el.at.resolve(w, h);
            match &el.kind {
                Kind::Image { look: Look { normal: Some(p), .. }, fill, .. } => draw_paint(&mut b, &model.skin, *p, r, *fill),
                Kind::List | Kind::Vis | Kind::EqGraph => b.fill(r.x, r.y, r.w, r.h, 0x808080),
                _ => {}
            }
        }
        bitmap
    };
    let (black, white) = (paint(0), paint(0xffffff));
    let green = |c: u32| (c >> 8) & 0xff;
    let on = black.px.iter().zip(&white.px).map(|(&k, &w)| green(w).saturating_sub(green(k)) < 128).collect();
    Mask { w: black.w, h: black.h, on, scale: if drawn.is_one() { shown } else { Scale::ONE } }
}

fn draw_paint(b: &mut Canvas, skin: &Skin, p: Paint, r: Rect, fill: Fill) {
    match fill {
        Fill::Once => picture(b, skin, p.picture, p.src, Rect { x: r.x + p.dx, y: r.y + p.dy, ..p.src }),
        Fill::Tile => {
            let (tw, th) = (p.src.w.max(1), p.src.h.max(1));
            let mut y = r.y;
            while y < r.y + r.h {
                let mut x = r.x;
                while x < r.x + r.w {
                    let (w, h) = (tw.min(r.x + r.w - x), th.min(r.y + r.h - y));
                    picture(b, skin, p.picture, p.src, Rect { x, y, w, h });
                    x += tw;
                }
                y += th;
            }
        }
    }
}

/// Spline through the band values, coloured from the skin's colour strip.
fn draw_eq_graph(model: &Model, b: &mut Canvas, r: Rect) {
    let skin = &model.skin;
    let (gx, gy) = (r.x, r.y);
    sprite(b, skin, Sheet::EqMain, e::GRAPH_BG, gx, gy);
    let preamp_y = gy + 1 + model.settings().preamp * 16 / 63;
    sprite(b, skin, Sheet::EqMain, e::PREAMP_LINE, gx, preamp_y);
    let bands = model.settings().eq_bands();
    let ys: Vec<f32> = bands.iter().map(|&v| v as f32 * 18.0 / 63.0).collect();
    let colors = skin.sheet(Sheet::EqMain);
    let mut prev: Option<i32> = None;
    for px in 0..113 {
        // Catmull-Rom over the 10 points spread across 113 px.
        let t = px as f32 / 112.0 * 9.0;
        let i = (t.floor() as usize).min(8);
        let f = t - i as f32;
        let p = |k: isize| ys[(i as isize + k).clamp(0, 9) as usize];
        let (p0, p1, p2, p3) = (p(-1), p(0), p(1), p(2));
        let y = 0.5
            * (2.0 * p1 + (-p0 + p2) * f + (2.0 * p0 - 5.0 * p1 + 4.0 * p2 - p3) * f * f + (-p0 + 3.0 * p1 - 3.0 * p2 + p3) * f * f * f);
        let y = (y.round() as i32).clamp(0, 18);
        let (lo, hi) = match prev {
            Some(py) => (py.min(y), py.max(y)),
            None => (y, y),
        };
        for yy in lo..=hi {
            let c = colors.get(e::GRAPH_COLORS.x, e::GRAPH_COLORS.y + yy).unwrap_or(0xffffff);
            b.set(gx + px, gy + yy, c);
        }
        prev = Some(y);
    }
}

/// Where a marquee in `r` writes the title.
fn marquee_rect(area: Option<Rect>, r: Rect) -> Rect {
    area.map_or(r, |a| Rect { x: r.x + a.x, y: r.y + a.y, ..a })
}

#[cfg(test)]
mod tests {
    use super::*;
    use popamp_skin::CursorKind as C;

    fn input(x: i32, y: i32) -> Input {
        Input { x, y, right: false, shift: false, ctrl: false, double: false }
    }

    #[test]
    fn main_cursors_follow_the_control_under_the_mouse() {
        let mut m = Model::for_tests();
        let w = SkinWin::new(WindowKey::Main);
        let at = |m: &Model, x, y| w.cursor(m, x, y);
        assert_eq!(at(&m, 9, 5), C::MainMenu);
        assert_eq!(at(&m, 247, 5), C::Min);
        assert_eq!(at(&m, 267, 5), C::Close);
        assert_eq!(at(&m, 257, 5), C::WinBut);
        assert_eq!(at(&m, 100, 5), C::TitleBar);
        assert_eq!(at(&m, 120, 60), C::VolBal);
        assert_eq!(at(&m, 180, 27), C::SongName);
        assert_eq!(at(&m, 50, 97), C::Normal);
        m.settings_mut().windowshade = true;
        assert_eq!(at(&m, 9, 5), C::MMenu);
        assert_eq!(at(&m, 100, 5), C::WSNormal);
    }

    #[test]
    fn main_tooltips_name_the_control_under_the_mouse() {
        let mut m = Model::for_tests();
        let w = SkinWin::new(WindowKey::Main);
        assert_eq!(w.tooltip(&m, 13, 36).as_deref(), Some("Toggle Always-On-Top"));
        assert_eq!(w.tooltip(&m, 180, 27).as_deref(), Some("Song Title (right-click for options)"));
        assert_eq!(w.tooltip(&m, 9, 5).as_deref(), Some("Main Menu"));
        assert_eq!(w.tooltip(&m, 50, 97), None);
        // The clutter bar is hidden in windowshade mode.
        m.settings_mut().windowshade = true;
        assert_eq!(w.tooltip(&m, 13, 36), None);
    }

    #[test]
    fn a_round_button_takes_the_mouse_on_its_shape_only() {
        let mut m = Model::for_tests();
        let dot = br##"<svg xmlns="http://www.w3.org/2000/svg" width="20" height="20"><circle cx="10" cy="10" r="10"/></svg>"##;
        let toml = "[meta]\nname = \"Round\"\nformat_version = 1\n[[window]]\nkey = \"main\"\nnormal.size = [275, 116]\n\
            [[window.normal.element]]\ntype = \"button\"\nrect = [10, 10, 20, 20]\nnormal = \"dot.svg\"\nhit = \"dot.svg\"\nclick = \"play\"\ntip = \"Play\"\n";
        let files = [("skin.toml".to_string(), toml.as_bytes().to_vec()), ("dot.svg".to_string(), dot.to_vec())].into();
        m.skin = popamp_skin::vector::load(&files, None).unwrap();
        let w = SkinWin::new(WindowKey::Main);
        assert_eq!(w.tooltip(&m, 20, 20).as_deref(), Some("Play"));
        assert_eq!(w.tooltip(&m, 11, 11), None, "the corner is not the button");
    }

    #[test]
    fn a_press_on_an_empty_spot_moves_the_window_but_not_its_bar_view() {
        let mut m = Model::for_tests();
        let toml =
            "[meta]\nname = \"Both\"\nformat_version = 1\n[[window]]\nkey = \"main\"\nnormal.size = [275, 116]\nbar.size = [400, 20]\n";
        m.skin = popamp_skin::vector::load(&[("skin.toml".to_string(), toml.as_bytes().to_vec())].into(), None).unwrap();
        let mut w = SkinWin::new(WindowKey::Main);
        assert!(matches!(w.mouse_down(&mut m, &input(5, 5)), Out::BeginMove));
        m.settings_mut().main_bar = true;
        assert!(matches!(w.mouse_down(&mut m, &input(5, 5)), Out::None), "a bar stays at its screen edge");
    }

    #[test]
    fn a_popup_opening_above_its_button_stays_open_after_a_click() {
        let mut m = Model::for_tests();
        let dot = br##"<svg xmlns="http://www.w3.org/2000/svg" width="20" height="20"><circle cx="10" cy="10" r="10"/></svg>"##;
        let toml = "[meta]\nname = \"Above\"\nformat_version = 1\n[[window]]\nkey = \"main\"\nnormal.size = [275, 116]\n\
            [[window.normal.element]]\ntype = \"popup\"\nrect = [10, 80, 20, 20]\nabove = true\n\
            [[window.normal.element.item]]\nnormal = \"dot.svg\"\nclick = \"play\"\n";
        let files = [("skin.toml".to_string(), toml.as_bytes().to_vec()), ("dot.svg".to_string(), dot.to_vec())].into();
        m.skin = popamp_skin::vector::load(&files, None).unwrap();
        let mut w = SkinWin::new(WindowKey::Main);
        w.mouse_down(&mut m, &input(20, 90));
        w.mouse_up(&mut m, &input(20, 90));
        assert!(w.popup_open(), "the button is not under an item");
        assert_eq!(w.popup_hit(&m, 20, 70).map(|(k, _)| k), Some(0), "the item sits on top of the button");
        assert_eq!(w.popup_hit(&m, 20, 90).map(|(k, _)| k), None);
    }

    #[test]
    fn a_see_through_window_shows_where_its_images_are() {
        let mut m = Model::for_tests();
        let dot = br##"<svg xmlns="http://www.w3.org/2000/svg" width="20" height="20"><circle cx="10" cy="10" r="10"/></svg>"##;
        let toml = "[meta]\nname = \"Round\"\nformat_version = 1\n[[window]]\nkey = \"main\"\n[window.normal]\nsize = [40, 20]\n\
            see_through = true\n[[window.normal.element]]\ntype = \"image\"\nrect = [0, 0, 40, 20]\nnormal = \"dot.svg\"\nfill = \"tile\"\n\
            [[window.normal.element]]\ntype = \"list\"\nrect = [20, 0, 4, 4]\n";
        let files = [("skin.toml".to_string(), toml.as_bytes().to_vec()), ("dot.svg".to_string(), dot.to_vec())].into();
        m.skin = popamp_skin::vector::load(&files, None).unwrap();
        let mode = &m.skin.layout.window(WindowKey::Main).unwrap().normal;
        let x2 = Scale::new(200, popamp_core::config::Scaling::Sharp);
        let mask = image_mask(&m, mode, (40, 20), x2, x2);
        assert_eq!((mask.w, mask.h), (80, 40), "drawn at screen size");
        let on = |x: i32, y: i32| mask.on[(y * mask.w + x) as usize];
        assert!(on(20, 20) && on(60, 20), "the middles of both tiles");
        assert!(!on(1, 1) && !on(78, 38), "their corners");
        assert!(on(41, 1), "a list paints all of its rectangle");
    }

    #[test]
    fn clicks_run_the_button_command_on_release() {
        let mut m = Model::for_tests();
        let mut w = SkinWin::new(WindowKey::Main);
        // Shuffle toggle at (164, 89).
        assert!(matches!(w.mouse_down(&mut m, &input(170, 95)), Out::Capture));
        assert!(!m.settings().shuffle, "nothing happens before release");
        w.mouse_up(&mut m, &input(170, 95));
        assert!(m.settings().shuffle);
        // Released away from the button: nothing.
        w.mouse_down(&mut m, &input(170, 95));
        w.mouse_up(&mut m, &input(10, 100));
        assert!(m.settings().shuffle);
    }

    #[test]
    fn volume_follows_the_drag_and_balance_snaps_to_centre() {
        let mut m = Model::for_tests();
        let mut w = SkinWin::new(WindowKey::Main);
        // Volume: 68 px wide at x 107, 14 px thumb; grabbed off the thumb at its centre.
        w.mouse_down(&mut m, &input(107 + 7, 60));
        assert_eq!(m.settings().volume, 0);
        w.mouse_move(&mut m, 107 + 7 + 54, 60);
        assert_eq!(m.settings().volume, 255);
        w.mouse_move(&mut m, 107 + 7 + 27, 60);
        assert_eq!(m.settings().volume, 128);
        w.mouse_up(&mut m, &input(0, 0));
        // Balance at x 177, 38 px: a little off centre snaps to it.
        m.set_balance(-127);
        w.mouse_down(&mut m, &input(177 + 7 + 13, 60));
        assert_eq!(m.settings().balance, 0);
        w.mouse_up(&mut m, &input(0, 0));
    }

    #[test]
    fn wheel_changes_the_volume_by_default() {
        let mut m = Model::for_tests();
        m.set_volume(100);
        let mut w = SkinWin::new(WindowKey::Main);
        w.wheel(&mut m, 50, 50, 2);
        assert_eq!(m.settings().volume, 116);
    }

    #[test]
    fn eq_cursors_follow_the_control_under_the_mouse() {
        let m = Model::for_tests();
        let w = SkinWin::new(WindowKey::Eq);
        assert_eq!(w.cursor(&m, 267, 5), C::EqClose);
        assert_eq!(w.cursor(&m, 25, 50), C::EqSlid);
        assert_eq!(w.cursor(&m, 82, 50), C::EqSlid);
        assert_eq!(w.cursor(&m, 100, 5), C::EqTitle);
        assert_eq!(w.cursor(&m, 5, 105), C::EqNormal);
    }

    #[test]
    fn eq_sliders_drag_reset_and_wheel() {
        let mut m = Model::for_tests();
        let mut w = SkinWin::new(WindowKey::Eq);
        // Band 0 at x 78, y 38, 63 px tall, 11 px thumb: top is +12 dB (0).
        w.mouse_down(&mut m, &input(84, 38 + 5));
        w.mouse_move(&mut m, 84, 38);
        assert_eq!(m.settings().eq_bands()[0], 0);
        w.mouse_move(&mut m, 84, 38 + 5 + 52);
        assert_eq!(m.settings().eq_bands()[0], 63);
        w.mouse_up(&mut m, &input(84, 100));
        w.mouse_down(&mut m, &Input { double: true, ..input(84, 60) });
        assert_eq!(m.settings().eq_bands()[0], 31, "double click resets");
        w.wheel(&mut m, 84, 60, 1);
        assert_eq!(m.settings().eq_bands()[0], 29, "wheel up raises the band");
        // Empty space moves the window.
        assert!(matches!(w.mouse_down(&mut m, &input(5, 105)), Out::BeginMove));
    }

    /// A 275x116 playlist (four 13 px rows from y 20) holding `n` tracks.
    fn playlist(n: usize) -> (Model, SkinWin) {
        let mut m = Model::for_tests();
        m.playlist.extend((0..n).map(|i| popamp_core::playlist::Entry::new(format!("/m/{i:02}.mp3"))));
        (m.settings_mut().pe_width, m.settings_mut().pe_height) = (275, 116);
        (m, SkinWin::new(WindowKey::Playlist))
    }

    fn selected(m: &Model) -> Vec<usize> {
        m.playlist.selected_indices()
    }

    #[test]
    fn playlist_cursors_follow_the_control_under_the_mouse() {
        let (mut m, w) = playlist(0);
        assert_eq!(w.cursor(&m, 267, 5), C::PClose);
        assert_eq!(w.cursor(&m, 257, 5), C::PWinBut);
        assert_eq!(w.cursor(&m, 100, 5), C::PTBar);
        assert_eq!(w.cursor(&m, 100, 50), C::PNormal);
        assert_eq!(w.cursor(&m, 262, 40), C::PVScroll);
        assert_eq!(w.cursor(&m, 268, 106), C::PSize);
        m.settings_mut().pe_ws = true;
        assert_eq!(w.cursor(&m, 100, 5), C::PWSNorm);
        assert_eq!(w.cursor(&m, 267, 5), C::PClose);
    }

    #[test]
    fn rows_select_with_modifiers_and_move_by_drag() {
        let (mut m, mut w) = playlist(6);
        w.mouse_down(&mut m, &input(50, 20 + 13 + 2));
        w.mouse_up(&mut m, &input(50, 35));
        assert_eq!(selected(&m), [1]);
        w.mouse_down(&mut m, &Input { shift: true, ..input(50, 20 + 3 * 13 + 2) });
        w.mouse_up(&mut m, &input(50, 61));
        assert_eq!(selected(&m), [1, 2, 3]);
        w.mouse_down(&mut m, &Input { ctrl: true, ..input(50, 22) });
        w.mouse_up(&mut m, &input(50, 22));
        assert_eq!(selected(&m), [0, 1, 2, 3]);
        // A plain press on a selected row keeps the selection and drags it.
        w.mouse_down(&mut m, &input(50, 22));
        w.mouse_move(&mut m, 50, 22 + 13);
        w.mouse_up(&mut m, &input(50, 35));
        assert_eq!(m.playlist.entry(0).unwrap().location, "/m/04.mp3");
        assert_eq!(selected(&m), [1, 2, 3, 4]);
        // Below the last row: select nothing.
        let (mut m, mut w) = playlist(2);
        m.playlist.select_only(0);
        w.mouse_down(&mut m, &input(50, 70));
        assert!(selected(&m).is_empty());
    }

    #[test]
    fn right_click_on_a_row_selects_it_for_the_row_menu() {
        let (mut m, mut w) = playlist(3);
        m.playlist.select_only(0);
        let out = w.mouse_down(&mut m, &Input { right: true, ..input(50, 20 + 2 * 13 + 2) });
        assert!(matches!(out, Out::Menu(_)));
        assert_eq!(selected(&m), [2]);
    }

    #[test]
    fn wheel_keys_and_scroll_bar_scroll_the_list() {
        let (mut m, mut w) = playlist(20);
        w.wheel(&mut m, 50, 50, -2);
        assert_eq!(w.scroll(&m), 6);
        w.wheel(&mut m, 50, 50, -10);
        assert_eq!(w.scroll(&m), 16, "stops at the last page");
        w.wheel(&mut m, 50, 50, 10);
        assert_eq!(w.scroll(&m), 0);
        w.key_nav(&mut m, Nav::End, false);
        assert_eq!((m.playlist.cursor, w.scroll(&m)), (19, 16));
        w.key_nav(&mut m, Nav::Home, true);
        assert_eq!((selected(&m).len(), w.scroll(&m)), (20, 0));
        // Scroll bar at x 260, y 20..78 with an 18 px thumb: bottom is the end.
        w.mouse_down(&mut m, &input(262, 77));
        assert_eq!(w.scroll(&m), 16);
        w.mouse_move(&mut m, 262, 20);
        assert_eq!(w.scroll(&m), 0);
        w.mouse_up(&mut m, &input(262, 20));
        assert_eq!(w.row_at(&m, 20 + 13), 1);
    }

    #[test]
    fn popup_buttons_open_picture_menus() {
        let (mut m, mut w) = playlist(3);
        // SEL at (72, 86): its items stack upward 18 px each.
        assert!(matches!(w.mouse_down(&mut m, &input(80, 100)), Out::Capture));
        assert!(w.popup_open());
        w.mouse_up(&mut m, &input(80, 100));
        assert!(w.popup_open(), "released on its own button: a click leaves it open");
        assert_eq!(selected(&m), [] as [usize; 0], "the item over the button did not run");
        w.mouse_down(&mut m, &input(80, 100));
        assert!(!w.popup_open());
        assert_eq!(selected(&m), [0, 1, 2], "a second click on the bottom item: Select all");
        m.playlist.select_all(false);
        w.mouse_down(&mut m, &input(80, 100));
        assert!(w.mouse_move(&mut m, 80, 86 - 36 + 5), "hovering an item redraws");
        w.mouse_up(&mut m, &input(80, 86 - 36 + 5));
        assert!(!w.popup_open());
        assert_eq!(selected(&m), [0, 1, 2], "held and released on Invert selection");
        // Dragged off and released elsewhere: closed, nothing run.
        w.mouse_down(&mut m, &input(80, 100));
        w.mouse_up(&mut m, &input(200, 40));
        assert!(!w.popup_open());
        assert_eq!(selected(&m), [0, 1, 2]);
    }

    #[test]
    fn resizing_snaps_to_the_layout_steps() {
        let (mut m, _) = playlist(0);
        assert!(crate::bind::resize(&mut m, WindowKey::Playlist, (275, 116), 30, 10));
        assert_eq!(crate::bind::window_size(&m, WindowKey::Playlist), (300, 116));
        assert!(!crate::bind::resize(&mut m, WindowKey::Playlist, (275, 116), 20, -100));
        assert!(!crate::bind::resize(&mut m, WindowKey::Main, (275, 116), 100, 100));
    }
}
