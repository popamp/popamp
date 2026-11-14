//! A tiny set of controls for Popamp's own dialogs: labels, text fields,
//! lists, buttons, check boxes, group frames and a list of pages. A
//! [`Form`] holds them,
//! turns mouse and keys into [`Event`]s and draws itself in the colours of
//! the pop-up menus. Coordinates are dialog pixels, as at ×1; a [`Pen`]
//! draws them at the size of the skin's windows.

use crate::menu::{BG, BORDER, DIM, FONT_PX, HILITE, TEXT};
use crate::pen::Pen;
use popamp_skin::bitmap::mix;
use popamp_skin::Bitmap;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Rect {
    pub x: i32,
    pub y: i32,
    pub w: i32,
    pub h: i32,
}

impl Rect {
    pub const fn new(x: i32, y: i32, w: i32, h: i32) -> Rect {
        Rect { x, y, w, h }
    }

    pub fn contains(&self, x: i32, y: i32) -> bool {
        x >= self.x && y >= self.y && x < self.x + self.w && y < self.y + self.h
    }
}

/// The keys a form understands; the window code translates real key events.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Key {
    Char(char),
    Backspace,
    Delete,
    Left,
    Right,
    Home,
    End,
    Up,
    Down,
    PageUp,
    PageDown,
    Enter,
    Escape,
    Tab,
}

/// One control. `A` names what a button or check box stands for.
#[derive(Debug, Clone)]
pub enum Widget<A> {
    Label {
        text: String,
        dim: bool,
    },
    Input {
        text: String,
        cursor: usize,
    },
    List {
        rows: Vec<String>,
        selected: Option<usize>,
        scroll: usize,
    },
    Button {
        label: String,
        action: A,
        default: bool,
    },
    Check {
        label: String,
        checked: bool,
        action: A,
    },
    Group {
        label: String,
    },
    /// A row of segments, one of them picked: tabs, or a choice of order.
    Tabs {
        labels: Vec<String>,
        selected: usize,
        action: A,
    },
    /// The pages of the form, one under another; picking one shows the
    /// controls on it.
    Pages {
        labels: Vec<String>,
        selected: usize,
        action: A,
    },
    /// A picture, or `note` in an empty frame until it arrives.
    Image {
        image: Option<Bitmap>,
        note: String,
    },
}

/// What happened to the form.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Event<A> {
    None,
    /// A button was clicked, or Enter pressed with a default button.
    Clicked(A),
    Toggled(A, bool),
    /// Another segment of a [`Widget::Tabs`] or page of a [`Widget::Pages`] was picked.
    Picked(A, usize),
    /// The text of a field changed.
    Edited,
    /// The list selection moved.
    Selected,
    /// A list row was double-clicked, or Enter pressed without a default button.
    Activated,
    /// Escape.
    Cancel,
}

pub const ROW_H: i32 = 18;
pub const BUTTON: (i32, i32) = (80, 24);
/// The height of one entry of a [`Widget::Pages`].
pub const PAGE_H: i32 = 24;
/// Text fields and lists are a little darker than the dialog.
const FIELD_BG: u32 = darken(BG);

const fn darken(c: u32) -> u32 {
    let (r, g, b) = ((c >> 16) & 0xff, (c >> 8) & 0xff, c & 0xff);
    ((r * 3 / 4) << 16) | ((g * 3 / 4) << 8) | (b * 3 / 4)
}

pub struct Form<A> {
    pub w: i32,
    pub h: i32,
    controls: Vec<(Rect, Widget<A>)>,
    /// The page each control is on; `None`: on every page.
    pages: Vec<Option<usize>>,
    /// The page shown.
    page: usize,
    focus: Option<usize>,
    /// The button held down by the mouse.
    pressed: Option<usize>,
}

impl<A: Copy + PartialEq> Form<A> {
    pub fn new(w: i32, h: i32) -> Form<A> {
        Form { w, h, controls: Vec::new(), pages: Vec::new(), page: 0, focus: None, pressed: None }
    }

    /// Adds a control shown on every page and returns its index. The first
    /// text field or list shown gets the keyboard.
    pub fn add(&mut self, rect: Rect, widget: Widget<A>) -> usize {
        self.put(None, rect, widget)
    }

    /// Adds a control shown only while `page` is.
    pub fn add_on(&mut self, page: usize, rect: Rect, widget: Widget<A>) -> usize {
        self.put(Some(page), rect, widget)
    }

    fn put(&mut self, page: Option<usize>, rect: Rect, widget: Widget<A>) -> usize {
        let i = self.controls.len();
        self.controls.push((rect, widget));
        self.pages.push(page);
        if self.focus.is_none() && self.focusable(i) {
            self.focus = Some(i);
        }
        i
    }

    /// Whether control `i` is on the page shown.
    fn shown(&self, i: usize) -> bool {
        self.pages[i].is_none_or(|p| p == self.page)
    }

    /// A text field or list on the page shown.
    fn focusable(&self, i: usize) -> bool {
        self.shown(i) && matches!(self.controls[i].1, Widget::Input { .. } | Widget::List { .. })
    }

    /// The page shown.
    #[cfg(test)]
    pub fn shown_page(&self) -> usize {
        self.page
    }

    /// The page control `i` is on; `None`: every page.
    #[cfg(test)]
    pub fn page_of(&self, i: usize) -> Option<usize> {
        self.pages[i]
    }

    /// Shows `page`; the keyboard goes to its first field or list when it
    /// was on another page.
    fn show_page(&mut self, page: usize) {
        self.page = page;
        if !self.focus.is_some_and(|f| self.shown(f)) {
            self.focus = (0..self.controls.len()).find(|&i| self.focusable(i));
        }
    }

    #[cfg(test)]
    pub fn widget(&self, i: usize) -> &Widget<A> {
        &self.controls[i].1
    }

    #[cfg(all(test, feature = "plugins"))]
    pub fn count(&self) -> usize {
        self.controls.len()
    }

    #[cfg(test)]
    pub fn count_all(&self) -> usize {
        self.controls.len()
    }

    #[cfg(test)]
    pub fn rect(&self, i: usize) -> Rect {
        self.controls[i].0
    }

    /// Changes the text of label or button `i`.
    pub fn set_label(&mut self, i: usize, new: String) {
        if let Widget::Label { text: old, .. } | Widget::Button { label: old, .. } = &mut self.controls[i].1 {
            *old = new;
        }
    }

    /// Shows `image` in image `i`, or `note` when there is none.
    pub fn set_image(&mut self, i: usize, new: Option<Bitmap>, new_note: &str) {
        if let Widget::Image { image, note } = &mut self.controls[i].1 {
            *image = new;
            *note = new_note.into();
        }
    }

    /// Appends rows to list `i`, keeping the selection and scroll.
    pub fn push_rows(&mut self, i: usize, more: impl IntoIterator<Item = String>) {
        if let Widget::List { rows, .. } = &mut self.controls[i].1 {
            rows.extend(more);
        }
    }

    /// The text of field `i` (empty for other controls).
    pub fn text(&self, i: usize) -> &str {
        match &self.controls[i].1 {
            Widget::Input { text, .. } => text,
            _ => "",
        }
    }

    /// The texts of the labels, in the order they were added.
    #[cfg(all(test, target_os = "macos"))]
    pub fn labels(&self) -> Vec<&str> {
        self.controls
            .iter()
            .filter_map(|(_, w)| match w {
                Widget::Label { text, .. } => Some(text.as_str()),
                _ => None,
            })
            .collect()
    }

    /// Whether check box `i` is ticked.
    #[cfg(linux_like)]
    pub fn checked(&self, i: usize) -> bool {
        matches!(self.controls[i].1, Widget::Check { checked: true, .. })
    }

    /// The picked segment of tabs `i`.
    pub fn tab(&self, i: usize) -> usize {
        match &self.controls[i].1 {
            Widget::Tabs { selected, .. } => *selected,
            _ => 0,
        }
    }

    /// The selected row of list `i`.
    pub fn selected(&self, i: usize) -> Option<usize> {
        match &self.controls[i].1 {
            Widget::List { selected, .. } => *selected,
            _ => None,
        }
    }

    /// Replaces the rows of list `i`, selecting `select` (clamped).
    pub fn set_rows(&mut self, i: usize, new_rows: Vec<String>, select: Option<usize>) {
        if let Widget::List { rows, selected, scroll } = &mut self.controls[i].1 {
            *selected = select.filter(|_| !new_rows.is_empty()).map(|s| s.min(new_rows.len() - 1));
            *rows = new_rows;
            *scroll = 0;
        }
        self.scroll_to_selection(i);
    }

    fn default_action(&self) -> Option<A> {
        self.controls.iter().enumerate().find_map(|(i, (_, w))| match w {
            Widget::Button { action, default: true, .. } if self.shown(i) => Some(*action),
            _ => None,
        })
    }

    /// The list the wheel and the arrows move: the one with the keyboard,
    /// else the first shown.
    fn list_index(&self) -> Option<usize> {
        let list = |i: &usize| self.shown(*i) && matches!(self.controls[*i].1, Widget::List { .. });
        self.focus.filter(list).or_else(|| (0..self.controls.len()).find(list))
    }

    /// Visible rows of the list at `i`.
    fn page(&self, i: usize) -> usize {
        ((self.controls[i].0.h - 4) / ROW_H).max(1) as usize
    }

    fn scroll_to_selection(&mut self, i: usize) {
        let page = self.page(i);
        if let Widget::List { selected: Some(s), scroll, .. } = &mut self.controls[i].1 {
            if *s < *scroll {
                *scroll = *s;
            } else if *s >= *scroll + page {
                *scroll = *s + 1 - page;
            }
        }
    }

    pub fn mouse_down(&mut self, x: i32, y: i32, double: bool) -> Event<A> {
        let Some(i) = (0..self.controls.len()).find(|&i| {
            let (r, w) = &self.controls[i];
            self.shown(i) && r.contains(x, y) && !matches!(w, Widget::Label { .. } | Widget::Group { .. } | Widget::Image { .. })
        }) else {
            return Event::None;
        };
        let rect = self.controls[i].0;
        match &mut self.controls[i].1 {
            Widget::Input { text, cursor } => {
                self.focus = Some(i);
                *cursor = text.chars().count();
                Event::None
            }
            Widget::List { rows, selected, scroll } => {
                self.focus = Some(i);
                let row = ((y - rect.y - 2) / ROW_H) as usize + *scroll;
                if row >= rows.len() {
                    return Event::None;
                }
                *selected = Some(row);
                if double {
                    Event::Activated
                } else {
                    Event::Selected
                }
            }
            Widget::Button { .. } => {
                self.pressed = Some(i);
                Event::None
            }
            Widget::Check { checked, action, .. } => {
                *checked = !*checked;
                Event::Toggled(*action, *checked)
            }
            Widget::Tabs { labels, selected, action } => {
                let n = ((x - rect.x) * labels.len() as i32 / rect.w.max(1)).clamp(0, labels.len() as i32 - 1) as usize;
                let picked = std::mem::replace(selected, n) != n;
                if picked {
                    Event::Picked(*action, n)
                } else {
                    Event::None
                }
            }
            Widget::Pages { .. } => self.pick_page(i, y),
            Widget::Label { .. } | Widget::Group { .. } | Widget::Image { .. } => Event::None,
        }
    }

    /// A press at height `y` on pages `i` shows the page there.
    fn pick_page(&mut self, i: usize, y: i32) -> Event<A> {
        let (rect, widget) = &mut self.controls[i];
        let Widget::Pages { labels, selected, action } = widget else { return Event::None };
        let n = ((y - rect.y - 2) / PAGE_H) as usize;
        if n >= labels.len() || n == *selected {
            return Event::None;
        }
        *selected = n;
        let action = *action;
        self.show_page(n);
        Event::Picked(action, n)
    }

    /// A button fires when the mouse is released over it.
    pub fn mouse_up(&mut self, x: i32, y: i32) -> Event<A> {
        match self.pressed.take().map(|i| &self.controls[i]) {
            Some((r, Widget::Button { action, .. })) if r.contains(x, y) => Event::Clicked(*action),
            _ => Event::None,
        }
    }

    /// Scrolls the list by `steps` wheel notches (positive = up).
    pub fn wheel(&mut self, steps: i32) {
        let Some(i) = self.list_index() else { return };
        let page = self.page(i);
        if let Widget::List { rows, scroll, .. } = &mut self.controls[i].1 {
            let max = rows.len().saturating_sub(page) as i32;
            *scroll = (*scroll as i32 - steps * 3).clamp(0, max) as usize;
        }
    }

    pub fn key(&mut self, key: Key) -> Event<A> {
        match key {
            Key::Escape => return Event::Cancel,
            Key::Enter => return self.default_action().map_or(Event::Activated, Event::Clicked),
            Key::Tab => {
                self.focus_next();
                return Event::None;
            }
            // The arrows move the list even while typing in a search field.
            Key::Up | Key::Down | Key::PageUp | Key::PageDown => return self.move_selection(key),
            _ => {}
        }
        let Some(f) = self.focus else { return Event::None };
        let Widget::Input { text, cursor } = &mut self.controls[f].1 else { return Event::None };
        if edit(text, cursor, key) {
            Event::Edited
        } else {
            Event::None
        }
    }

    fn focus_next(&mut self) {
        let focusable: Vec<usize> = (0..self.controls.len()).filter(|&i| self.focusable(i)).collect();
        if focusable.is_empty() {
            return;
        }
        let cur = self.focus.and_then(|f| focusable.iter().position(|&x| x == f));
        self.focus = Some(focusable[cur.map_or(0, |c| (c + 1) % focusable.len())]);
    }

    fn move_selection(&mut self, key: Key) -> Event<A> {
        let Some(i) = self.list_index() else { return Event::None };
        let page = self.page(i) as i64;
        let Widget::List { rows, selected, .. } = &mut self.controls[i].1 else { return Event::None };
        if rows.is_empty() {
            return Event::None;
        }
        let step = match key {
            Key::Up => -1,
            Key::Down => 1,
            Key::PageUp => -page,
            _ => page,
        };
        let cur = selected.map_or(-1, |s| s as i64);
        *selected = Some((cur + step).clamp(0, rows.len() as i64 - 1) as usize);
        self.scroll_to_selection(i);
        Event::Selected
    }

    pub fn draw(&self, pen: &mut Pen) {
        pen.fill(0, 0, self.w, self.h, BG);
        for (i, (r, w)) in self.controls.iter().enumerate().filter(|&(i, _)| self.shown(i)) {
            let focused = self.focus == Some(i);
            match w {
                Widget::Label { text, dim } => {
                    let t = pen.fit(text, r.w);
                    pen.text(r.x, r.y + (r.h - FONT_PX as i32) / 2 - 1, &t, if *dim { DIM } else { TEXT }, r.x, r.w);
                }
                Widget::Input { text, cursor } => draw_input(pen, *r, text, *cursor, focused),
                Widget::List { rows, selected, scroll } => draw_list(pen, *r, rows, *selected, *scroll, focused),
                Widget::Button { label, default, .. } => draw_button(pen, *r, label, self.pressed == Some(i), *default),
                Widget::Check { label, checked, .. } => draw_check(pen, *r, label, *checked),
                Widget::Group { label } => draw_group(pen, *r, label),
                Widget::Image { image, note } => draw_image(pen, *r, image.as_ref(), note),
                Widget::Tabs { labels, selected, .. } => draw_tabs(pen, *r, labels, *selected),
                Widget::Pages { labels, selected, .. } => draw_pages(pen, *r, labels, *selected),
            }
        }
    }
}

/// `img` shrunk to fit `w`×`h`, each pixel the average of those it covers.
pub fn fit(img: &Bitmap, w: i32, h: i32) -> Bitmap {
    if img.w <= w && img.h <= h || img.w <= 0 || img.h <= 0 {
        return img.clone();
    }
    let k = (w as f64 / img.w as f64).min(h as f64 / img.h as f64);
    let (ow, oh) = (((img.w as f64 * k) as i32).max(1), ((img.h as f64 * k) as i32).max(1));
    let mut out = Bitmap::new(ow, oh, 0);
    for y in 0..oh {
        let (y0, y1) = (y * img.h / oh, ((y + 1) * img.h / oh).max(y * img.h / oh + 1));
        for x in 0..ow {
            let (x0, x1) = (x * img.w / ow, ((x + 1) * img.w / ow).max(x * img.w / ow + 1));
            let mut sum = [0u64; 4];
            for sy in y0..y1 {
                for sx in x0..x1 {
                    let p = img.px[(sy * img.w + sx) as usize];
                    for (c, s) in sum.iter_mut().enumerate() {
                        *s += ((p >> (c * 8)) & 0xff) as u64;
                    }
                }
            }
            let n = ((y1 - y0) * (x1 - x0)) as u64;
            out.px[(y * ow + x) as usize] = sum.iter().enumerate().map(|(c, s)| ((s / n) as u32) << (c * 8)).sum();
        }
    }
    out
}

/// Applies an editing key to a one-line text field. Returns whether the
/// text changed.
fn edit(text: &mut String, cursor: &mut usize, key: Key) -> bool {
    let mut chars: Vec<char> = text.chars().collect();
    *cursor = (*cursor).min(chars.len());
    match key {
        Key::Char(c) if !c.is_control() => {
            chars.insert(*cursor, c);
            *cursor += 1;
        }
        Key::Backspace if *cursor > 0 => {
            *cursor -= 1;
            chars.remove(*cursor);
        }
        Key::Delete if *cursor < chars.len() => {
            chars.remove(*cursor);
        }
        Key::Left => *cursor = cursor.saturating_sub(1),
        Key::Right => *cursor = (*cursor + 1).min(chars.len()),
        Key::Home => *cursor = 0,
        Key::End => *cursor = chars.len(),
        _ => {}
    }
    let new: String = chars.into_iter().collect();
    let changed = new != *text;
    *text = new;
    changed
}

fn sunken(pen: &mut Pen, r: Rect, focused: bool) {
    pen.fill(r.x, r.y, r.w, r.h, FIELD_BG);
    let edge = if focused { HILITE } else { BORDER };
    pen.bevel(r.x, r.y, r.w, r.h, edge, edge);
}

fn text_y(r: Rect) -> i32 {
    r.y + (r.h - FONT_PX as i32) / 2 - 1
}

fn draw_input(pen: &mut Pen, r: Rect, text: &str, cursor: usize, focused: bool) {
    sunken(pen, r, focused);
    let (clip_x, clip_w) = (r.x + 2, r.w - 4);
    // Scroll the text left when the caret would leave the field.
    let before: String = text.chars().take(cursor).collect();
    let caret = pen.width(&before);
    let shift = (caret - (clip_w - 8)).max(0);
    let x = r.x + 4 - shift;
    pen.text(x, text_y(r), text, TEXT, clip_x, clip_w);
    if focused {
        pen.vline(x + caret, r.y + 4, r.h - 8, TEXT);
    }
}

fn draw_list(pen: &mut Pen, r: Rect, rows: &[String], selected: Option<usize>, scroll: usize, focused: bool) {
    sunken(pen, r, focused);
    let page = ((r.h - 4) / ROW_H) as usize;
    for (n, row) in rows.iter().enumerate().skip(scroll).take(page) {
        let y = r.y + 2 + (n - scroll) as i32 * ROW_H;
        let sel = selected == Some(n);
        if sel {
            pen.fill(r.x + 2, y, r.w - 4, ROW_H, if focused { HILITE } else { mix(HILITE, FIELD_BG, 0.5) });
        }
        let t = pen.fit(row, r.w - 12);
        pen.text(r.x + 5, y + (ROW_H - FONT_PX as i32) / 2 - 1, &t, TEXT, r.x + 2, r.w - 4);
    }
    // A thin bar on the right shows where the visible rows are.
    if rows.len() > page && page > 0 {
        let track = r.h - 4;
        let bar_h = (track * page as i32 / rows.len() as i32).max(10);
        let bar_y = r.y + 2 + (track - bar_h) * scroll as i32 / (rows.len() - page) as i32;
        pen.fill(r.x + r.w - 5, bar_y, 3, bar_h, BORDER);
    }
}

fn draw_button(pen: &mut Pen, r: Rect, label: &str, down: bool, default: bool) {
    let face = if down { HILITE } else { mix(BG, BORDER, 0.35) };
    pen.fill(r.x, r.y, r.w, r.h, face);
    let edge = if default { mix(HILITE, TEXT, 0.3) } else { BORDER };
    pen.bevel(r.x, r.y, r.w, r.h, edge, edge);
    let tw = pen.width(label);
    pen.text(r.x + (r.w - tw) / 2, text_y(r), label, TEXT, r.x + 2, r.w - 4);
}

fn draw_tabs(pen: &mut Pen, r: Rect, labels: &[String], selected: usize) {
    let n = labels.len().max(1) as i32;
    for (k, label) in labels.iter().enumerate() {
        let (x0, x1) = (r.x + r.w * k as i32 / n, r.x + r.w * (k as i32 + 1) / n);
        let seg = Rect::new(x0, r.y, x1 - x0 + 1, r.h);
        let face = if k == selected { HILITE } else { mix(BG, BORDER, 0.35) };
        pen.fill(seg.x, seg.y, seg.w, seg.h, face);
        pen.bevel(seg.x, seg.y, seg.w, seg.h, BORDER, BORDER);
        let t = pen.fit(label, seg.w - 6);
        let tw = pen.width(&t);
        pen.text(seg.x + (seg.w - tw) / 2, text_y(seg), &t, TEXT, seg.x + 2, seg.w - 4);
    }
}

fn draw_pages(pen: &mut Pen, r: Rect, labels: &[String], selected: usize) {
    sunken(pen, r, false);
    for (k, label) in labels.iter().enumerate() {
        let row = Rect::new(r.x + 2, r.y + 2 + k as i32 * PAGE_H, r.w - 4, PAGE_H);
        if k == selected {
            pen.fill(row.x, row.y, row.w, row.h, HILITE);
        }
        let t = pen.fit(label, row.w - 12);
        pen.text(row.x + 6, text_y(row), &t, TEXT, row.x, row.w);
    }
}

fn draw_check(pen: &mut Pen, r: Rect, label: &str, checked: bool) {
    let (bx, by) = (r.x, r.y + (r.h - 13) / 2);
    sunken(pen, Rect::new(bx, by, 13, 13), false);
    if checked {
        for k in 0..3 {
            pen.vline(bx + 3 + k, by + 5 + k, 2, TEXT);
        }
        for k in 0..4 {
            pen.vline(bx + 6 + k, by + 6 - k, 2, TEXT);
        }
    }
    pen.text(r.x + 20, text_y(r), label, TEXT, r.x, r.w);
}

fn draw_group(pen: &mut Pen, r: Rect, label: &str) {
    let line = mix(BORDER, BG, 0.3);
    let top = r.y + FONT_PX as i32 / 2;
    pen.bevel(r.x, top, r.w, r.h - (top - r.y), line, line);
    let tw = pen.width(label);
    pen.fill(r.x + 8, r.y, tw + 6, FONT_PX as i32 + 2, BG);
    pen.text(r.x + 11, r.y - 1, label, DIM, r.x, r.w);
}

fn draw_image(pen: &mut Pen, r: Rect, image: Option<&Bitmap>, note: &str) {
    match image {
        Some(img) => pen.image(img, r),
        None => {
            sunken(pen, r, false);
            let t = pen.fit(note, r.w - 8);
            let tw = pen.width(&t);
            pen.text(r.x + (r.w - tw) / 2, r.y + r.h / 2 - FONT_PX as i32 / 2, &t, DIM, r.x, r.w);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    enum Act {
        Ok,
        Cancel,
        Flag,
    }

    fn rows(n: usize) -> Vec<String> {
        (0..n).map(|i| format!("row {i}")).collect()
    }

    /// A search field over a list of 20 rows (about 5 visible) and two buttons.
    fn form() -> (Form<Act>, usize, usize) {
        let mut f = Form::new(300, 200);
        let input = f.add(Rect::new(10, 10, 200, 22), Widget::Input { text: String::new(), cursor: 0 });
        let list = f.add(Rect::new(10, 40, 200, 4 + 5 * ROW_H), Widget::List { rows: rows(20), selected: Some(0), scroll: 0 });
        f.add(Rect::new(10, 150, 80, 24), Widget::Button { label: "OK".into(), action: Act::Ok, default: true });
        f.add(Rect::new(100, 150, 80, 24), Widget::Button { label: "Cancel".into(), action: Act::Cancel, default: false });
        (f, input, list)
    }

    fn type_text(f: &mut Form<Act>, s: &str) {
        for c in s.chars() {
            f.key(Key::Char(c));
        }
    }

    #[test]
    fn typing_edits_the_focused_field() {
        let (mut f, input, _) = form();
        type_text(&mut f, "abc");
        assert_eq!(f.key(Key::Left), Event::None);
        assert_eq!(f.key(Key::Backspace), Event::Edited);
        assert_eq!(f.text(input), "ac");
        f.key(Key::Home);
        assert_eq!(f.key(Key::Char('x')), Event::Edited);
        f.key(Key::End);
        assert_eq!(f.key(Key::Delete), Event::None);
        assert_eq!(f.text(input), "xac");
        assert_eq!(f.key(Key::Char('й')), Event::Edited);
        assert_eq!(f.text(input), "xacй");
    }

    #[test]
    fn arrows_move_the_list_while_typing() {
        let (mut f, _, list) = form();
        type_text(&mut f, "q");
        assert_eq!(f.key(Key::Down), Event::Selected);
        assert_eq!(f.selected(list), Some(1));
        f.key(Key::PageDown);
        assert_eq!(f.selected(list), Some(6));
        f.key(Key::Up);
        f.key(Key::Up);
        assert_eq!(f.selected(list), Some(4));
        for _ in 0..30 {
            f.key(Key::Down);
        }
        assert_eq!(f.selected(list), Some(19));
    }

    #[test]
    fn keeps_the_selection_in_view() {
        let (mut f, _, list) = form();
        f.key(Key::PageDown);
        f.key(Key::PageDown);
        // Row 10 selected, 5 rows visible: the first visible row is 6.
        assert_eq!(f.mouse_down(15, 40 + 2, false), Event::Selected);
        assert_eq!(f.selected(list), Some(6));
    }

    #[test]
    fn enter_clicks_the_default_button_and_escape_cancels() {
        let (mut f, _, _) = form();
        assert_eq!(f.key(Key::Enter), Event::Clicked(Act::Ok));
        assert_eq!(f.key(Key::Escape), Event::Cancel);
        let mut plain: Form<Act> = Form::new(100, 100);
        plain.add(Rect::new(0, 0, 100, 100), Widget::List { rows: rows(2), selected: Some(0), scroll: 0 });
        assert_eq!(plain.key(Key::Enter), Event::Activated);
    }

    #[test]
    fn buttons_fire_on_release_inside() {
        let (mut f, _, _) = form();
        assert_eq!(f.mouse_down(110, 160, false), Event::None);
        assert_eq!(f.mouse_up(110, 160), Event::Clicked(Act::Cancel));
        f.mouse_down(110, 160, false);
        assert_eq!(f.mouse_up(5, 5), Event::None, "released outside");
        assert_eq!(f.mouse_up(110, 160), Event::None, "not pressed");
    }

    #[test]
    fn clicking_rows_selects_and_double_click_activates() {
        let (mut f, _, list) = form();
        let row2 = 40 + 2 + 2 * ROW_H + 1;
        assert_eq!(f.mouse_down(20, row2, false), Event::Selected);
        assert_eq!(f.selected(list), Some(2));
        assert_eq!(f.mouse_down(20, row2, true), Event::Activated);
        f.set_rows(list, rows(1), Some(0));
        assert_eq!(f.mouse_down(20, row2, false), Event::None, "below the last row");
    }

    #[test]
    fn check_boxes_toggle() {
        let mut f: Form<Act> = Form::new(100, 40);
        let c = f.add(Rect::new(0, 0, 100, 20), Widget::Check { label: "Flag".into(), checked: false, action: Act::Flag });
        assert_eq!(f.mouse_down(5, 5, false), Event::Toggled(Act::Flag, true));
        assert_eq!(f.mouse_down(5, 5, false), Event::Toggled(Act::Flag, false));
        assert!(matches!(f.widget(c), Widget::Check { checked: false, .. }));
    }

    #[test]
    fn tab_moves_focus_between_field_and_list() {
        let (mut f, input, _) = form();
        f.key(Key::Tab);
        assert_eq!(f.key(Key::Char('a')), Event::None, "the list has the keyboard");
        f.key(Key::Tab);
        f.key(Key::Char('a'));
        assert_eq!(f.text(input), "a");
    }

    #[test]
    fn wheel_scrolls_within_bounds() {
        let (mut f, _, list) = form();
        f.wheel(-100);
        // 20 rows, 5 visible: the last page starts at row 15.
        assert_eq!(f.mouse_down(15, 40 + 2, false), Event::Selected);
        assert_eq!(f.selected(list), Some(15));
        f.wheel(100);
        f.mouse_down(15, 40 + 2, false);
        assert_eq!(f.selected(list), Some(0));
    }

    #[test]
    fn tabs_pick_the_segment_under_the_mouse() {
        let mut f: Form<Act> = Form::new(300, 40);
        let t = f.add(
            Rect::new(0, 0, 300, 24),
            Widget::Tabs { labels: vec!["A".into(), "B".into(), "C".into()], selected: 0, action: Act::Flag },
        );
        assert_eq!(f.mouse_down(250, 10, false), Event::Picked(Act::Flag, 2));
        assert_eq!(f.tab(t), 2);
        assert_eq!(f.mouse_down(250, 10, false), Event::None, "already picked");
        assert_eq!(f.mouse_down(99, 10, false), Event::Picked(Act::Flag, 0));
        assert_eq!(f.mouse_down(100, 10, false), Event::Picked(Act::Flag, 1));
    }

    #[test]
    fn new_rows_clamp_the_selection() {
        let (mut f, _, list) = form();
        f.set_rows(list, rows(3), Some(10));
        assert_eq!(f.selected(list), Some(2));
        f.set_rows(list, vec![], Some(0));
        assert_eq!(f.selected(list), None);
        assert_eq!(f.key(Key::Down), Event::None);
    }

    #[test]
    fn pictures_shrink_to_fit_averaging_pixels() {
        let mut img = Bitmap::new(4, 2, 0);
        img.px = vec![0xff00_0000, 0xff00_00ff, 0xffff_ffff, 0xffff_ffff, 0xff00_0000, 0xff00_00ff, 0xffff_ffff, 0xffff_ffff];
        let small = fit(&img, 2, 2);
        assert_eq!((small.w, small.h), (2, 1));
        assert_eq!(small.px, [0xff00_007f, 0xffff_ffff]);
        assert_eq!(fit(&img, 10, 10).w, 4, "small ones stay as they are");
    }

    #[test]
    fn pages_show_their_own_controls() {
        let mut f = Form::new(300, 200);
        let labels = vec!["One".to_string(), "Two".to_string()];
        f.add(Rect::new(0, 0, 80, 4 + 2 * PAGE_H), Widget::Pages { labels, selected: 0, action: Act::Cancel });
        let field = f.add_on(0, Rect::new(100, 10, 100, 22), Widget::Input { text: String::new(), cursor: 0 });
        f.add_on(1, Rect::new(100, 10, 100, 22), Widget::Check { label: "x".into(), checked: false, action: Act::Flag });
        f.add_on(1, Rect::new(100, 40, 100, 4 + 3 * ROW_H), Widget::List { rows: rows(3), selected: None, scroll: 0 });
        f.add(Rect::new(100, 150, 80, 24), Widget::Button { label: "OK".into(), action: Act::Ok, default: true });
        assert_eq!(f.mouse_down(105, 15, false), Event::None, "the field, not the check box under it");
        type_text(&mut f, "a");
        assert_eq!(f.mouse_down(10, 2 + PAGE_H + 5, false), Event::Picked(Act::Cancel, 1));
        assert_eq!(f.shown_page(), 1);
        assert_eq!(f.mouse_down(10, 2 + PAGE_H + 5, false), Event::None, "picked already");
        assert_eq!(f.mouse_down(105, 15, false), Event::Toggled(Act::Flag, true));
        type_text(&mut f, "b");
        assert_eq!(f.text(field), "a", "the hidden field lost the keyboard");
        assert_eq!(f.key(Key::Down), Event::Selected, "the arrows move the list of the page shown");
        assert_eq!(f.key(Key::Enter), Event::Clicked(Act::Ok), "controls on every page stay");
    }
}
