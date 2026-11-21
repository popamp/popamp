//! A tiny set of controls for Popamp's own dialogs: labels, text fields,
//! lists, buttons, check boxes, group frames and a tree of pages. A
//! [`Form`] holds them, scrolls a page taller than its view,
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
    /// The pages of the form, one under another, some under others as in
    /// a tree; picking one shows the controls on it.
    Pages {
        entries: Vec<PageEntry>,
        selected: usize,
        action: A,
    },
    /// A drop-down list: shows the option picked; a click opens the
    /// options below it. See [`Form::choice`].
    Choice {
        options: Vec<String>,
        selected: usize,
    },
    /// A picture, or `note` in an empty frame until it arrives.
    Image {
        image: Option<Bitmap>,
        note: String,
    },
}

/// One page listed in a [`Widget::Pages`].
#[derive(Debug, Clone)]
pub struct PageEntry {
    pub label: String,
    /// The page it is listed under, indented, while that one is open.
    pub parent: Option<usize>,
    /// Whether the pages under it are listed.
    pub open: bool,
}

impl PageEntry {
    pub fn top(label: impl Into<String>) -> PageEntry {
        PageEntry { label: label.into(), parent: None, open: true }
    }

    #[cfg_attr(not(feature = "plugins"), allow(dead_code))]
    pub fn under(parent: usize, label: impl Into<String>) -> PageEntry {
        PageEntry { label: label.into(), parent: Some(parent), open: false }
    }
}

/// The pages listed, top to bottom.
fn listed(entries: &[PageEntry]) -> Vec<usize> {
    (0..entries.len()).filter(|&k| entries[k].parent.is_none_or(|p| entries[p].open)).collect()
}

/// Whether page `k` has pages listed under it, and so an arrow to close it.
fn has_children(entries: &[PageEntry], k: usize) -> bool {
    entries.iter().any(|e| e.parent == Some(k))
}

/// The width at the right end of a page row where the arrow opens and closes it.
const ARROW_W: i32 = 18;
/// How far a wheel notch scrolls a page.
const WHEEL_STEP: i32 = 40;

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
    /// The drop-down whose options are showing.
    open: Option<usize>,
    /// Where the controls on pages are shown: they scroll inside it.
    /// `None`: nothing scrolls.
    view: Option<Rect>,
    /// How far each page is scrolled, by page.
    scrolled: Vec<i32>,
    /// Controls put away for now, as the details of a plug-in.
    hidden: Vec<bool>,
    /// Controls shown greyed out, taking no clicks or keys.
    disabled: Vec<bool>,
    /// The scroll bar's thumb is being dragged: the mouse's distance from its top.
    grip: Option<i32>,
}

impl<A: Copy + PartialEq> Form<A> {
    pub fn new(w: i32, h: i32) -> Form<A> {
        Form {
            w,
            h,
            controls: Vec::new(),
            pages: Vec::new(),
            page: 0,
            focus: None,
            pressed: None,
            open: None,
            view: None,
            scrolled: Vec::new(),
            hidden: Vec::new(),
            disabled: Vec::new(),
            grip: None,
        }
    }

    /// Shows the controls on pages inside `view`, scrolled when a page is taller.
    pub fn set_view(&mut self, view: Rect) {
        self.view = Some(view);
    }

    /// Makes the form no taller than `max_h`, for a small screen: the view
    /// gets shorter, and what is under it moves up with the form's bottom.
    pub fn limit_height(&mut self, max_h: i32) {
        let Some(view) = self.view.as_mut() else { return };
        let cut = (self.h - max_h).min(view.h - 2 * WHEEL_STEP);
        if cut <= 0 {
            return;
        }
        let bottom = view.y + view.h;
        for ((r, _), _) in self.controls.iter_mut().zip(&self.pages).filter(|(_, page)| page.is_none()) {
            if r.y >= bottom {
                r.y -= cut;
            } else if r.y + r.h > bottom {
                // Runs past the view, as the list of pages does.
                r.h -= cut;
            }
        }
        view.h -= cut;
        self.h -= cut;
        self.set_scroll(self.scroll());
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
        self.hidden.push(false);
        self.disabled.push(false);
        if self.focus.is_none() && self.focusable(i) {
            self.focus = Some(i);
        }
        i
    }

    /// Whether control `i` is on the page shown.
    fn shown(&self, i: usize) -> bool {
        !self.hidden[i] && self.pages[i].is_none_or(|p| p == self.page)
    }

    /// Puts control `i` away, or back.
    #[cfg_attr(not(feature = "plugins"), allow(dead_code))]
    pub fn set_hidden(&mut self, i: usize, hidden: bool) {
        self.hidden[i] = hidden;
        if hidden && self.focus == Some(i) {
            self.focus = (0..self.controls.len()).find(|&i| self.focusable(i));
        }
        self.set_scroll(self.scroll());
    }

    /// Greys control `i` out, or brings it back.
    #[cfg_attr(not(feature = "plugins"), allow(dead_code))]
    pub fn set_disabled(&mut self, i: usize, disabled: bool) {
        self.disabled[i] = disabled;
        if disabled && self.focus == Some(i) {
            self.focus = (0..self.controls.len()).find(|&i| self.focusable(i));
        }
    }

    /// Whether control `i` moves with its page.
    fn scrolls(&self, i: usize) -> bool {
        self.view.is_some() && self.pages[i].is_some()
    }

    /// How far the page shown is scrolled.
    fn scroll(&self) -> i32 {
        self.scrolled.get(self.page).copied().unwrap_or(0)
    }

    /// Where control `i` is in the window, its page scrolled.
    pub fn screen_rect(&self, i: usize) -> Rect {
        let r = self.controls[i].0;
        if self.scrolls(i) {
            Rect { y: r.y - self.scroll(), ..r }
        } else {
            r
        }
    }

    /// Whether the mouse at `x`, `y` is on control `i`; one scrolled out of
    /// the view is not.
    fn under(&self, i: usize, x: i32, y: i32) -> bool {
        self.shown(i) && self.screen_rect(i).contains(x, y) && (!self.scrolls(i) || self.view.is_some_and(|v| v.contains(x, y)))
    }

    /// How far the page shown scrolls: until its lowest control and a
    /// margin are in view.
    fn max_scroll(&self) -> i32 {
        let Some(v) = self.view else { return 0 };
        let lowest = (0..self.controls.len()).filter(|&i| self.shown(i) && self.scrolls(i)).map(|i| {
            let r = self.controls[i].0;
            r.y + r.h
        });
        lowest.max().map_or(0, |b| (b + 8 - (v.y + v.h)).max(0))
    }

    fn set_scroll(&mut self, s: i32) {
        let s = s.clamp(0, self.max_scroll());
        if self.scrolled.len() <= self.page {
            self.scrolled.resize(self.page + 1, 0);
        }
        self.scrolled[self.page] = s;
    }

    /// Scrolls the page so control `i` is in view.
    fn scroll_into_view(&mut self, i: usize) {
        let Some(v) = self.view.filter(|_| self.scrolls(i)) else { return };
        let (r, s) = (self.controls[i].0, self.scroll());
        if r.y - s < v.y + 4 {
            self.set_scroll(r.y - v.y - 4);
        } else if r.y + r.h - s > v.y + v.h - 4 {
            self.set_scroll(r.y + r.h - (v.y + v.h) + 4);
        }
    }

    /// The scroll bar along the right of the view and its thumb, while
    /// the page is taller than the view.
    fn scroll_bar(&self) -> Option<(Rect, Rect)> {
        let v = self.view?;
        let max = self.max_scroll();
        if max == 0 {
            return None;
        }
        let track = Rect::new(v.x + v.w - 8, v.y + 2, 6, v.h - 4);
        let thumb_h = (track.h * v.h / (v.h + max)).max(16);
        let thumb_y = track.y + (track.h - thumb_h) * self.scroll() / max;
        Some((track, Rect::new(track.x, thumb_y, track.w, thumb_h)))
    }

    /// A press on the scroll bar: the thumb is taken, or the page moves by
    /// a view towards the press.
    fn press_bar(&mut self, track: Rect, thumb: Rect, y: i32) {
        if thumb.contains(thumb.x, y) {
            self.grip = Some(y - thumb.y);
        } else {
            let step = track.h - WHEEL_STEP;
            self.set_scroll(self.scroll() + if y < thumb.y { -step } else { step });
        }
    }

    /// The mouse moved to height `y`. Returns whether the page scrolled:
    /// it does while the thumb of the scroll bar is held.
    pub fn mouse_move(&mut self, y: i32) -> bool {
        let (Some(grip), Some((track, thumb))) = (self.grip, self.scroll_bar()) else { return false };
        let room = (track.h - thumb.h).max(1);
        self.set_scroll((y - grip - track.y) * self.max_scroll() / room);
        true
    }

    /// A text field or list on the page shown.
    fn focusable(&self, i: usize) -> bool {
        self.shown(i) && !self.disabled[i] && matches!(self.controls[i].1, Widget::Input { .. } | Widget::List { .. })
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
        self.open = None;
        if !self.focus.is_some_and(|f| self.shown(f)) {
            self.focus = (0..self.controls.len()).find(|&i| self.focusable(i));
        }
    }

    /// Renames page `page` of pages `i`.
    #[cfg_attr(not(feature = "plugins"), allow(dead_code))]
    pub fn set_page_label(&mut self, i: usize, page: usize, label: String) {
        if let Widget::Pages { entries, .. } = &mut self.controls[i].1 {
            entries[page].label = label;
        }
    }

    /// Picks page `page` of pages `i`, opening the one it is under.
    #[cfg_attr(not(feature = "plugins"), allow(dead_code))]
    pub fn select_page(&mut self, i: usize, page: usize) {
        let Widget::Pages { entries, selected, .. } = &mut self.controls[i].1 else { return };
        if let Some(p) = entries[page].parent {
            entries[p].open = true;
        }
        *selected = page;
        self.show_page(page);
    }

    /// Where the row of page `page` is in pages `i`, left of its arrow;
    /// `None` while the page it is under is closed.
    #[cfg(test)]
    pub fn page_row(&self, i: usize, page: usize) -> Option<Rect> {
        let (r, Widget::Pages { entries, .. }) = &self.controls[i] else { return None };
        let n = listed(entries).iter().position(|&k| k == page)?;
        Some(Rect::new(r.x + 2, r.y + 2 + n as i32 * PAGE_H, r.w - 4 - ARROW_W, PAGE_H))
    }

    /// Whether control `i` is on the page shown and not put away.
    #[cfg(all(test, feature = "plugins"))]
    pub fn visible(&self, i: usize) -> bool {
        self.shown(i)
    }

    #[cfg(test)]
    pub fn widget(&self, i: usize) -> &Widget<A> {
        &self.controls[i].1
    }

    /// How many controls there are: the index the next one gets.
    #[cfg_attr(not(feature = "plugins"), allow(dead_code))]
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

    /// Moves or resizes control `i`, for a frame sized after what it holds.
    pub fn set_rect(&mut self, i: usize, r: Rect) {
        self.controls[i].0 = r;
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

    /// The option picked in drop-down `i`.
    pub fn choice(&self, i: usize) -> usize {
        match &self.controls[i].1 {
            Widget::Choice { selected, .. } => *selected,
            _ => 0,
        }
    }

    /// Picks option `n` of drop-down `i` (the last, when `n` is past it).
    pub fn set_choice(&mut self, i: usize, n: usize) {
        if let Widget::Choice { options, selected } = &mut self.controls[i].1 {
            *selected = n.min(options.len().saturating_sub(1));
        }
    }

    /// Where the options of the open drop-down `i` are drawn: under it,
    /// or over it when they would run past the form's bottom.
    fn dropdown(&self, i: usize) -> Rect {
        let r = self.screen_rect(i);
        let n = match &self.controls[i].1 {
            Widget::Choice { options, .. } => options.len() as i32,
            _ => 0,
        };
        let h = n * ROW_H + 4;
        let y = if r.y + r.h + h > self.h && r.y - h >= 0 { r.y - h } else { r.y + r.h };
        Rect::new(r.x, y, r.w, h)
    }

    /// A press while a drop-down is open: it picks the option under the
    /// mouse, or just closes the list; either way the press is spent.
    /// A key while drop-down `i` is open, which has the keys: the arrows
    /// pick, Enter and Escape close it.
    fn key_in_open(&mut self, i: usize, key: Key) -> Event<A> {
        let step = match key {
            Key::Up => -1,
            Key::Down => 1,
            Key::Enter | Key::Escape => {
                self.open = None;
                return Event::None;
            }
            _ => return Event::None,
        };
        let n = (self.choice(i) as i64 + step).max(0) as usize;
        self.set_choice(i, n);
        Event::Selected
    }

    fn press_in_open(&mut self, i: usize, x: i32, y: i32) -> Event<A> {
        self.open = None;
        let list = self.dropdown(i);
        if !list.contains(x, y) {
            return Event::None;
        }
        let row = ((y - list.y - 2) / ROW_H).max(0) as usize;
        self.set_choice(i, row);
        Event::Selected
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
            Widget::Button { action, default: true, .. } if self.shown(i) && !self.disabled[i] => Some(*action),
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
        if let Some(open) = self.open {
            return self.press_in_open(open, x, y);
        }
        if let Some((track, thumb)) = self.scroll_bar().filter(|(track, _)| track.contains(x, y)) {
            self.press_bar(track, thumb, y);
            return Event::None;
        }
        let Some(i) = (0..self.controls.len()).find(|&i| {
            self.under(i, x, y)
                && !self.disabled[i]
                && !matches!(self.controls[i].1, Widget::Label { .. } | Widget::Group { .. } | Widget::Image { .. })
        }) else {
            return Event::None;
        };
        let rect = self.screen_rect(i);
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
            Widget::Pages { .. } => self.pick_page(i, x, y),
            Widget::Choice { .. } => {
                self.open = Some(i);
                Event::None
            }
            Widget::Label { .. } | Widget::Group { .. } | Widget::Image { .. } => Event::None,
        }
    }

    /// A press on pages `i` shows the page there; on the arrow of a page
    /// with pages under it, it opens or closes that page.
    fn pick_page(&mut self, i: usize, x: i32, y: i32) -> Event<A> {
        let (rect, widget) = &mut self.controls[i];
        let Widget::Pages { entries, selected, action } = widget else { return Event::None };
        let Some(&k) = listed(entries).get(((y - rect.y - 2) / PAGE_H).max(0) as usize) else { return Event::None };
        if has_children(entries, k) && x >= rect.x + rect.w - ARROW_W {
            entries[k].open = !entries[k].open;
            // Closing a page over the one shown shows the closed one.
            if entries[k].open || entries[*selected].parent != Some(k) {
                return Event::None;
            }
        } else {
            entries[k].open = true;
        }
        if k == *selected {
            return Event::None;
        }
        *selected = k;
        let action = *action;
        self.show_page(k);
        Event::Picked(action, k)
    }

    /// A button fires when the mouse is released over it.
    pub fn mouse_up(&mut self, x: i32, y: i32) -> Event<A> {
        self.grip = None;
        match self.pressed.take().map(|i| &self.controls[i]) {
            Some((r, Widget::Button { action, .. })) if r.contains(x, y) => Event::Clicked(*action),
            _ => Event::None,
        }
    }

    /// Scrolls by `steps` wheel notches (positive = up) what is under the
    /// mouse at `x`, `y`: a list, or else the page. Elsewhere the list
    /// with the keyboard scrolls.
    pub fn wheel(&mut self, x: i32, y: i32, steps: i32) {
        let list = (0..self.controls.len()).find(|&i| self.under(i, x, y) && matches!(self.controls[i].1, Widget::List { .. }));
        if list.is_none() && self.view.is_some_and(|v| v.contains(x, y)) {
            self.set_scroll(self.scroll() - steps * WHEEL_STEP);
            return;
        }
        let Some(i) = list.or_else(|| self.list_index()) else { return };
        let page = self.page(i);
        if let Widget::List { rows, scroll, .. } = &mut self.controls[i].1 {
            let max = rows.len().saturating_sub(page) as i32;
            *scroll = (*scroll as i32 - steps * 3).clamp(0, max) as usize;
        }
    }

    pub fn key(&mut self, key: Key) -> Event<A> {
        if let Some(i) = self.open {
            return self.key_in_open(i, key);
        }
        match key {
            Key::Escape => return Event::Cancel,
            Key::Enter => return self.default_action().map_or(Event::Activated, Event::Clicked),
            Key::Tab => {
                self.focus_next();
                if let Some(f) = self.focus {
                    self.scroll_into_view(f);
                }
                return Event::None;
            }
            Key::PageUp | Key::PageDown if self.list_index().is_none() => {
                let step = self.view.map_or(0, |v| v.h - WHEEL_STEP);
                self.set_scroll(self.scroll() + if key == Key::PageUp { -step } else { step });
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
        // The page first, scrolled; then what scrolled out of the view is
        // covered, and the controls on every page go on top.
        pen.set_offset(-self.scroll());
        for i in (0..self.controls.len()).filter(|&i| self.shown(i) && self.scrolls(i)) {
            self.draw_control(pen, i);
        }
        pen.set_offset(0);
        if let Some(v) = self.view {
            pen.fill(v.x, 0, v.w, v.y, BG);
            pen.fill(v.x, v.y + v.h, v.w, self.h - v.y - v.h, BG);
        }
        for i in (0..self.controls.len()).filter(|&i| self.shown(i) && !self.scrolls(i)) {
            self.draw_control(pen, i);
        }
        if let Some((track, thumb)) = self.scroll_bar() {
            pen.fill(track.x, track.y, track.w, track.h, FIELD_BG);
            pen.fill(thumb.x + 1, thumb.y, thumb.w - 2, thumb.h, DIM);
        }
        // The options of an open drop-down go over what is below it.
        if let Some(i) = self.open.filter(|&i| self.shown(i)) {
            if let Widget::Choice { options, selected } = &self.controls[i].1 {
                draw_options(pen, self.dropdown(i), options, *selected);
            }
        }
    }

    fn draw_control(&self, pen: &mut Pen, i: usize) {
        let (r, w) = &self.controls[i];
        {
            let focused = self.focus == Some(i);
            let ink = if self.disabled[i] { DIM } else { TEXT };
            match w {
                Widget::Label { text, dim } => {
                    let t = pen.fit(text, r.w);
                    pen.text(r.x, r.y + (r.h - FONT_PX as i32) / 2 - 1, &t, if *dim { DIM } else { ink }, r.x, r.w);
                }
                Widget::Input { text, cursor } => draw_input(pen, *r, text, *cursor, focused, ink),
                Widget::List { rows, selected, scroll } => draw_list(pen, *r, rows, *selected, *scroll, focused),
                Widget::Button { label, default, .. } => draw_button(pen, *r, label, self.pressed == Some(i), *default, ink),
                Widget::Check { label, checked, .. } => draw_check(pen, *r, label, *checked, ink),
                Widget::Group { label } => draw_group(pen, *r, label),
                Widget::Image { image, note } => draw_image(pen, *r, image.as_ref(), note),
                Widget::Tabs { labels, selected, .. } => draw_tabs(pen, *r, labels, *selected),
                Widget::Pages { entries, selected, .. } => draw_pages(pen, *r, entries, *selected),
                Widget::Choice { options, selected } => {
                    draw_choice(pen, *r, options.get(*selected).map_or("", |s| s), self.open == Some(i))
                }
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

fn draw_input(pen: &mut Pen, r: Rect, text: &str, cursor: usize, focused: bool, ink: u32) {
    sunken(pen, r, focused);
    let (clip_x, clip_w) = (r.x + 2, r.w - 4);
    // Scroll the text left when the caret would leave the field.
    let before: String = text.chars().take(cursor).collect();
    let caret = pen.width(&before);
    let shift = (caret - (clip_w - 8)).max(0);
    let x = r.x + 4 - shift;
    pen.text(x, text_y(r), text, ink, clip_x, clip_w);
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
        // A row with tabs is a row of columns of equal width.
        let cols = row.split('\t').count() as i32;
        let col = (r.w - 4) / cols;
        for (n, cell) in row.split('\t').enumerate() {
            let x = r.x + 2 + n as i32 * col;
            let t = pen.fit(cell, col - 8);
            pen.text(x + 3, y + (ROW_H - FONT_PX as i32) / 2 - 1, &t, TEXT, x, col - 2);
        }
    }
    // A thin bar on the right shows where the visible rows are.
    if rows.len() > page && page > 0 {
        let track = r.h - 4;
        let bar_h = (track * page as i32 / rows.len() as i32).max(10);
        let bar_y = r.y + 2 + (track - bar_h) * scroll as i32 / (rows.len() - page) as i32;
        pen.fill(r.x + r.w - 5, bar_y, 3, bar_h, BORDER);
    }
}

fn draw_choice(pen: &mut Pen, r: Rect, text: &str, open: bool) {
    sunken(pen, r, open);
    let t = pen.fit(text, r.w - 24);
    pen.text(r.x + 5, text_y(r), &t, TEXT, r.x + 2, r.w - 18);
    arrow_down(pen, r.x + r.w - 13, r.y + r.h / 2 - 1);
}

/// A small arrow pointing down, its top left at `x`, `y`.
fn arrow_down(pen: &mut Pen, x: i32, y: i32) {
    for k in 0..4 {
        pen.set(x + k, y + k, TEXT);
        pen.set(x + 7 - k, y + k, TEXT);
    }
}

/// A small arrow pointing right, its top left at `x`, `y`.
fn arrow_right(pen: &mut Pen, x: i32, y: i32) {
    for k in 0..4 {
        pen.set(x + k, y + k, TEXT);
        pen.set(x + k, y + 7 - k, TEXT);
    }
}

fn draw_options(pen: &mut Pen, r: Rect, options: &[String], selected: usize) {
    sunken(pen, r, true);
    for (n, option) in options.iter().enumerate() {
        let y = r.y + 2 + n as i32 * ROW_H;
        if n == selected {
            pen.fill(r.x + 2, y, r.w - 4, ROW_H, HILITE);
        }
        let t = pen.fit(option, r.w - 12);
        pen.text(r.x + 5, y + (ROW_H - FONT_PX as i32) / 2 - 1, &t, TEXT, r.x + 2, r.w - 4);
    }
}

fn draw_button(pen: &mut Pen, r: Rect, label: &str, down: bool, default: bool, ink: u32) {
    let face = if down { HILITE } else { mix(BG, BORDER, 0.35) };
    pen.fill(r.x, r.y, r.w, r.h, face);
    let edge = if default { mix(HILITE, TEXT, 0.3) } else { BORDER };
    pen.bevel(r.x, r.y, r.w, r.h, edge, edge);
    let tw = pen.width(label);
    pen.text(r.x + (r.w - tw) / 2, text_y(r), label, ink, r.x + 2, r.w - 4);
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

fn draw_pages(pen: &mut Pen, r: Rect, entries: &[PageEntry], selected: usize) {
    sunken(pen, r, false);
    for (n, k) in listed(entries).into_iter().enumerate() {
        let row = Rect::new(r.x + 2, r.y + 2 + n as i32 * PAGE_H, r.w - 4, PAGE_H);
        if row.y + row.h > r.y + r.h - 2 {
            break;
        }
        if k == selected {
            pen.fill(row.x, row.y, row.w, row.h, HILITE);
        }
        let indent = if entries[k].parent.is_some() { 14 } else { 0 };
        let arrow = has_children(entries, k);
        let t = pen.fit(&entries[k].label, row.w - 12 - indent - if arrow { ARROW_W } else { 0 });
        pen.text(row.x + 6 + indent, text_y(row), &t, TEXT, row.x, row.w);
        if arrow {
            let (ax, ay) = (row.x + row.w - 13, row.y + PAGE_H / 2);
            if entries[k].open {
                arrow_down(pen, ax, ay - 2);
            } else {
                arrow_right(pen, ax + 2, ay - 4);
            }
        }
    }
}

fn draw_check(pen: &mut Pen, r: Rect, label: &str, checked: bool, ink: u32) {
    let (bx, by) = (r.x, r.y + (r.h - 13) / 2);
    sunken(pen, Rect::new(bx, by, 13, 13), false);
    if checked {
        for k in 0..3 {
            pen.vline(bx + 3 + k, by + 5 + k, 2, ink);
        }
        for k in 0..4 {
            pen.vline(bx + 6 + k, by + 6 - k, 2, ink);
        }
    }
    pen.text(r.x + 20, text_y(r), label, ink, r.x, r.w);
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
    fn a_drop_down_opens_and_picks_an_option() {
        let mut f: Form<Act> = Form::new(300, 200);
        let c = f.add(Rect::new(10, 10, 120, 24), Widget::Choice { options: rows(4), selected: 0 });
        assert_eq!(f.mouse_down(20, 20, false), Event::None);
        // The options are under the box; the third is at 10 + 24 + 2 + 2 * ROW_H.
        assert_eq!(f.mouse_down(20, 10 + 24 + 2 + 2 * ROW_H + 3, false), Event::Selected);
        assert_eq!(f.choice(c), 2);
        // Closed again: a press where the options were no longer picks.
        assert_eq!(f.mouse_down(20, 10 + 24 + 2 + 3, false), Event::None);
        assert_eq!(f.choice(c), 2);
    }

    #[test]
    fn a_drop_down_near_the_bottom_opens_upwards() {
        let mut f: Form<Act> = Form::new(300, 200);
        let c = f.add(Rect::new(10, 150, 120, 24), Widget::Choice { options: rows(4), selected: 0 });
        f.mouse_down(20, 160, false);
        // Four options over the box: the first at 150 - (4 * ROW_H + 4) + 2.
        assert_eq!(f.mouse_down(20, 150 - 4 * ROW_H - 4 + 2 + 3, false), Event::Selected);
        assert_eq!(f.choice(c), 0);
        f.mouse_down(20, 160, false);
        assert_eq!(f.mouse_down(20, 150 - ROW_H - 4 + 2 + 3, false), Event::Selected);
        assert_eq!(f.choice(c), 3, "the last option is just over the box");
    }

    #[test]
    fn a_press_outside_closes_a_drop_down_without_acting() {
        let mut f: Form<Act> = Form::new(300, 200);
        let c = f.add(Rect::new(10, 10, 120, 24), Widget::Choice { options: rows(3), selected: 1 });
        f.add(Rect::new(10, 150, 80, 24), Widget::Check { label: "x".into(), checked: false, action: Act::Flag });
        f.mouse_down(20, 20, false);
        assert_eq!(f.mouse_down(20, 155, false), Event::None, "the check box is not toggled by the press that closes");
        assert_eq!(f.choice(c), 1);
        assert_eq!(f.mouse_down(20, 155, false), Event::Toggled(Act::Flag, true));
    }

    #[test]
    fn the_arrows_pick_in_an_open_drop_down() {
        let mut f: Form<Act> = Form::new(300, 200);
        let c = f.add(Rect::new(10, 10, 120, 24), Widget::Choice { options: rows(3), selected: 0 });
        f.mouse_down(20, 20, false);
        assert_eq!(f.key(Key::Down), Event::Selected);
        assert_eq!(f.key(Key::Down), Event::Selected);
        assert_eq!(f.key(Key::Down), Event::Selected);
        assert_eq!(f.choice(c), 2, "stops at the last");
        assert_eq!(f.key(Key::Escape), Event::None, "closes the list, not the form");
        assert_eq!(f.key(Key::Escape), Event::Cancel);
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
        f.wheel(0, 0, -100);
        // 20 rows, 5 visible: the last page starts at row 15.
        assert_eq!(f.mouse_down(15, 40 + 2, false), Event::Selected);
        assert_eq!(f.selected(list), Some(15));
        f.wheel(0, 0, 100);
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
        let entries = vec![PageEntry::top("One"), PageEntry::top("Two")];
        f.add(Rect::new(0, 0, 80, 4 + 2 * PAGE_H), Widget::Pages { entries, selected: 0, action: Act::Cancel });
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

    /// A page row of the pages at the top left, `row` places down.
    fn page_row(row: i32) -> (i32, i32) {
        (10, 2 + row * PAGE_H + 5)
    }

    /// Pages One and Plug-ins, with Crossfeed under Plug-ins.
    fn tree() -> (Form<Act>, usize) {
        let mut f = Form::new(300, 200);
        let entries = vec![PageEntry::top("One"), PageEntry::top("Plug-ins"), PageEntry::under(1, "Crossfeed")];
        let pages = f.add(Rect::new(0, 0, 100, 4 + 4 * PAGE_H), Widget::Pages { entries, selected: 0, action: Act::Cancel });
        (f, pages)
    }

    #[test]
    fn a_page_under_another_is_listed_while_its_parent_is_open() {
        let (mut f, _) = tree();
        let (x, y) = page_row(2);
        assert_eq!(f.mouse_down(x, y, false), Event::Picked(Act::Cancel, 2));
        // The arrow at the right end of Plug-ins closes it: Crossfeed goes, and the page shown with it.
        let (_, y1) = page_row(1);
        assert_eq!(f.mouse_down(95, y1, false), Event::Picked(Act::Cancel, 1));
        assert_eq!(f.shown_page(), 1);
        assert_eq!(f.mouse_down(x, y, false), Event::None, "closed");
        assert_eq!(f.mouse_down(95, y1, false), Event::None, "opened again, Plug-ins stays picked");
        assert_eq!(f.mouse_down(x, y, false), Event::Picked(Act::Cancel, 2));
        f.mouse_down(95, y1, false);
        assert_eq!(f.mouse_down(x, y1, false), Event::None, "picked already");
        assert_eq!(f.mouse_down(x, y, false), Event::Picked(Act::Cancel, 2), "picking a closed page opens it");
    }

    #[test]
    fn a_page_can_be_picked_from_the_code() {
        let (mut f, pages) = tree();
        f.select_page(pages, 2);
        assert_eq!(f.shown_page(), 2);
        let (x, y) = page_row(2);
        assert_eq!(f.mouse_down(x, y, false), Event::None, "picked already");
    }

    /// A page 200 wide at the right whose view is 150 high, a check box at
    /// its top and a field far below; an OK button under the view.
    fn tall() -> (Form<Act>, usize, usize, usize) {
        let mut f = Form::new(300, 200);
        f.set_view(Rect::new(100, 0, 200, 150));
        let check = f.add_on(0, Rect::new(110, 10, 150, 20), Widget::Check { label: "x".into(), checked: false, action: Act::Flag });
        let field = f.add_on(0, Rect::new(110, 300, 150, 22), Widget::Input { text: String::new(), cursor: 0 });
        let ok = f.add(Rect::new(200, 160, 80, 24), Widget::Button { label: "OK".into(), action: Act::Ok, default: true });
        (f, check, field, ok)
    }

    #[test]
    fn the_wheel_scrolls_a_page_taller_than_its_view_and_no_further() {
        let (mut f, check, field, _) = tall();
        assert_eq!(f.mouse_down(115, 305, false), Event::None, "below the view the field is out of reach");
        f.wheel(150, 50, -100);
        // The field's bottom (322) and a margin of 8 reach the view's bottom (150).
        assert_eq!(f.screen_rect(field).y, 300 - 180);
        assert_eq!(f.screen_rect(check).y, 10 - 180, "scrolled out at the top");
        assert_eq!(f.mouse_down(115, 15, false), Event::None, "the check box is out of reach too");
        f.mouse_down(115, 125, false);
        type_text(&mut f, "a");
        assert_eq!(f.text(field), "a");
        assert_eq!(f.mouse_down(205, 165, false), Event::None, "the button below the view stays where it is");
        assert_eq!(f.mouse_up(205, 165), Event::Clicked(Act::Ok));
        f.wheel(150, 50, 100);
        assert_eq!(f.mouse_down(115, 15, false), Event::Toggled(Act::Flag, true));
    }

    #[test]
    fn tab_scrolls_the_field_with_the_keyboard_into_view() {
        let (mut f, _, field, _) = tall();
        f.mouse_down(115, 15, false);
        f.key(Key::Tab);
        let r = f.screen_rect(field);
        assert!(r.y >= 0 && r.y + r.h <= 150, "{r:?}");
    }

    #[test]
    fn page_down_scrolls_a_page_without_a_list() {
        let (mut f, _, field, _) = tall();
        f.key(Key::PageDown);
        assert!(f.screen_rect(field).y < 300, "scrolled");
        f.key(Key::PageUp);
        assert_eq!(f.screen_rect(field).y, 300);
    }

    #[test]
    fn the_scroll_bar_thumb_drags_the_page() {
        let (mut f, _, field, _) = tall();
        // The thumb is at the top of the bar, at the view's right edge.
        f.mouse_down(296, 10, false);
        assert!(f.mouse_move(1000), "dragged");
        f.mouse_up(296, 1000);
        assert_eq!(f.screen_rect(field).y, 300 - 180, "to the end");
        assert!(!f.mouse_move(10), "let go");
    }

    #[test]
    fn a_disabled_control_takes_no_clicks_or_keys() {
        let (mut f, check, field, ok) = tall();
        f.wheel(150, 50, -100);
        f.mouse_down(115, 125, false);
        f.set_disabled(field, true);
        type_text(&mut f, "a");
        assert_eq!(f.text(field), "", "it lost the keyboard");
        assert_eq!(f.mouse_down(115, 125, false), Event::None);
        type_text(&mut f, "a");
        assert_eq!(f.text(field), "", "a click does not give it back");
        f.wheel(150, 50, 100);
        f.set_disabled(check, true);
        assert_eq!(f.mouse_down(115, 15, false), Event::None);
        f.set_disabled(ok, true);
        assert_eq!(f.key(Key::Enter), Event::Activated, "Enter skips a disabled default button");
        f.set_disabled(field, false);
        f.mouse_down(115, 305, false);
        f.wheel(150, 50, -100);
        f.mouse_down(115, 125, false);
        type_text(&mut f, "b");
        assert_eq!(f.text(field), "b");
    }

    #[test]
    fn hidden_controls_are_out_of_reach_and_take_no_room() {
        let (mut f, check, field, _) = tall();
        f.set_hidden(field, true);
        f.wheel(150, 50, -100);
        assert_eq!(f.screen_rect(check).y, 10, "nothing to scroll");
        f.set_hidden(check, true);
        assert_eq!(f.mouse_down(115, 15, false), Event::None);
    }

    #[test]
    fn a_form_too_tall_for_the_screen_shortens_its_view_and_keeps_the_buttons() {
        let (mut f, _, field, ok) = tall();
        f.limit_height(140);
        assert_eq!(f.h, 140);
        let r = f.screen_rect(ok);
        assert_eq!(r.y, 160 - 60, "moved up with the bottom");
        f.wheel(150, 20, -100);
        assert_eq!(f.screen_rect(field).y, 300 - (300 + 22 + 8 - 90), "the view now ends at 90");
        let mut tiny = tall().0;
        tiny.limit_height(10);
        assert_eq!(tiny.h, 200 - 70, "the view keeps two wheel steps");
        let mut low = tall().0;
        low.limit_height(500);
        assert_eq!(low.h, 200, "a form that fits stays");
    }
}
