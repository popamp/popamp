//! Windowing: winit event loop, skinned windows, docking/snapping, pop-up
//! menus, keyboard shortcuts and drag & drop.

use crate::bar::{self, Screen};
use crate::canvas::Canvas;
use crate::dialogs::{self, DialogResult};
use crate::font::Fonts;
use crate::menu::{self, Layout, MenuItem};
use crate::model::{AddMode, AppEvent, Model, UiReq};
use crate::panels::{Panel, PanelKind};
use crate::pen::Pen;
use crate::platform::{self, Mask, Platform, Surface, Win, WinId, WindowRole};
use crate::scale::Scale;
use crate::shortcuts::{self, Click};
use crate::skinwin::{Nav, SkinWin};
use crate::ui::{Input, Out};
use crate::widgets::{self, Event};
use popamp_audio::PlayState;
use popamp_core::config::BarEdge;
use popamp_core::Command;
use popamp_skin::layout::{Dim, Mode, Paint, Placement, Shape, WindowKey};
use popamp_skin::textcfg::region_mask;
use popamp_skin::CursorKind;
use popamp_skin::{Bitmap, Picture};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};
use winit::application::ApplicationHandler;
use winit::dpi::{PhysicalPosition, PhysicalSize};
use winit::event::{DeviceEvent, DeviceId, ElementState, KeyEvent, MouseButton, MouseScrollDelta, StartCause, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow};
use winit::keyboard::{Key, KeyCode, ModifiersState, NamedKey, PhysicalKey};
use winit::window::{CursorIcon, CustomCursor, Window, WindowId, WindowLevel};

struct SkinWindow {
    window: Win,
    surface: Surface,
    /// What the window manager was told the window is.
    role: WindowRole,
    /// The strip reserved for a bar, as last set.
    strip: Option<bar::Strip>,
    /// Top-left in physical screen pixels.
    pos: (i32, i32),
    /// Logical (1x) size.
    size: (i32, i32),
    /// The scale the window is shown at now.
    shown: Scale,
    visible: bool,
    focused: bool,
    /// Cursor in physical window coordinates.
    cursor: (i32, i32),
    frame: Bitmap,
    shape_sig: Option<(usize, i32, i32)>,
    /// The cursor last set on this window.
    pointer: Option<Pointer>,
}

/// What a window's cursor shows.
#[derive(Clone, Copy, PartialEq)]
enum Pointer {
    /// The skin's, for the control under it.
    Skin(CursorKind),
    /// The system's: tracks carried out of the library, whether they can be
    /// let go here; a border in the library that drags.
    System(CursorIcon),
}

struct Popup {
    window: Win,
    surface: Surface,
    items: Vec<MenuItem>,
    layout: Layout,
    hover: Option<usize>,
    pos: (i32, i32),
    frame: Bitmap,
}

/// Tooltip timing, as in the original: appears after the mouse rests on a
/// control for a moment, disappears after a while.
const TIP_DELAY: Duration = Duration::from_millis(600);
const TIP_TIMEOUT: Duration = Duration::from_secs(7);

/// The tooltip for what the mouse rests on in the main window.
struct Tooltip {
    text: String,
    since: Instant,
    /// The window, once shown.
    shown: Option<(Win, Surface)>,
}

/// One of Popamp's own dialogs (see `panels.rs`) in its window.
struct PanelWindow {
    window: Arc<Window>,
    surface: Surface,
    panel: Panel,
    frame: Bitmap,
    /// The size the form is drawn at, in percent.
    pct: i32,
    /// The mouse, in the form's dialog pixels.
    cursor: (i32, i32),
    last_click: Option<(Instant, (i32, i32))>,
}

enum Drag {
    /// `held` is the window the mouse holds.
    Move {
        held: WindowKey,
        group: Vec<(WindowKey, (i32, i32))>,
        start: (i32, i32),
        /// How far the group goes, from [`reach`].
        reach: Reach,
    },
    Resize {
        start_size: (i32, i32),
        start: (i32, i32),
    },
}

pub struct App {
    pub model: Model,
    fonts: Fonts,
    /// The skinned windows, by `idx`.
    skins: [SkinWin; 4],
    wins: [Option<SkinWindow>; 4],
    popups: Vec<Popup>,
    panels: Vec<PanelWindow>,
    #[cfg(feature = "visualizers")]
    visualizer: Option<crate::visualizer::VisualizerWindow>,
    tip: Option<Tooltip>,
    /// The skin's cursors, made into window-system cursors.
    cursors: HashMap<CursorKind, CustomCursor>,
    drag: Option<Drag>,
    /// Windows the user moved away from the bar while it is on, by `idx`:
    /// they stay where they were put until the bar goes.
    off_bar: [bool; 4],
    /// The library is on screen maximized: it neither moves nor docks.
    library_maxed: bool,
    capture: Option<WindowKey>,
    mods: ModifiersState,
    last_click: Option<(WindowKey, Instant, (i32, i32))>,
    drops: Vec<(WindowKey, String)>,
    drop_time: Instant,
    seen_pl_dirty: u64,
    next_frame: Instant,
    /// When a shaped window next looks where the cursor is (macOS).
    next_poll: Instant,
    last_slow: Instant,
    start_minimized: bool,
    popup_focus_check: Option<Instant>,
    /// The companions hidden while the main window is minimized, where
    /// they do not go to the Dock with it (macOS).
    hidden_with_main: Vec<WindowKey>,
    /// Under Wayland on COSMIC: the windows not docked to the main one,
    /// hidden while it is dragged, which would take them along, and when
    /// to show them again where they were should COSMIC not tell that the
    /// main window moved.
    aside: Vec<WindowKey>,
    /// Under Wayland the equalizer and the playlist, parts of the main
    /// window, show above every window: they hide while the maximized
    /// library has the focus.
    covered: bool,
    aside_until: Option<Instant>,
    /// `POPAMP_SNAPSHOT=<dir>`: write each window as PNG after a few frames and exit.
    snapshot: Option<(std::path::PathBuf, u32)>,
    platform: Platform,
    media: crate::media::Controls,
    hotkeys: crate::hotkeys::Hotkeys,
    /// The monitor the main window is on.
    monitor: Option<Screen>,
    /// Under Wayland, where only the main window is one for the system:
    /// the window the keys go to, the one last clicked.
    key_focus: WindowKey,
    /// Under Wayland: the main window has the keyboard.
    active: bool,
    /// Under Wayland: the skin cursor shown over the equalizer and the
    /// playlist, which share one; `None` is the desktop's arrow.
    sub_cursor: Option<CursorKind>,
}

impl App {
    pub fn new(model: Model, start_minimized: bool, media: crate::media::Controls, hotkeys: crate::hotkeys::Hotkeys) -> App {
        let fonts = Fonts::for_skin(&model.skin);
        App {
            model,
            fonts,
            skins: WindowKey::ALL.map(SkinWin::new),
            wins: [None, None, None, None],
            popups: Vec::new(),
            #[cfg(feature = "visualizers")]
            visualizer: None,
            panels: Vec::new(),
            tip: None,
            cursors: HashMap::new(),
            drag: None,
            off_bar: [false; 4],
            library_maxed: false,
            capture: None,
            mods: ModifiersState::empty(),
            last_click: None,
            drops: Vec::new(),
            drop_time: Instant::now(),
            seen_pl_dirty: u64::MAX,
            next_frame: Instant::now(),
            next_poll: Instant::now(),
            last_slow: Instant::now(),
            start_minimized,
            popup_focus_check: None,
            hidden_with_main: Vec::new(),
            aside: Vec::new(),
            covered: false,
            aside_until: None,
            snapshot: std::env::var_os("POPAMP_SNAPSHOT").map(|d| (std::path::PathBuf::from(d), 0)),
            platform: Platform::connect(),
            media,
            hotkeys,
            monitor: None,
            key_focus: WindowKey::Main,
            active: false,
            sub_cursor: None,
        }
    }

    fn scale(&self, k: WindowKey) -> Scale {
        crate::bind::scale(&self.model, k)
    }

    fn logical_size(&self, k: WindowKey) -> (i32, i32) {
        crate::bind::window_size(&self.model, k)
    }

    fn placement(&self, k: WindowKey) -> Placement {
        crate::bind::placement(&self.model, k)
    }

    fn wanted_visible(&self, k: WindowKey) -> bool {
        !self.model.in_tray && crate::bind::shown(&self.model, k)
    }

    fn saved_pos(&self, k: WindowKey) -> (i32, i32) {
        crate::bind::position(&self.model, k)
    }

    /// Remembers where a window is; not while a bar places the windows,
    /// so they return to where they were when the bar goes.
    fn store_pos(&mut self, k: WindowKey, p: (i32, i32)) {
        if bar::edge(&self.model).is_none() {
            crate::bind::set_position(&mut self.model, k, p)
        }
    }

    fn win(&self, k: WindowKey) -> Option<&SkinWindow> {
        self.wins[k.index()].as_ref()
    }

    fn win_mut(&mut self, k: WindowKey) -> Option<&mut SkinWindow> {
        self.wins[k.index()].as_mut()
    }

    fn kind_of(&self, id: WinId) -> Option<WindowKey> {
        WindowKey::ALL.into_iter().find(|&k| self.win(k).is_some_and(|w| w.window.id() == id))
    }

    fn role(&self, k: WindowKey) -> WindowRole {
        // The maximized library is a window of its own, as other programs'
        // windows are, not one docked to the main window: a window manager
        // keeps docked ones above the program's dialogs.
        if k == WindowKey::Library && self.model.settings().ml_max {
            return WindowRole::Main;
        }
        match self.placement(k) {
            Placement::Free => WindowRole::Main,
            Placement::Docked => WindowRole::Docked,
            Placement::Bar(_) => WindowRole::Bar,
        }
    }

    /// The screens, in the desktop's units, with their pixels per unit.
    fn screens(&self) -> Vec<(Screen, f64)> {
        let Some(top) = self.win(WindowKey::Main).and_then(|w| w.window.top()) else { return Vec::new() };
        top.available_monitors().map(|m| (platform::monitor(&m), m.scale_factor())).collect()
    }

    fn monitors(el: &ActiveEventLoop) -> Vec<Screen> {
        el.available_monitors().map(|m| platform::monitor(&m)).collect()
    }

    /// The monitor with the main window's top-left corner, else the first.
    fn screen_of_main(&self, monitors: &[Screen]) -> Option<Screen> {
        let (x, y) = self.win(WindowKey::Main).map_or_else(|| self.saved_pos(WindowKey::Main), |w| w.pos);
        monitors.iter().find(|m| x >= m.x && x < m.x + m.w && y >= m.y && y < m.y + m.h).or(monitors.first()).copied()
    }

    /// Finds again what a bar spans on the main window's monitor; true
    /// when it changed, as when a desktop panel appears or resizes.
    fn update_bar_area(&mut self) -> bool {
        // The strip as reserved, not where the window is: the system may
        // move the bar out of its own strip when the work area shrinks.
        let strip = self.win(WindowKey::Main).filter(|w| w.role == WindowRole::Bar && w.visible).and_then(|w| w.strip).map(|s| s.area());
        let under = self.model.settings().bar_under_side_panels;
        let mut area = self.monitor.map(|m| bar::span(m, &self.platform.work_areas(), strip, under));
        // A Wayland compositor gives the bar its width itself.
        if let (Some(area), Some(w)) = (&mut area, self.platform.bar_width().filter(|_| self.platform.on_wayland())) {
            area.w = w;
        }
        let changed = area != self.model.screen;
        self.model.screen = area;
        changed
    }

    fn create_windows(&mut self, el: &ActiveEventLoop) {
        self.platform.start(el);
        self.model.no_bar = !self.platform.can_bar();
        self.model.bar_width_given = self.platform.on_wayland();
        let monitors = Self::monitors(el);
        self.monitor = self.screen_of_main(&monitors);
        self.update_bar_area();
        for k in WindowKey::ALL {
            if !self.create_window(el, k, &monitors) {
                el.exit();
                return;
            }
        }
        self.build_cursors(el);
        self.attach_docked();
        self.apply_layout(WindowKey::Main);
        if self.start_minimized {
            self.model.exec(Command::Minimize);
        }
    }

    /// Makes the window for `k`, hidden; false if the system cannot.
    fn create_window(&mut self, el: &ActiveEventLoop, k: WindowKey, monitors: &[Screen]) -> bool {
        let scale = self.scale(k);
        let (w, h) = self.logical_size(k);
        let mut pos = self.saved_pos(k);
        let wayland = self.platform.on_wayland();
        // Keep windows on a connected monitor. Not under Wayland, where
        // nobody tells where on the monitors a window is.
        if self.model.settings().keeponscreen && !monitors.is_empty() && !wayland {
            let on_screen =
                monitors.iter().any(|m| pos.0 + 20 > m.x && pos.0 < m.x + m.w - 20 && pos.1 >= m.y - 5 && pos.1 < m.y + m.h - 14);
            if !on_screen {
                pos = (monitors[0].x + 100, monitors[0].y + 100 + scale.px(k.index() as i32 * 116));
            }
        }
        if wayland && k != WindowKey::Main {
            pos = self.near_main(k, pos, monitors);
        }
        let role = self.role(k);
        let own = k == WindowKey::Library && self.model.settings().ml_max;
        let mut attrs = platform::attributes(role)
            .with_title(crate::bind::title(k))
            .with_inner_size(platform::size(scale.px(w), scale.px(h)))
            .with_position(platform::position(pos.0, pos.1))
            .with_visible(false)
            .with_window_icon(window_icon());
        if self.model.settings().aot {
            attrs = attrs.with_window_level(WindowLevel::AlwaysOnTop);
        }
        if own && self.compositor_maximizes() {
            attrs = attrs.with_resizable(true).with_maximized(true);
        }
        // Under Wayland only the main window is a window; its picture
        // carries its shape, so it has see-through pixels.
        let made = if wayland && k != WindowKey::Main && !own {
            self.platform.child(pos, true).ok_or_else(|| "no main window to place it against".to_string())
        } else if let (true, Some(edge)) = (wayland, bar::edge(&self.model).filter(|_| role == WindowRole::Bar)) {
            // The bar is a layer of the desktop. A window of winit's is
            // still made for the connection, and never shown.
            if !self.platform.joined() {
                let attrs = platform::attributes(WindowRole::Main).with_title(crate::bind::title(k)).with_visible(false);
                match el.create_window(attrs) {
                    Ok(w) => drop(self.platform.surface(&Arc::new(w), pos)),
                    Err(e) => log::error!("cannot create window: {e}"),
                }
            }
            self.platform.bar(edge, pos, (scale.px(w), scale.px(h))).ok_or_else(|| "the compositor makes no bar".to_string())
        } else {
            el.create_window(attrs.with_transparent(wayland)).map_err(|e| e.to_string()).and_then(|w| {
                let w = Arc::new(w);
                w.set_cursor(CursorIcon::Default);
                // The window made through the platform is the main one: under
                // Wayland the one the others are parts of. The library's own
                // is not it.
                let surface = if own { Surface::soft(&w)? } else { self.platform.surface(&w, pos)? };
                Ok((Win::Top(w.clone()), surface))
            })
        };
        let (window, surface) = match made {
            Ok(made) => made,
            Err(e) => {
                log::error!("cannot create window: {e}");
                return false;
            }
        };
        self.wins[k.index()] = Some(SkinWindow {
            window,
            surface,
            role,
            strip: None,
            pos,
            size: (w, h),
            shown: scale,
            visible: false,
            focused: false,
            cursor: (0, 0),
            frame: Bitmap::new(w, h, 0),
            shape_sig: None,
            pointer: None,
        });
        self.store_pos(k, pos);
        true
    }

    /// Under Wayland, where the equalizer, the playlist and the library are
    /// parts of the main window: one kept more than half a screen away from
    /// it, as an X11 session or a stray drag may leave it, goes back against
    /// it, since off the screen nothing could bring it back.
    fn near_main(&self, k: WindowKey, pos: (i32, i32), monitors: &[Screen]) -> (i32, i32) {
        let main = self.saved_pos(WindowKey::Main);
        let screen = monitors.first().map_or((1920, 1080), |m| (m.w, m.h));
        if (pos.0 - main.0).abs() <= screen.0 / 2 && (pos.1 - main.1).abs() <= screen.1 / 2 {
            return pos;
        }
        let px = |k: WindowKey| {
            let (w, h) = self.logical_size(k);
            (self.scale(k).px(w), self.scale(k).px(h))
        };
        let (mw, mh) = px(WindowKey::Main);
        let pos = match k {
            WindowKey::Eq => (main.0, main.1 + mh),
            // Under the equalizer, where it is shown.
            WindowKey::Playlist => match self.win(WindowKey::Eq).filter(|_| self.wanted_visible(WindowKey::Eq)) {
                Some(eq) => (eq.pos.0, eq.pos.1 + px(WindowKey::Eq).1),
                None => (main.0, main.1 + mh),
            },
            _ => (main.0 + mw, main.1),
        };
        log::info!("Wayland: {k:?} was far from the main window, put back against it at {pos:?}");
        pos
    }

    /// Minimizes or restores the windows. Where the system does that with
    /// the main window for the windows tied to it, only the main window:
    /// a minimized owned window would stay behind as a stub on the desktop.
    fn set_minimized(&mut self, on: bool) {
        let main = self.win(WindowKey::Main).filter(|w| w.role == WindowRole::Main).and_then(|w| w.window.top().cloned());
        if let (true, Some(main)) = (platform::MINIMIZE_HIDES_COMPANIONS, main) {
            // One Dock tile: the main window goes there, its companions hide.
            main.set_minimized(on);
            if on {
                for k in WindowKey::ALL.into_iter().filter(|&k| k != WindowKey::Main) {
                    if let Some(w) = self.win_mut(k).filter(|w| w.visible) {
                        if let Some(top) = w.window.top() {
                            top.set_visible(false);
                        }
                        w.visible = false;
                        self.hidden_with_main.push(k);
                    }
                }
            } else {
                self.show_hidden_companions();
            }
            return;
        }
        let owned = platform::OWNED_FOLLOW_OWNER && self.win(WindowKey::Main).is_some_and(|w| w.role == WindowRole::Main);
        for k in WindowKey::ALL {
            if owned && k != WindowKey::Main {
                continue;
            }
            if let Some(w) = self.win(k).filter(|w| w.visible).and_then(|w| w.window.top()) {
                w.set_minimized(on);
            }
        }
    }

    /// Shows again the companions hidden when the main window was minimized.
    fn show_hidden_companions(&mut self) {
        for k in std::mem::take(&mut self.hidden_with_main) {
            let wanted = self.wanted_visible(k);
            if let Some(w) = self.win_mut(k).filter(|_| wanted) {
                if let Some(top) = w.window.top() {
                    top.set_visible(true);
                }
                w.visible = true;
            }
        }
    }

    /// Ties the equalizer and the playlist to an ordinary main window, so
    /// that the window manager brings all three up together. A bar is a
    /// panel of the desktop, and nothing is tied to it.
    fn attach_docked(&self) {
        let owner = self.win(WindowKey::Main).filter(|w| w.role == WindowRole::Main).and_then(|w| w.window.top()).map(|w| &**w);
        for k in [WindowKey::Eq, WindowKey::Playlist, WindowKey::Library] {
            if let Some(w) = self.win(k).filter(|w| w.role == WindowRole::Docked).and_then(|w| w.window.top()) {
                if let Err(e) = self.platform.attach(w, owner) {
                    log::warn!("cannot tie {k:?} to the main window: {e}");
                }
            }
        }
    }

    /// After a skin change: windows that became a bar or stopped being one
    /// are made again, since the window manager reads a window's type only
    /// when it first shows it.
    fn remake_changed_windows(&mut self, el: &ActiveEventLoop) {
        let monitors = Self::monitors(el);
        self.monitor = self.screen_of_main(&monitors);
        self.update_bar_area();
        let left_bar = self.win(WindowKey::Main).is_some_and(|w| w.role == WindowRole::Bar) && bar::edge(&self.model).is_none();
        // Under Wayland the other windows are parts of the main window or
        // the bar: they go first, and come again with the new one.
        if self.platform.on_wayland() && self.win(WindowKey::Main).is_some_and(|w| w.role != self.role(WindowKey::Main)) {
            self.popups.clear();
            self.tip = None;
            for k in WindowKey::ALL.into_iter().rev() {
                self.wins[k.index()] = None;
            }
        }
        for k in WindowKey::ALL {
            if self.win(k).is_none_or(|w| w.role != self.role(k)) {
                if let Some(bar) = self.win(k).and_then(|w| w.window.top().filter(|_| w.role == WindowRole::Bar)) {
                    self.platform.release(bar);
                }
                self.wins[k.index()] = None;
                if !self.create_window(el, k, &monitors) {
                    el.exit();
                    return;
                }
            }
        }
        self.attach_docked();
        if left_bar {
            self.off_bar = [false; 4];
            for k in [WindowKey::Eq, WindowKey::Playlist, WindowKey::Library] {
                let pos = self.saved_pos(k);
                if let Some(w) = self.win_mut(k) {
                    w.pos = pos;
                    w.window.move_to(pos);
                }
            }
        }
    }

    /// The shown windows' rectangles, of those that dock: a maximized
    /// library, or one coming back from it, takes no part in docking.
    fn docking_rects(&self, maxed: bool) -> Vec<Option<Rect>> {
        let docking = |k: WindowKey| k != WindowKey::Library || (!maxed && !self.library_maxed);
        WindowKey::ALL
            .iter()
            .map(|&k| {
                let w = self.win(k).filter(|w| w.visible && docking(k))?;
                Some((w.pos.0, w.pos.1, w.shown.px(w.size.0), w.shown.px(w.size.1)))
            })
            .collect()
    }

    /// Where the library goes instead of `planned`: the corner of its area
    /// when maximized, back to its own place when it stops being.
    fn library_place(&mut self, maxed: Option<Screen>, planned: (i32, i32)) -> (i32, i32) {
        let was = std::mem::replace(&mut self.library_maxed, maxed.is_some());
        match maxed {
            Some(a) => (a.x, a.y),
            None if was => self.saved_pos(WindowKey::Library),
            None => planned,
        }
    }

    /// Under Wayland nobody tells a program where its windows are or where
    /// the panels are: the maximized library is a window of its own there,
    /// which the compositor maximizes and sizes.
    fn compositor_maximizes(&self) -> bool {
        self.platform.on_wayland() && self.model.settings().ml_max
    }

    /// Finds where the library goes when it is maximized; under Wayland the
    /// compositor says, in its size for the window.
    fn update_library_area(&mut self) {
        if self.platform.on_wayland() {
            if !self.model.settings().ml_max {
                self.model.library_area = None;
            }
            return;
        }
        let (x, y) = self.saved_pos(WindowKey::Library);
        let (w, h) = crate::bind::stored_size(&self.model, WindowKey::Library);
        let scale = self.scale(WindowKey::Library);
        let monitors: Vec<Screen> = self.screens().into_iter().map(|(m, _)| m).collect();
        let area =
            self.model.settings().ml_max.then(|| library_area(&monitors, &self.platform.work_areas(), (x, y, scale.px(w), scale.px(h))));
        self.model.library_area = area.flatten();
    }

    /// Applies sizes, scale and visibility; keeps docked windows attached
    /// when windows change size; `anchor` stays where it is.
    fn apply_layout(&mut self, anchor: WindowKey) {
        self.update_library_area();
        let maxed = crate::bind::maximized_area(&self.model, WindowKey::Library);
        let old = self.docking_rects(maxed.is_some());
        let bar = bar::edge(&self.model).zip(self.model.screen);
        let mut plan = self.plan(&old, anchor, bar);
        let pl = WindowKey::Playlist.index();
        if bar.is_none() && !self.platform.on_wayland() && self.keep_playlist_grip_on_screen(old[pl], plan.pos[pl]) {
            plan = self.plan(&old, anchor, bar);
        }
        let Plan { sizes, pos: mut new_pos } = plan;
        new_pos[WindowKey::Library.index()] = self.library_place(maxed, new_pos[WindowKey::Library.index()]);
        let wayland = self.platform.on_wayland();
        for (i, &k) in WindowKey::ALL.iter().enumerate() {
            let visible = self.wanted_visible(k);
            let (lw, lh) = self.logical_size(k);
            let pos = new_pos[i];
            let scale = self.scale(k);
            if let Some(w) = self.win_mut(k) {
                let size = (scale.px(lw), scale.px(lh));
                // The compositor sizes the library it maximized.
                let own = wayland && k == WindowKey::Library && w.role == WindowRole::Main;
                if let Some(top) = w.window.top().filter(|_| !own) {
                    // winit holds a window that cannot be resized by the
                    // user to its first size under Wayland, as its least
                    // and greatest size: those go along.
                    if wayland {
                        top.set_min_inner_size(Some(platform::size(size.0, size.1)));
                        top.set_max_inner_size(Some(platform::size(size.0, size.1)));
                    }
                    let _ = top.request_inner_size(platform::size(size.0, size.1));
                }
                if w.pos != pos {
                    w.window.move_to(pos);
                    w.pos = pos;
                }
                w.size = (lw, lh);
                w.shown = scale;
                if w.visible != visible {
                    if let Some(top) = w.window.top() {
                        top.set_visible(visible);
                    }
                    if !visible {
                        w.surface.hide(size);
                    }
                    w.visible = visible;
                }
                w.window.request_redraw();
            }
            self.store_pos(k, pos);
        }
        if let Some((edge, screen)) = bar {
            self.reserve_bar_strip(edge, screen, sizes[0].1);
        }
        self.update_shapes();
    }

    /// The windows' sizes in screen pixels and their top-left corners for
    /// the sizes and scale in the settings now.
    fn plan(&self, old: &[Option<Rect>], anchor: WindowKey, bar: Option<(BarEdge, Screen)>) -> Plan {
        let sizes: Vec<(i32, i32)> = WindowKey::ALL
            .iter()
            .map(|&k| {
                let (lw, lh) = self.logical_size(k);
                (self.scale(k).px(lw), self.scale(k).px(lh))
            })
            .collect();
        let new_pos: Vec<(i32, i32)> = match bar {
            Some((edge, screen)) => {
                // The library, and a window the user moved off the bar,
                // stays where the user has it.
                let off_bar = |k: WindowKey| k == WindowKey::Library || self.off_bar[k.index()];
                let pl_by_bar = self.wanted_visible(WindowKey::Playlist) && !off_bar(WindowKey::Playlist);
                let by_bar = bar::places(edge, screen, [sizes[0], sizes[1], sizes[2]], pl_by_bar);
                let user = |k: WindowKey| self.win(k).map_or_else(|| self.saved_pos(k), |w| w.pos);
                WindowKey::ALL.iter().map(|&k| if off_bar(k) { user(k) } else { by_bar[k.index()] }).collect()
            }
            None => {
                // Without a bar all windows have one scale: the equalizer's.
                let (scale, shown) = (self.scale(WindowKey::Eq), self.win(WindowKey::Eq).map_or(Scale::ONE, |w| w.shown));
                let moved = follow_docked(old, &sizes, anchor.index(), |d| scale.px(shown.skin(d)));
                // Under Wayland the main window is where the compositor has
                // it, so the others move instead of it.
                let main = self.win(WindowKey::Main).filter(|w| w.visible && self.platform.on_wayland());
                let back = main.map_or((0, 0), |w| (w.pos.0 - moved[0].0, w.pos.1 - moved[0].1));
                let place = |w: &SkinWindow, p: (i32, i32)| if w.visible { (p.0 + back.0, p.1 + back.1) } else { w.pos };
                WindowKey::ALL.iter().zip(&moved).map(|(&k, &p)| self.win(k).map_or((0, 0), |w| place(w, p))).collect()
            }
        };
        Plan { sizes, pos: new_pos }
    }

    /// Shortens the playlist where a resize or a bigger window size would
    /// push its resize grip out of its monitor's usable area: off the
    /// screen the grip cannot be reached, and the window manager moves a
    /// window that grows over a panel. True if the stored size changed.
    fn keep_playlist_grip_on_screen(&mut self, old: Option<Rect>, pos: (i32, i32)) -> bool {
        let (Some(old), Some(w)) = (old, self.win(WindowKey::Playlist)) else { return false };
        let Some(def) = self.model.skin.layout.window(WindowKey::Playlist) else { return false };
        let Some(top) = w.window.top() else { return false };
        let monitors: Vec<Screen> = top.available_monitors().map(|m| platform::monitor(&m)).collect();
        let Some(monitor) = monitors.into_iter().find(|m| pos.0 >= m.x && pos.0 < m.x + m.w && pos.1 >= m.y && pos.1 < m.y + m.h) else {
            return false;
        };
        let area = bar::usable(monitor, &self.platform.work_areas(), None);
        let scale = self.scale(WindowKey::Playlist);
        let stored = crate::bind::stored_size(&self.model, WindowKey::Playlist);
        let fitted = (
            fit_grown(def.normal.width, stored.0, old.0 + old.2, |n| pos.0 + scale.px(n), area.x + area.w),
            fit_grown(def.normal.height, stored.1, old.1 + old.3, |n| pos.1 + scale.px(n), area.y + area.h),
        );
        if fitted == stored {
            return false;
        }
        let s = self.model.settings_mut();
        (s.pe_width, s.pe_height) = fitted;
        true
    }

    /// Tells the window manager to keep other windows out of the bar's strip.
    fn reserve_bar_strip(&mut self, edge: BarEdge, screen: Screen, bar_h: i32) {
        let strip = bar::Strip { edge, screen, height: bar_h };
        let Some(w) = self.wins[WindowKey::Main.index()].as_mut() else { return };
        if w.strip == Some(strip) {
            return;
        }
        // A Wayland bar keeps its strip itself; it only changes edge.
        #[cfg(linux_like)]
        if let platform::Win::Sub(bar) = &w.window {
            bar.set_bar_edge(edge);
            w.strip = Some(strip);
            return;
        }
        let Some(top) = w.window.top() else { return };
        match self.platform.reserve(top, strip) {
            Ok(()) => w.strip = Some(strip),
            Err(e) => log::warn!("cannot reserve the bar's strip: {e}"),
        }
    }

    /// Applies the window shapes of the modes on screen.
    fn update_shapes(&mut self) {
        for k in WindowKey::ALL {
            let scale = self.scale(k);
            let def = self.model.skin.layout.window(k);
            let shape = def.and_then(|d| d.mode(crate::bind::view(&self.model, k)).shape.as_ref());
            let Some(w) = self.wins[k.index()].as_mut() else { continue };
            let size = (scale.px(w.size.0), scale.px(w.size.1));
            let sig = (shape.map_or(0, |s| s as *const Shape as usize), size.0, size.1);
            if w.shape_sig == Some(sig) {
                continue;
            }
            w.shape_sig = Some(sig);
            let mode = def.map(|d| d.mode(crate::bind::view(&self.model, k)));
            let mask = shape.zip(mode).and_then(|(s, mode)| shape_mask(&self.model, mode, s, w.size, scale));
            if let Some(Err(e)) = w.window.top().map(|top| self.platform.set_shape(top, mask.as_ref(), size)) {
                log::debug!("shape: {e}");
            }
            w.surface.set_shape(mask.as_ref(), size);
        }
    }

    fn render(&mut self, k: WindowKey) {
        let i = k.index();
        let layout = self.scale(k);
        if self.aside.contains(&k) || (self.covered && matches!(k, WindowKey::Eq | WindowKey::Playlist)) {
            return;
        }
        let Some(w) = self.wins[i].as_mut() else { return };
        if !w.visible {
            return;
        }
        // In screen pixels: the window's size in the desktop's units at its
        // screen's pixels per unit (2 on a Retina Mac, else 1).
        let scale = layout.on_screen(if platform::DESKTOP_IN_POINTS { w.window.scale_factor() } else { 1.0 });
        let (lw, lh) = w.size;
        // Vector skins are drawn at screen size, classic ones at x1 and scaled.
        let drawn = if self.model.skin.vector { scale } else { Scale::ONE };
        let (fw, fh) = (drawn.px(lw), drawn.px(lh));
        if w.frame.w != fw || w.frame.h != fh {
            w.frame = Bitmap::new(fw, fh, 0);
        }
        let active = w.focused;
        self.skins[k.index()].draw(&self.model, &mut self.fonts, &mut Canvas::new(&mut w.frame, drawn), active);
        let factor = if platform::DESKTOP_IN_POINTS { w.window.scale_factor() } else { 1.0 };
        let (pw, ph) = surface_size(layout, (lw, lh), factor);
        let frame = &w.frame;
        w.surface.present((pw as i32, ph as i32), |buf| scale.present(frame, buf, pw as usize, ph as usize));
    }

    fn redraw_all(&mut self) {
        for k in WindowKey::ALL {
            self.render(k);
        }
        for i in 0..self.popups.len() {
            self.render_popup(i);
        }
    }

    // --- pop-up menus ---------------------------------------------------------

    fn cursor_screen(&self, k: WindowKey) -> (i32, i32) {
        if let Some(at) = self.platform.pointer() {
            return at;
        }
        self.win(k).map(|w| (w.pos.0 + w.cursor.0, w.pos.1 + w.cursor.1)).unwrap_or((100, 100))
    }

    /// Opens a menu at `at`; a submenu there with `sub`.
    fn open_popup(&mut self, el: &ActiveEventLoop, items: Vec<MenuItem>, at: (i32, i32), sub: bool) {
        if items.is_empty() {
            return;
        }
        // Drawn at the density of the screen it opens on, placed within it.
        let screens = self.screens();
        let pct = if platform::DESKTOP_IN_POINTS { pct_on(&screens, at, self.dialog_pct()) } else { self.dialog_pct() };
        let layout = menu::layout(&items, &mut self.fonts.list, pct);
        let (w, h) = layout.screen();
        // Under Wayland a bar is where Popamp put it, on the monitor it
        // spans: its menus open above it at the bottom, as elsewhere.
        let bar = self.win(WindowKey::Main).is_some_and(|w| w.role == WindowRole::Bar);
        let main_monitor =
            if self.platform.on_wayland() && bar { self.monitor } else { self.main_monitor().map(|m| platform::monitor(&m)) };
        let monitor = menu_screen(platform::DESKTOP_IN_POINTS, &screens, at, main_monitor);
        // Placed in the desktop's units, as the cursor and the monitor are.
        let units = (platform::from_drawn(w, pct), platform::from_drawn(h, pct));
        let (x, y) = monitor.map_or(at, |m| menu_origin(at, units, m, !sub));
        let attrs = platform::attributes(WindowRole::Popup)
            .with_inner_size(platform::drawn_size(w, h, pct))
            .with_position(platform::position(x, y));
        let Some((window, surface)) = self.overlay(el, attrs, (x, y), true) else { return };
        let frame = Bitmap::new(w, h, 0);
        self.popups.push(Popup { window, surface, items, layout, hover: None, pos: (x, y), frame });
        let i = self.popups.len() - 1;
        self.render_popup(i);
    }

    /// A window above ours that the system does not manage, for a menu or
    /// a tooltip, at `pos`; under Wayland a part of the main window.
    /// Without `input` the mouse goes through it where the system can do that.
    fn overlay(
        &mut self,
        el: &ActiveEventLoop,
        attrs: winit::window::WindowAttributes,
        pos: (i32, i32),
        input: bool,
    ) -> Option<(Win, Surface)> {
        if self.platform.on_wayland() {
            return self.platform.child(pos, input);
        }
        let window = el.create_window(attrs).map(Arc::new).ok()?;
        self.platform.no_activate(&window);
        let surface = Surface::soft(&window).ok()?;
        Some((Win::Top(window), surface))
    }

    /// The monitor the main window is on, where the system tells.
    fn main_monitor(&self) -> Option<winit::monitor::MonitorHandle> {
        // Under Wayland a monitor is known, but not where the window is on it.
        let main = self.win(WindowKey::Main).filter(|_| !self.platform.on_wayland())?;
        main.window.top()?.current_monitor()
    }

    fn render_popup(&mut self, i: usize) {
        let Some(p) = self.popups.get_mut(i) else { return };
        menu::draw(&mut p.frame, &p.items, &p.layout, p.hover, &mut self.fonts.list);
        present_unscaled(&mut p.surface, &p.frame);
    }

    // --- own dialogs ------------------------------------------------------------

    /// Opens the dialog `kind`, or raises it
    /// when it is already open.
    /// One visualizer at a time: opening another replaces it.
    #[cfg(feature = "visualizers")]
    fn open_visualizer(&mut self, el: &ActiveEventLoop, id: &str) {
        self.close_visualizer();
        let Some(plugin) = self.model.visual_plugin(id) else { return };
        let (shared, events) = (self.model.engine.shared().clone(), self.model.events().clone());
        match crate::visualizer::VisualizerWindow::open(el, &plugin, shared, events) {
            Ok(v) => self.visualizer = Some(v),
            Err(why) => self.model.on_plugin_stopped(id, &why),
        }
    }

    #[cfg(feature = "visualizers")]
    fn close_visualizer(&mut self) {
        if let Some(v) = self.visualizer.take() {
            v.close();
        }
    }

    fn open_panel(&mut self, el: &ActiveEventLoop, kind: PanelKind) {
        if let Some(i) = self.panels.iter().position(|p| p.panel.kind() == kind.window()) {
            // File info opened again is about the track it is opened on now.
            if kind == PanelKind::FileInfo {
                self.panels[i].panel = Panel::open(kind, &self.model);
            }
            self.panels[i].panel.show(kind);
            self.panels[i].window.focus_window();
            self.render_panel(i);
            return;
        }
        let mut panel = Panel::open(kind, &self.model);
        let pct = self.dialog_pct();
        // Centred on the main window's monitor, so the title bar is on screen.
        let monitor = self.main_monitor().map(|m| platform::monitor(&m));
        if let Some(room) = self.dialog_room(el, monitor, pct) {
            panel.form_mut().limit_height(room);
        }
        let (w, h) = (crate::pen::screen(pct, panel.form().w), crate::pen::screen(pct, panel.form().h));
        let (cx, cy) = monitor.map_or((640, 400), |m| (m.x + m.w / 2, m.y + m.h / 2));
        let (uw, uh) = (platform::from_drawn(w, pct), platform::from_drawn(h, pct));
        let attrs = platform::attributes(WindowRole::Dialog)
            .with_title(panel.title())
            .with_inner_size(platform::drawn_size(w, h, pct))
            .with_position(platform::position((cx - uw / 2).max(0), (cy - uh / 2).max(0)));
        let Ok(window) = el.create_window(attrs).map(Arc::new) else { return };
        let Ok(surface) = Surface::soft(&window) else { return };
        let frame = Bitmap::new(w, h, 0);
        self.panels.push(PanelWindow { window, surface, panel, frame, pct, cursor: (0, 0), last_click: None });
        self.panel_changed(self.panels.len() - 1);
    }

    /// How tall a dialog drawn at `pct` may be, in dialog pixels: what the
    /// desktop's panels leave of the main window's monitor, less room for
    /// the title bar. Under Wayland, where that monitor is not known, the
    /// lowest monitor.
    fn dialog_room(&self, el: &ActiveEventLoop, monitor: Option<Screen>, pct: i32) -> Option<i32> {
        let lowest = || el.available_monitors().map(|m| platform::monitor(&m)).min_by_key(|m| m.h);
        let area = bar::usable(monitor.or_else(lowest)?, &self.platform.work_areas(), None);
        // The title bar and a margin, in the desktop's units.
        const FRAME: i32 = 80;
        let units = area.h - FRAME;
        let px = if platform::DESKTOP_IN_POINTS { units * pct / 100 } else { units };
        Some(crate::pen::dialog(pct, px))
    }

    /// Starts the network work panel `i` asked for and redraws it.
    fn panel_changed(&mut self, i: usize) {
        for job in self.panels[i].panel.take_jobs() {
            crate::museum::start(job, self.model.settings().proxy.clone(), self.model.events().clone());
        }
        for job in self.panels[i].panel.take_gallery_jobs() {
            crate::gallery::start(job, self.model.settings().proxy.clone(), self.model.events().clone());
        }
        self.render_panel(i);
    }

    /// The size menus, tooltips and dialogs are drawn at, in percent: the
    /// display scaling of the main window's monitor, as for the menus and
    /// dialogs of other programs (Windows' scaling setting, `Xft.dpi` on
    /// X11). Not the skin's window size: a player made bigger keeps
    /// dialogs of the usual size.
    fn dialog_pct(&self) -> i32 {
        let factor = self.win(WindowKey::Main).and_then(|w| w.window.top()).map_or(1.0, |w| w.scale_factor());
        ((factor * 100.0).round() as i32).max(100)
    }

    fn render_panel(&mut self, i: usize) {
        let main_pct = self.dialog_pct();
        let Some(p) = self.panels.get_mut(i) else { return };
        // A dialog dragged to another screen is drawn at that screen's density.
        let pct = if platform::DESKTOP_IN_POINTS { ((p.window.scale_factor() * 100.0).round() as i32).max(100) } else { main_pct };
        let form = p.panel.form();
        let (w, h) = (crate::pen::screen(pct, form.w), crate::pen::screen(pct, form.h));
        if (w, h) != (p.frame.w, p.frame.h) {
            // The windows changed size while the dialog was open.
            p.frame = Bitmap::new(w, h, 0);
            p.pct = pct;
            let _ = p.window.request_inner_size(platform::drawn_size(w, h, pct));
        }
        form.draw(&mut Pen::new(&mut p.frame, &mut self.fonts.list, pct));
        present_unscaled(&mut p.surface, &p.frame);
    }

    fn panel_event(&mut self, el: &ActiveEventLoop, i: usize, ev: WindowEvent) {
        let p = &mut self.panels[i];
        let (x, y) = p.cursor;
        let form = p.panel.form_mut();
        let out = match ev {
            WindowEvent::CloseRequested => Event::Cancel,
            WindowEvent::RedrawRequested | WindowEvent::ScaleFactorChanged { .. } => Event::None,
            WindowEvent::ModifiersChanged(m) => {
                self.mods = m.state();
                return;
            }
            WindowEvent::CursorMoved { position, .. } => {
                p.cursor = (crate::pen::dialog(p.pct, position.x as i32), crate::pen::dialog(p.pct, position.y as i32));
                // Dragging the scroll bar scrolls.
                if form.mouse_move(p.cursor.1) {
                    self.render_panel(i);
                }
                return;
            }
            WindowEvent::MouseInput { state: ElementState::Pressed, button: MouseButton::Left, .. } => {
                let now = Instant::now();
                let double = p.last_click.is_some_and(|(t, at)| now - t < Duration::from_millis(400) && at == (x, y));
                p.last_click = Some((now, (x, y)));
                form.mouse_down(x, y, double)
            }
            WindowEvent::MouseInput { state: ElementState::Released, button: MouseButton::Left, .. } => form.mouse_up(x, y),
            WindowEvent::MouseWheel { delta, .. } => {
                form.wheel(
                    x,
                    y,
                    match delta {
                        MouseScrollDelta::LineDelta(_, y) => y.round() as i32,
                        MouseScrollDelta::PixelDelta(p) => (p.y / 20.0).round() as i32,
                    },
                );
                Event::None
            }
            WindowEvent::KeyboardInput { event, .. } if event.state == ElementState::Pressed => {
                match form_key(
                    &event,
                    self.mods.control_key() || self.mods.alt_key() || (cfg!(target_os = "macos") && self.mods.super_key()),
                ) {
                    Some(k) => form.key(k),
                    None => return,
                }
            }
            _ => return,
        };
        if !p.panel.handle(out, &mut self.model) {
            self.panels.remove(i);
        } else {
            self.panel_changed(i);
        }
        self.after_model(el);
        self.redraw_all();
    }

    /// Turns the skin's cursors into window-system cursors (none when the
    /// skin has none or `usecursors` is off).
    fn build_cursors(&mut self, el: &ActiveEventLoop) {
        self.cursors.clear();
        if self.model.settings().usecursors {
            for (kind, c) in &self.model.skin.cursors {
                match CustomCursor::from_rgba(c.rgba.clone(), c.w, c.h, c.hot_x, c.hot_y) {
                    Ok(src) => {
                        self.cursors.insert(*kind, el.create_custom_cursor(src));
                    }
                    Err(e) => log::warn!("cursor {}: {e}", kind.file_name()),
                }
            }
        }
        for w in self.wins.iter_mut().flatten() {
            w.pointer = None;
        }
        self.sub_cursor = None;
        self.platform.set_cursor(None);
    }

    /// Shows the skin cursor for the control under the mouse in window `k`.
    fn update_cursor(&mut self, k: WindowKey) {
        if k == WindowKey::Library && self.model.library.carrying() {
            self.show_carry_cursor();
            return;
        }
        let input = self.input(k, false, false);
        if k == WindowKey::Library && self.show_resize_cursor((input.x, input.y)) {
            return;
        }
        let kind = self.skins[k.index()].cursor(&self.model, input.x, input.y);
        let custom = self.cursors.get(&kind).or_else(|| self.cursors.get(&CursorKind::Normal)).cloned();
        let Some(w) = self.wins[k.index()].as_mut() else { return };
        let Some(top) = w.window.top() else {
            if self.sub_cursor != Some(kind) {
                self.sub_cursor = Some(kind);
                let image = |kind| self.model.skin.cursors.iter().find(|(k, _)| **k == kind).map(|(_, image)| image);
                let image = image(kind).or_else(|| image(CursorKind::Normal)).filter(|_| self.model.settings().usecursors);
                self.platform.set_cursor(image);
            }
            return;
        };
        if w.pointer == Some(Pointer::Skin(kind)) {
            return;
        }
        w.pointer = Some(Pointer::Skin(kind));
        match custom {
            Some(c) => top.set_cursor(c),
            None => top.set_cursor(CursorIcon::Default),
        }
    }

    /// While tracks are carried out of the library, the cursor tells
    /// whether letting go puts them in the playlist. The library window
    /// holds the mouse, so its cursor shows over the others too.
    fn show_carry_cursor(&mut self) {
        let into = self.playlist_row_under_mouse().is_some() || self.model.library.dropping();
        let icon = if into { CursorIcon::Copy } else { CursorIcon::NoDrop };
        let Some(w) = self.wins[WindowKey::Library.index()].as_mut() else { return };
        let Some(top) = w.window.top() else { return };
        if w.pointer != Some(Pointer::System(icon)) {
            w.pointer = Some(Pointer::System(icon));
            top.set_cursor(icon);
        }
    }

    /// Over a border of the library that drags, the cursor shows it; true
    /// when it does.
    fn show_resize_cursor(&mut self, input: (i32, i32)) -> bool {
        let k = WindowKey::Library;
        let icon = match self.skins[k.index()].library_resizes(&self.model, input.0, input.1) {
            Some(crate::library::Resize::Across) => CursorIcon::EwResize,
            Some(crate::library::Resize::Down) => CursorIcon::NsResize,
            None => return false,
        };
        let Some(w) = self.wins[k.index()].as_mut() else { return false };
        let Some(top) = w.window.top() else { return false };
        if w.pointer != Some(Pointer::System(icon)) {
            w.pointer = Some(Pointer::System(icon));
            top.set_cursor(icon);
        }
        true
    }

    /// Starts the tooltip timer when the mouse rests on a new control.
    fn hover_tooltip(&mut self, k: WindowKey, x: i32, y: i32) {
        if k != WindowKey::Main || self.capture.is_some() {
            self.tip = None;
            return;
        }
        let text = self.skins[k.index()].tooltip(&self.model, x, y).unwrap_or_else(crate::edition::title);
        if self.tip.as_ref().is_none_or(|t| t.text != text) {
            self.tip = Some(Tooltip { text, since: Instant::now(), shown: None });
        }
    }

    /// Shows the tooltip once the mouse has rested long enough, and hides it
    /// after a while or when a menu or drag takes over.
    fn update_tooltip(&mut self, el: &ActiveEventLoop) {
        if !self.popups.is_empty() || self.drag.is_some() {
            self.tip = None;
        }
        let Some(tip) = &mut self.tip else { return };
        let age = tip.since.elapsed();
        if age >= TIP_TIMEOUT {
            tip.shown = None;
            return;
        }
        if age < TIP_DELAY || tip.shown.is_some() {
            return;
        }
        let text = tip.text.clone();
        let pct = self.dialog_pct();
        let frame = menu::tooltip(&text, &mut self.fonts.list, pct);
        let (x, y) = self.cursor_screen(WindowKey::Main);
        let attrs = platform::attributes(WindowRole::Tooltip)
            .with_title(text)
            .with_inner_size(platform::drawn_size(frame.w, frame.h, pct))
            .with_position(platform::position(x, y + 20))
            .with_window_level(WindowLevel::AlwaysOnTop);
        let Some((window, surface)) = self.overlay(el, attrs, (x, y + 20), false) else { return };
        window.request_redraw();
        if let Some(tip) = &mut self.tip {
            tip.shown = Some((window, surface));
        }
    }

    fn render_tooltip(&mut self) {
        let pct = self.dialog_pct();
        let Some(Tooltip { text, shown: Some((_, surface)), .. }) = &mut self.tip else { return };
        let frame = menu::tooltip(text, &mut self.fonts.list, pct);
        present_unscaled(surface, &frame);
    }

    fn close_popups_from(&mut self, level: usize) {
        self.popups.truncate(level);
    }

    fn popup_hover(&mut self, el: &ActiveEventLoop, level: usize, hover: Option<usize>) {
        let Some(p) = self.popups.get_mut(level) else { return };
        if p.hover == hover {
            return;
        }
        p.hover = hover;
        let sub = hover.and_then(|h| {
            (!p.items[h].sub.is_empty()).then(|| {
                let at = |n: i32| platform::from_drawn(crate::pen::screen(p.layout.pct, n), p.layout.pct);
                (p.items[h].sub.clone(), p.pos.0 + at(p.layout.w - 3), p.pos.1 + at(p.layout.rows[h].0 - 3))
            })
        });
        self.close_popups_from(level + 1);
        self.render_popup(level);
        if let Some((items, x, y)) = sub {
            self.open_popup(el, items, (x, y), true);
        }
    }

    fn popup_activate(&mut self, el: &ActiveEventLoop, level: usize, item: usize) {
        let Some(it) = self.popups.get(level).and_then(|p| p.items.get(item)).cloned() else { return };
        if !it.sub.is_empty() {
            self.popups[level].hover = None;
            self.popup_hover(el, level, Some(item));
            return;
        }
        if let Some(action) = it.action {
            self.popups.clear();
            self.model.on_menu(action);
            self.after_model(el);
            self.redraw_all();
        }
    }

    fn popup_key(&mut self, el: &ActiveEventLoop, code: KeyCode) {
        let level = self.popups.len() - 1;
        let p = &self.popups[level];
        let n = p.items.len();
        let enabled: Vec<usize> = (0..n).filter(|&i| p.items[i].enabled()).collect();
        let cur = p.hover.and_then(|h| enabled.iter().position(|&e| e == h));
        match code {
            KeyCode::Escape => self.popups.clear(),
            KeyCode::ArrowDown | KeyCode::ArrowUp if !enabled.is_empty() => {
                let next = match (cur, code) {
                    (None, KeyCode::ArrowDown) => 0,
                    (None, _) => enabled.len() - 1,
                    (Some(c), KeyCode::ArrowDown) => (c + 1) % enabled.len(),
                    (Some(c), _) => (c + enabled.len() - 1) % enabled.len(),
                };
                let item = enabled[next];
                self.popups[level].hover = Some(item);
                self.render_popup(level);
            }
            KeyCode::ArrowRight | KeyCode::Enter | KeyCode::Space => {
                if let Some(h) = self.popups[level].hover {
                    self.popup_activate(el, level, h);
                }
            }
            KeyCode::ArrowLeft if level > 0 => {
                self.popups.truncate(level);
            }
            _ => {}
        }
    }

    // --- docking & moving -------------------------------------------------------

    fn rect(&self, k: WindowKey) -> Option<(i32, i32, i32, i32)> {
        let w = self.win(k)?;
        if !w.visible || (k == WindowKey::Library && self.library_maxed) {
            return None;
        }
        Some((w.pos.0, w.pos.1, w.shown.px(w.size.0), w.shown.px(w.size.1)))
    }

    fn docked_group(&self, root: WindowKey) -> Vec<WindowKey> {
        let mut group = vec![root];
        let touching = |a: (i32, i32, i32, i32), b: (i32, i32, i32, i32)| {
            let h_overlap = a.0 < b.0 + b.2 && b.0 < a.0 + a.2;
            let v_overlap = a.1 < b.1 + b.3 && b.1 < a.1 + a.3;
            (h_overlap && ((a.1 + a.3 - b.1).abs() <= 1 || (b.1 + b.3 - a.1).abs() <= 1))
                || (v_overlap && ((a.0 + a.2 - b.0).abs() <= 1 || (b.0 + b.2 - a.0).abs() <= 1))
        };
        let mut changed = true;
        while changed {
            changed = false;
            for k in WindowKey::ALL {
                if group.contains(&k) {
                    continue;
                }
                let Some(r) = self.rect(k) else { continue };
                if group.iter().filter_map(|&g| self.rect(g)).any(|g| touching(g, r)) {
                    group.push(k);
                    changed = true;
                }
            }
        }
        group
    }

    /// Brings `k` up with the other shown windows under it: the window
    /// manager brings up only the window clicked, and another program's
    /// window would then cut the docked group in two. A minimized one stays
    /// minimized: bringing it up would restore it.
    fn raise_with_companions(&self, k: WindowKey) {
        let order =
            raise_order(k, |o| self.win(o).is_some_and(|w| w.visible && w.window.top().and_then(|t| t.is_minimized()) != Some(true)));
        let windows: Vec<&Window> = order.iter().filter_map(|&o| self.win(o)).filter_map(|w| w.window.top()).map(|t| &**t).collect();
        self.platform.raise_together(&windows);
    }

    fn begin_move(&mut self, k: WindowKey) {
        // The bar stays where it is; another window moves alone and
        // leaves the bar.
        let by_bar = bar::edge(&self.model).is_some();
        if by_bar && k == WindowKey::Main {
            return;
        }
        // With "Move docked windows together", a window docked to the main
        // one takes the main one along, and so the whole group.
        let whole = !by_bar && k != WindowKey::Main && self.model.settings().groupmove && self.docked_group(WindowKey::Main).contains(&k);
        if whole && self.platform.on_wayland() {
            if self.platform.drag_main() {
                self.set_aside();
            }
            return;
        }
        // Under Wayland only the compositor moves a window, and the others
        // go with the main one as parts of it.
        if let Some(main) = self.win(k).and_then(|w| w.window.top()).filter(|_| self.platform.on_wayland()) {
            if let Err(e) = main.drag_window() {
                log::debug!("move: {e}");
                return;
            }
            self.set_aside();
            return;
        }
        // A free window takes the windows docked to it along.
        let group = if whole {
            self.docked_group(WindowKey::Main)
        } else if self.placement(k) == Placement::Free && !by_bar {
            self.docked_group(k)
        } else {
            vec![k]
        };
        let start = self.cursor_screen(k);
        // Windows lets a window past the work area's edges; X11 window
        // managers hold it inside.
        // Under Wayland nobody tells where the windows are on the monitors.
        let reach = if cfg!(windows) || self.platform.on_wayland() {
            Reach::FREE
        } else {
            let rects: Vec<Rect> = group.iter().filter_map(|&g| self.rect(g)).collect();
            let monitors: Vec<Screen> = self.screens().into_iter().map(|(m, _)| m).collect();
            reach(&rects, &monitors, &self.platform.work_areas())
        };
        let group: Vec<_> = group.into_iter().filter_map(|g| self.win(g).map(|w| (g, w.pos))).collect();
        log::debug!(target: "moves", "begin {k:?}: group {group:?}, cursor {start:?}, reach {reach:?}, x1 is 100 px wide: {}", self.scale(k).px(100));
        if self.platform.on_wayland() {
            log::debug!(target: "moves", "monitors: {:?}", self.platform.wayland_monitors());
        }
        self.drag = Some(Drag::Move { held: k, group, start, reach });
    }

    /// Under Wayland, as the main window starts to be dragged: hides the
    /// windows not docked to it, which go along with it as its parts. Only
    /// where COSMIC tells where it went, which they go back by.
    fn set_aside(&mut self) {
        if !self.aside.is_empty() || self.platform.wayland_monitors().is_empty() {
            return;
        }
        let docked = self.docked_group(WindowKey::Main);
        for k in WindowKey::ALL.into_iter().filter(|k| !docked.contains(k)) {
            let scale = self.scale(k);
            let Some(w) = self.win_mut(k).filter(|w| w.visible && w.window.top().is_none()) else { continue };
            w.surface.hide((scale.px(w.size.0), scale.px(w.size.1)));
            self.aside.push(k);
        }
        log::debug!(target: "moves", "set aside {:?}", self.aside);
    }

    /// Hides the equalizer and the playlist under Wayland while the
    /// maximized library is in front (`on`), and shows them again.
    fn cover_companions(&mut self, on: bool) {
        if self.covered == on {
            return;
        }
        self.covered = on;
        for k in [WindowKey::Eq, WindowKey::Playlist] {
            let scale = self.scale(k);
            let Some(w) = self.win_mut(k).filter(|w| w.visible) else { continue };
            if on {
                w.surface.hide((scale.px(w.size.0), scale.px(w.size.1)));
            } else {
                self.render(k);
            }
        }
    }

    /// Shows the windows hidden by `set_aside` where they were on the
    /// screen, the main window having moved by `by`.
    fn bring_back(&mut self, by: (i32, i32)) {
        self.aside_until = None;
        for k in std::mem::take(&mut self.aside) {
            let Some(w) = self.win_mut(k) else { continue };
            let p = (w.pos.0 - by.0, w.pos.1 - by.1);
            w.pos = p;
            w.window.move_to(p);
            w.window.request_redraw();
            self.store_pos(k, p);
        }
    }

    /// The mouse is back in a window after the main one was dragged: COSMIC
    /// tells where it went about then, if it moved at all.
    fn mouse_back(&mut self) {
        if !self.aside.is_empty() && self.aside_until.is_none() {
            self.aside_until = Some(Instant::now() + Duration::from_millis(400));
        }
    }

    fn snap_delta(&self, group: &[(WindowKey, (i32, i32))], dx: i32, dy: i32) -> (i32, i32) {
        let s = self.model.settings();
        if !s.snap {
            return (dx, dy);
        }
        let snap = s.snaplen;
        let in_group = |k: WindowKey| group.iter().any(|(g, _)| *g == k);
        let mut targets: Vec<(i32, i32, i32, i32)> =
            WindowKey::ALL.iter().filter(|&&k| !in_group(k)).filter_map(|&k| self.rect(k)).collect();
        // Under Wayland the monitors are known in the windows' positions
        // only where the compositor tells where the main window is: the
        // one the moved window is on.
        let monitor = if self.platform.on_wayland() {
            let held = group.first().and_then(|&(k, start)| Some((self.win(k)?, start)));
            let centre = held.map(|(w, start)| (start.0 + dx + w.shown.px(w.size.0) / 2, start.1 + dy + w.shown.px(w.size.1) / 2));
            let monitors = self.platform.wayland_monitors();
            let on = |m: &&Screen| centre.is_some_and(|(x, y)| x >= m.x && x < m.x + m.w && y >= m.y && y < m.y + m.h);
            monitors.iter().find(on).or(monitors.first()).copied()
        } else {
            self.main_monitor().map(|m| platform::monitor(&m))
        };
        if let Some(Screen { x: mx, y: my, w: mw, h: mh }) = monitor {
            // Screen edges as zero-thickness rectangles around the monitor.
            targets.push((mx - 10_000, my - 10_000, 10_000 + mx + mw + 10_000 - mx, 10_000));
            targets.push((mx - 10_000, my + mh, 30_000, 10_000));
            targets.push((mx - 10_000, my - 10_000, 10_000, 30_000));
            targets.push((mx + mw, my - 10_000, 10_000, 30_000));
        }
        let (mut best_x, mut best_y) = (None::<i32>, None::<i32>);
        for (k, start) in group {
            let Some(w) = self.win(*k) else { continue };
            if !w.visible {
                continue;
            }
            let (gw, gh) = (w.shown.px(w.size.0), w.shown.px(w.size.1));
            let (gx, gy) = (start.0 + dx, start.1 + dy);
            for &(tx, ty, tw, th) in &targets {
                let v_near = gy < ty + th + snap && gy + gh > ty - snap;
                let h_near = gx < tx + tw + snap && gx + gw > tx - snap;
                if v_near {
                    for cand in [tx + tw - gx, tx - (gx + gw), tx - gx, tx + tw - (gx + gw)] {
                        if cand.abs() <= snap && best_x.is_none_or(|b| cand.abs() < b.abs()) {
                            best_x = Some(cand);
                        }
                    }
                }
                if h_near {
                    for cand in [ty + th - gy, ty - (gy + gh), ty - gy, ty + th - (gy + gh)] {
                        if cand.abs() <= snap && best_y.is_none_or(|b| cand.abs() < b.abs()) {
                            best_y = Some(cand);
                        }
                    }
                }
            }
        }
        (dx + best_x.unwrap_or(0), dy + best_y.unwrap_or(0))
    }

    fn drag_to(&mut self, k: WindowKey) {
        let cursor = self.cursor_screen(k);
        match &self.drag {
            Some(Drag::Move { group, start, reach, .. }) => {
                let (dx, dy) = (cursor.0 - start.0, cursor.1 - start.1);
                let (group, reach) = (group.clone(), *reach);
                let (sx, sy) = self.snap_delta(&group, dx, dy);
                let (sx, sy) = reach.clamp(sx, sy);
                log::debug!(target: "moves", "drag {k:?}: cursor {cursor:?} (local {:?}), by ({dx},{dy}) snapped ({sx},{sy})", self.win(k).map(|w| w.cursor));
                let (dx, dy) = (sx, sy);
                let wayland = self.platform.on_wayland();
                for (g, origin) in group {
                    let p = (origin.0 + dx, origin.1 + dy);
                    if let Some(w) = self.win_mut(g) {
                        if w.pos != p {
                            // Under Wayland the mouse is known only from
                            // the window: it stays where it is on the screen
                            // as the window goes, as when a snap moves it.
                            if wayland {
                                w.cursor = (w.cursor.0 - (p.0 - w.pos.0), w.cursor.1 - (p.1 - w.pos.1));
                            }
                            w.pos = p;
                            w.window.move_to(p);
                        }
                    }
                    self.store_pos(g, p);
                }
            }
            Some(Drag::Resize { start_size, start }) => {
                let s = self.scale(k);
                let (dx, dy) = (s.skin(cursor.0 - start.0), s.skin(cursor.1 - start.1));
                if crate::bind::resize(&mut self.model, k, *start_size, dx, dy) {
                    self.apply_layout(k);
                }
            }
            None => {}
        }
    }

    // --- input -----------------------------------------------------------------

    /// ⌘V: adds the copied files, or the copied links and paths, to the
    /// playlist, as `/ADD` would.
    #[cfg(target_os = "macos")]
    fn paste(&mut self, el: &ActiveEventLoop) {
        let (files, text) = self.platform.clipboard();
        let items = crate::mac_keys::paste_items(files, text);
        if items.is_empty() {
            return;
        }
        self.model.on_remote(vec![crate::cli::Remote::Add(items)]);
        self.after_model(el);
        self.redraw_all();
    }

    /// The modifiers held, as the Mac key table reads them.
    #[cfg(target_os = "macos")]
    fn mac_mods(&self) -> crate::mac_keys::Mods {
        let m = self.mods;
        crate::mac_keys::Mods { cmd: m.super_key(), ctrl: m.control_key(), alt: m.alt_key(), shift: m.shift_key() }
    }

    fn input(&self, k: WindowKey, right: bool, double: bool) -> Input {
        let s = self.scale(k);
        let c = self.win(k).map(|w| w.cursor).unwrap_or((0, 0));
        // Popamp's Ctrl+click is ⌘+click on a Mac.
        let ctrl = if cfg!(target_os = "macos") { self.mods.super_key() } else { self.mods.control_key() };
        Input { x: s.skin(c.0), y: s.skin(c.1), right, shift: self.mods.shift_key(), ctrl, double }
    }

    fn handle_out(&mut self, el: &ActiveEventLoop, k: WindowKey, out: Out) {
        match out {
            Out::None => {}
            Out::Redraw => self.render(k),
            Out::Capture => {
                self.capture = Some(k);
                self.render(k);
            }
            // A maximized library stays as it is.
            Out::BeginMove | Out::BeginResize if k == WindowKey::Library && self.library_maxed => {}
            Out::BeginMove => self.begin_move(k),
            Out::BeginResize => {
                let start = self.cursor_screen(k);
                self.drag = Some(Drag::Resize { start_size: crate::bind::stored_size(&self.model, k), start });
            }
            Out::Menu(items) => {
                let at = self.cursor_screen(k);
                self.capture = None;
                self.open_popup(el, items, at, false);
                self.render(k);
            }
        }
        self.after_model(el);
    }

    /// The middle button on the library sends the track under it to the playlist.
    fn library_middle(&mut self, el: &ActiveEventLoop) {
        let k = WindowKey::Library;
        let input = self.input(k, false, false);
        if self.skins[k.index()].library_middle(&mut self.model, input.x, input.y) {
            self.render(k);
            self.after_model(el);
        }
    }

    fn mouse_down(&mut self, el: &ActiveEventLoop, k: WindowKey, button: MouseButton) {
        self.tip = None;
        if !self.popups.is_empty() {
            self.popups.clear();
            return;
        }
        if let Some(click) = match button {
            MouseButton::Right => Some(Click::Right),
            MouseButton::Middle => Some(Click::Middle),
            _ => None,
        } {
            if self.title_shortcut(el, k, click) {
                return;
            }
        }
        if button == MouseButton::Middle && k == WindowKey::Library {
            return self.library_middle(el);
        }
        if button != MouseButton::Left && button != MouseButton::Right {
            return;
        }
        // Window managers do not focus a panel on a click; a bar asks, so
        // that the keys work.
        if let Some(w) = self.win(k).filter(|w| w.role == WindowRole::Bar && !w.focused).and_then(|w| w.window.top()) {
            w.focus_window();
        }
        if self.platform.on_wayland() {
            self.set_key_focus(k);
        } else if !cfg!(target_os = "macos") {
            self.raise_with_companions(k);
        }
        let right = button == MouseButton::Right;
        let pos = self.win(k).map(|w| w.cursor).unwrap_or_default();
        let double = !right
            && self.last_click.is_some_and(|(lk, t, p)| {
                lk == k && t.elapsed() < Duration::from_millis(400) && (p.0 - pos.0).abs() < 4 && (p.1 - pos.1).abs() < 4
            });
        self.last_click = if right || double { None } else { Some((k, Instant::now(), pos)) };
        let input = self.input(k, right, double);
        let out = self.skins[k.index()].mouse_down(&mut self.model, &input);
        self.handle_out(el, k, out);
    }

    /// A right or middle click on a title bar runs what Preferences >
    /// Shortcuts says for it. Returns whether it did; the right button then
    /// opens no menu.
    fn title_shortcut(&mut self, el: &ActiveEventLoop, k: WindowKey, click: Click) -> bool {
        self.title_shortcut_times(el, k, click, 1)
    }

    /// As [`Self::title_shortcut`], running the command `times` times, for
    /// the notches of the wheel.
    fn title_shortcut_times(&mut self, el: &ActiveEventLoop, k: WindowKey, click: Click, times: i32) -> bool {
        let Some(command) = shortcuts::find(&self.model.settings().title_shortcuts, k, click) else { return false };
        let at = self.input(k, false, false);
        if !self.skins[k.index()].on_title(&self.model, at.x, at.y) {
            return false;
        }
        for _ in 0..times {
            self.model.exec(command);
        }
        self.after_model(el);
        self.redraw_all();
        true
    }

    fn mouse_up(&mut self, el: &ActiveEventLoop, k: WindowKey) {
        if let Some(drag) = self.drag.take() {
            if let Drag::Move { group, .. } = drag {
                let by_bar = bar::edge(&self.model).is_some();
                let was_off = self.off_bar;
                for (g, from) in group {
                    log::debug!(target: "moves", "end {g:?} at {:?}", self.win(g).map(|w| w.pos));
                    if let Some(p) = self.win(g).map(|w| w.pos) {
                        self.off_bar[g.index()] |= by_bar && p != from;
                        self.store_pos(g, p);
                    }
                }
                // The equalizer takes the playlist's place by the bar.
                if self.off_bar != was_off {
                    self.apply_layout(WindowKey::Main);
                }
            }
            return;
        }
        let target = self.capture.take().unwrap_or(k);
        // Let go over the player's playlist; over one of its own, the
        // library takes them.
        let over = self.model.library.carrying() && self.playlist_row_under_mouse().is_some();
        if target == WindowKey::Library && over && self.model.library.take_carried() {
            self.drop_on_playlist();
            self.update_cursor(target);
        }
        let input = self.input(target, false, false);
        let out = self.skins[target.index()].mouse_up(&mut self.model, &input);
        self.handle_out(el, target, out);
    }

    /// Tracks carried out of the library go into the playlist at the row
    /// they are let go over; anywhere else, nowhere.
    fn drop_on_playlist(&mut self) {
        let Some(row) = self.playlist_row_under_mouse() else { return };
        self.model.library_add(AddMode::Insert(row));
        self.render(WindowKey::Playlist);
    }

    /// The playlist row the mouse held by the library is over, if it is
    /// over the playlist.
    fn playlist_row_under_mouse(&self) -> Option<usize> {
        let at = self.cursor_screen(WindowKey::Library);
        let s = self.scale(WindowKey::Playlist);
        let pl = self.win(WindowKey::Playlist).filter(|w| w.visible)?;
        let (x, y) = (at.0 - pl.pos.0, at.1 - pl.pos.1);
        let inside = x >= 0 && y >= 0 && x < s.px(pl.size.0) && y < s.px(pl.size.1);
        inside.then(|| self.skins[WindowKey::Playlist.index()].row_at(&self.model, s.skin(y)).min(self.model.playlist.len()))
    }

    fn mouse_moved(&mut self, k: WindowKey) {
        if self.drag.is_some() {
            self.drag_to(k);
            return;
        }
        self.update_cursor(k);
        let target = self.capture.unwrap_or(k);
        let input = self.input(target, false, false);
        self.hover_tooltip(target, input.x, input.y);
        let changed = self.skins[target.index()].mouse_move(&mut self.model, input.x, input.y);
        if changed {
            self.render(target);
            if target == WindowKey::Eq {
                self.render(WindowKey::Main);
            }
        }
    }

    fn wheel(&mut self, el: &ActiveEventLoop, k: WindowKey, steps: i32) {
        // Over a title bar the wheel can run a command, once for each notch.
        let click = if steps > 0 { Click::ScrollUp } else { Click::ScrollDown };
        if steps != 0 && self.title_shortcut_times(el, k, click, steps.abs()) {
            return;
        }
        if steps != 0 {
            let input = self.input(k, false, false);
            self.skins[k.index()].wheel(&mut self.model, input.x, input.y, steps);
            self.render(k);
            self.render(WindowKey::Main);
        }
    }

    /// The mouse is at `at` in the menu `level`.
    fn popup_pointed(&mut self, el: &ActiveEventLoop, level: usize, at: (i32, i32)) {
        let p = &self.popups[level];
        let h = menu::hit(&p.layout, &p.items, at.0, at.1);
        if h.is_some() {
            self.popup_hover(el, level, h);
        }
    }

    /// A mouse button went up over the menu `level`.
    fn popup_released(&mut self, el: &ActiveEventLoop, level: usize) {
        if let Some(h) = self.popups[level].hover {
            self.popup_activate(el, level, h);
        }
    }

    /// Under Wayland: what happened to the windows that are parts of the
    /// main one, which winit does not know.
    #[cfg(linux_like)]
    fn pump_children(&mut self, el: &ActiveEventLoop) {
        use platform::wayland::Event as E;
        for ev in self.platform.events() {
            if let E::MainMoved(by) = ev {
                self.bring_back(by);
                continue;
            }
            let (E::Moved(id, _) | E::Left(id) | E::Button(id, ..) | E::Wheel(id, _) | E::Redraw(id)) = ev else { continue };
            let id = WinId::Sub(id);
            if let Some(level) = self.popups.iter().position(|p| p.window.id() == id) {
                match ev {
                    E::Moved(_, at) => {
                        // A menu has the desktop's arrow, not the skin's cursor.
                        if self.sub_cursor.take().is_some() {
                            self.platform.set_cursor(None);
                        }
                        let pos = self.popups[level].pos;
                        self.popup_pointed(el, level, (at.0 - pos.0, at.1 - pos.1));
                    }
                    E::Button(_, MouseButton::Left | MouseButton::Right, false) => self.popup_released(el, level),
                    E::Redraw(_) => self.render_popup(level),
                    _ => {}
                }
            } else if self.tip.as_ref().and_then(|t| t.shown.as_ref()).is_some_and(|(w, _)| w.id() == id) {
                self.render_tooltip();
            } else if let Some(k) = self.kind_of(id) {
                match ev {
                    E::Moved(_, at) => {
                        if let Some(w) = self.win_mut(k) {
                            w.cursor = (at.0 - w.pos.0, at.1 - w.pos.1);
                        }
                        self.mouse_moved(k);
                    }
                    E::Left(_) => self.tip = None,
                    E::Button(_, button, true) => self.mouse_down(el, k, button),
                    E::Button(_, MouseButton::Left, false) => self.mouse_up(el, k),
                    E::Button(..) => {}
                    E::Wheel(_, steps) => self.wheel(el, k, steps),
                    E::Redraw(_) => self.render(k),
                    E::MainMoved(_) => {}
                }
            }
        }
    }

    #[cfg(not(linux_like))]
    fn pump_children(&mut self, _el: &ActiveEventLoop) {}

    fn key(&mut self, el: &ActiveEventLoop, k: WindowKey, ev: &KeyEvent) {
        if ev.state != ElementState::Pressed {
            return;
        }
        let PhysicalKey::Code(code) = ev.physical_key else { return };
        if !self.popups.is_empty() {
            self.popup_key(el, code);
            return;
        }
        if self.skins[k.index()].popup_open() && code == KeyCode::Escape {
            self.skins[k.index()].close_popup();
            self.render(k);
            return;
        }
        #[cfg(not(target_os = "macos"))]
        let (ctrl, shift, alt) = (self.mods.control_key(), self.mods.shift_key(), self.mods.alt_key());
        use Command as C;
        use KeyCode as K;
        // A Mac's keys become the ones below, or a Mac action of their own.
        #[cfg(target_os = "macos")]
        let (code, ctrl, alt, shift) = {
            use crate::mac_keys::Mac;
            match crate::mac_keys::translate(code, self.mac_mods(), k) {
                Mac::Key { code, ctrl, alt, shift } => (code, ctrl, alt, shift),
                Mac::Paste => return self.paste(el),
                Mac::Close => {
                    let c = match k {
                        WindowKey::Main => C::Exit,
                        WindowKey::Eq => C::ToggleEqWindow,
                        WindowKey::Playlist => C::TogglePlaylistWindow,
                        WindowKey::Library => C::ToggleLibraryWindow,
                    };
                    return self.run(el, c);
                }
                Mac::Minimize => return self.run(el, C::Minimize),
                Mac::Nothing => return,
            }
        };
        // Window-specific keys first.
        if k == WindowKey::Eq && !ctrl && !alt {
            const UP: [K; 10] =
                [K::Digit1, K::Digit2, K::Digit3, K::Digit4, K::Digit5, K::Digit6, K::Digit7, K::Digit8, K::Digit9, K::Digit0];
            const DOWN: [K; 10] = [K::KeyQ, K::KeyW, K::KeyE, K::KeyR, K::KeyT, K::KeyY, K::KeyU, K::KeyI, K::KeyO, K::KeyP];
            if let Some(b) = UP.iter().position(|&c| c == code) {
                let v = self.model.settings().eq_bands()[b] as i32 - 1;
                self.model.set_eq_band(b, v);
                return self.redraw_after(el, k);
            }
            if let Some(b) = DOWN.iter().position(|&c| c == code) {
                let v = self.model.settings().eq_bands()[b] as i32 + 1;
                self.model.set_eq_band(b, v);
                return self.redraw_after(el, k);
            }
            match code {
                K::Backquote => {
                    let v = self.model.settings().preamp - 1;
                    self.model.set_preamp(v);
                    return self.redraw_after(el, k);
                }
                K::Tab => {
                    let v = self.model.settings().preamp + 1;
                    self.model.set_preamp(v);
                    return self.redraw_after(el, k);
                }
                K::KeyN => return self.run(el, C::ToggleEq),
                K::KeyA => return self.run(el, C::ToggleEqAutoload),
                K::KeyS => {
                    let items = self.model.eq_presets_menu();
                    let at = self.cursor_screen(k);
                    self.open_popup(el, items, at, false);
                    return;
                }
                _ => {}
            }
        }
        if k == WindowKey::Eq && ctrl && alt && code == K::KeyS {
            return self.run(el, C::EqPresetLoad);
        }
        if k == WindowKey::Playlist {
            let nav = match code {
                K::ArrowUp if !alt => Some(Nav::Up),
                K::ArrowDown if !alt => Some(Nav::Down),
                K::PageUp => Some(Nav::PageUp),
                K::PageDown => Some(Nav::PageDown),
                K::Home => Some(Nav::Home),
                K::End => Some(Nav::End),
                _ => None,
            };
            if let Some(n) = nav {
                self.skins[WindowKey::Playlist.index()].key_nav(&mut self.model, n, shift);
                return self.redraw_after(el, k);
            }
            let cmd = match code {
                K::ArrowUp if alt => Some(C::MoveSelectionUp),
                K::ArrowDown if alt => Some(C::MoveSelectionDown),
                K::Enter | K::NumpadEnter => Some(C::PlaySelected),
                K::Delete if ctrl && shift => Some(C::ClearPlaylist),
                K::Delete if ctrl => Some(C::PlCropSelected),
                K::Delete if alt => Some(C::RemoveMissing),
                K::Delete => Some(C::PlRemoveSelected),
                K::KeyA if ctrl && alt => Some(C::ToggleAlwaysOnTop),
                K::KeyA if ctrl => Some(C::SelectAll),
                K::KeyI if ctrl => Some(C::InvertSelection),
                K::KeyN if ctrl => Some(C::ClearPlaylist),
                K::KeyO if ctrl => Some(C::PlaylistOpen),
                K::KeyS if ctrl => Some(C::PlaylistSave),
                K::KeyE if ctrl => Some(C::EditPlaylistEntry),
                K::KeyW if ctrl => Some(C::TogglePlShade),
                K::KeyR if ctrl && shift => Some(C::RandomizeList),
                K::KeyR if ctrl => Some(C::ReverseList),
                K::Digit1 if ctrl && shift => Some(C::SortByTitle),
                K::Digit2 if ctrl && shift => Some(C::SortByFileName),
                K::Digit3 if ctrl && shift => Some(C::SortByPath),
                K::KeyL if ctrl => Some(C::PlAddUrl),
                K::KeyL if shift => Some(C::PlAddFolder),
                K::KeyL if !alt => Some(C::PlAddFiles),
                K::Insert => Some(C::PlAddFolder),
                _ => None,
            };
            if let Some(c) = cmd {
                return self.run(el, c);
            }
        }
        // On a Mac ⌥ types accents and symbols (⌥E é): into the search,
        // never to a shortcut.
        #[cfg(target_os = "macos")]
        let typing = crate::mac_keys::types_text(alt, ctrl, ev.text.as_deref());
        #[cfg(not(target_os = "macos"))]
        let typing = false;
        if k == WindowKey::Library && (!alt || typing) {
            let used = match (ctrl, code) {
                (true, K::KeyA) => {
                    self.model.library.select_all();
                    true
                }
                (true, _) => false,
                (false, _) => form_key(ev, false).is_some_and(|key| self.skins[k.index()].library_key(&mut self.model, key, shift)),
            };
            if used || typing {
                self.after_model(el);
                return self.redraw_all();
            }
        }
        if k == WindowKey::Eq && ctrl && code == K::KeyW {
            return self.run(el, C::ToggleEqShade);
        }
        let cmd = match code {
            K::F1 if ctrl => Some(C::About),
            K::F1 => Some(C::About),
            K::KeyA if ctrl => Some(C::ToggleAlwaysOnTop),
            K::KeyW if ctrl => Some(C::ToggleWindowshade),
            K::KeyD if ctrl => Some(C::ToggleDoubleSize),
            K::KeyE if ctrl => Some(C::ToggleEasyMove),
            K::KeyT if ctrl => Some(C::ToggleTimeMode),
            K::KeyJ if ctrl => Some(C::JumpToTime),
            K::KeyH if ctrl => Some(C::History),
            K::KeyP if ctrl => Some(C::Preferences),
            K::KeyK if ctrl && shift => Some(C::ToggleVis),
            K::KeyK if ctrl || alt => Some(C::SelectVisPlugin),
            K::KeyZ if ctrl => Some(C::StartOfList),
            K::KeyB if ctrl => Some(C::EndOfList),
            K::KeyV if ctrl => Some(C::StopAfterCurrent),
            K::KeyL if ctrl => Some(C::OpenUrl),
            K::KeyW if alt => Some(C::ToggleMainWindow),
            K::KeyE if alt => Some(C::TogglePlaylistWindow),
            K::KeyG if alt => Some(C::ToggleEqWindow),
            K::KeyL if alt => Some(C::ToggleLibraryWindow),
            K::KeyS if alt => Some(C::SkinBrowser),
            K::KeyM if alt => Some(C::Minimize),
            K::Digit3 if alt => Some(C::FileInfo),
            K::KeyA if alt => Some(C::AlbumArt),
            K::KeyI if ctrl && alt => Some(C::RemoveBookmark),
            K::KeyI if alt && k == WindowKey::Playlist => Some(C::BookmarkSelected),
            K::KeyI if alt => Some(C::AddBookmark),
            K::KeyF if alt => {
                let items = self.model.main_menu();
                let at = self.cursor_screen(k);
                self.open_popup(el, items, at, false);
                return;
            }
            K::Tab if ctrl => {
                self.cycle_focus(k);
                return;
            }
            _ if ctrl || alt => None,
            K::KeyZ | K::Numpad4 => Some(C::Previous),
            K::KeyX | K::Numpad5 => Some(C::Play),
            K::KeyC => Some(C::Pause),
            K::KeyV if shift => Some(C::StopFadeout),
            K::KeyV => Some(C::Stop),
            K::KeyB | K::Numpad6 => Some(C::Next),
            K::KeyR if shift => Some(C::ToggleManualAdvance),
            K::KeyR => Some(C::ToggleRepeat),
            K::KeyS => Some(C::ToggleShuffle),
            K::KeyJ | K::NumpadDecimal => Some(C::JumpToFile),
            K::KeyL if shift => Some(C::OpenFolder),
            K::KeyL | K::Numpad0 => Some(C::OpenFile),
            K::Insert => Some(C::OpenFolder),
            K::ArrowLeft | K::Numpad7 => Some(C::Back5s),
            K::ArrowRight | K::Numpad9 => Some(C::Fwd5s),
            K::ArrowUp | K::Numpad8 => Some(C::VolumeUp),
            K::ArrowDown | K::Numpad2 => Some(C::VolumeDown),
            K::Numpad1 => Some(C::TenTracksBack),
            K::Numpad3 => Some(C::TenTracksForward),
            _ => None,
        };
        if let Some(c) = cmd {
            self.run(el, c);
        }
    }

    fn run(&mut self, el: &ActiveEventLoop, c: Command) {
        self.model.exec(c);
        self.after_model(el);
        self.redraw_all();
    }

    fn redraw_after(&mut self, el: &ActiveEventLoop, k: WindowKey) {
        self.after_model(el);
        self.render(k);
        self.render(WindowKey::Main);
    }

    fn cycle_focus(&mut self, from: WindowKey) {
        let start = from.index();
        let n = WindowKey::ALL.len();
        for i in 1..=n {
            let k = WindowKey::ALL[(start + i) % n];
            let Some(w) = self.win(k).filter(|w| w.visible) else { continue };
            match w.window.top().filter(|_| !self.platform.on_wayland()) {
                Some(top) => top.focus_window(),
                None => self.set_key_focus(k),
            }
            return;
        }
    }

    /// Under Wayland: gives the keys to window `k`, which shows as the
    /// active one while the main window has the keyboard.
    fn set_key_focus(&mut self, k: WindowKey) {
        self.key_focus = k;
        for other in WindowKey::ALL {
            let focused = self.active && other == k;
            if self.win(other).is_some_and(|w| w.focused != focused) {
                self.wins[other.index()].as_mut().unwrap().focused = focused;
                if !focused && self.skins[other.index()].popup_open() {
                    self.skins[other.index()].close_popup();
                }
                self.render(other);
            }
        }
    }

    /// Executes window-system requests queued by the model.
    fn after_model(&mut self, el: &ActiveEventLoop) {
        let reqs: Vec<UiReq> = std::mem::take(&mut self.model.ui);
        for r in reqs {
            match r {
                UiReq::Dialog(kind, req) => {
                    let events = self.model.events().clone();
                    let _ = std::thread::Builder::new().name("popamp-dialog".into()).spawn(move || {
                        let values = dialogs::show(&req).unwrap_or_else(|why| {
                            events.send(AppEvent::DialogFailed(why));
                            Vec::new()
                        });
                        events.send(AppEvent::Dialog(DialogResult { kind, values }));
                    });
                }
                UiReq::Panel(kind) => self.open_panel(el, kind),
                #[cfg(feature = "visualizers")]
                UiReq::Visualizer(id) => self.open_visualizer(el, &id),
                UiReq::Menu(items) => {
                    let at = self.cursor_screen(WindowKey::Main);
                    self.open_popup(el, items, at, false);
                }
                UiReq::Layout => {
                    if WindowKey::ALL.iter().any(|&k| self.win(k).is_some_and(|w| w.role != self.role(k))) {
                        self.remake_changed_windows(el);
                    }
                    if !self.compositor_maximizes() {
                        self.covered = false;
                    }
                    self.update_bar_area();
                    self.apply_layout(WindowKey::Main);
                    self.redraw_all();
                }
                UiReq::Minimize => self.set_minimized(true),
                UiReq::Raise => {
                    self.set_minimized(false);
                    // Each window is brought above other programs' windows,
                    // the main one last so that it has the keyboard.
                    for k in [WindowKey::Playlist, WindowKey::Eq, WindowKey::Main] {
                        if let Some(w) = self.win(k).filter(|w| w.visible).and_then(|w| w.window.top()) {
                            w.focus_window();
                        }
                    }
                }
                UiReq::Quit => {
                    self.model.save_session();
                    el.exit();
                }
                UiReq::SkinChanged => {
                    self.remake_changed_windows(el);
                    self.fonts = Fonts::for_skin(&self.model.skin);
                    self.build_cursors(el);
                    for k in WindowKey::ALL {
                        if let Some(w) = self.win_mut(k) {
                            w.shape_sig = None;
                        }
                    }
                    // A vector skin may size its windows differently.
                    self.apply_layout(WindowKey::Main);
                    self.redraw_all();
                }
                UiReq::AlwaysOnTop => {
                    let level = if self.model.settings().aot { WindowLevel::AlwaysOnTop } else { WindowLevel::Normal };
                    for k in WindowKey::ALL {
                        if let Some(w) = self.win(k).and_then(|w| w.window.top()) {
                            w.set_window_level(level);
                        }
                    }
                    self.render(WindowKey::Main);
                }
            }
        }
        for i in 0..self.panels.len() {
            if self.panels[i].panel.refresh(&self.model) {
                self.render_panel(i);
            }
        }
        self.panels.retain(|p| !p.panel.wants_close());
        if self.model.pl_dirty != self.seen_pl_dirty {
            self.seen_pl_dirty = self.model.pl_dirty;
            for i in 0..self.panels.len() {
                self.panels[i].panel.playlist_changed(&self.model);
                self.render_panel(i);
            }
            if let Some(cur) = self.model.playlist.current() {
                if self.model.status.playing_id.is_some() {
                    let _ = cur;
                }
            }
            self.render(WindowKey::Playlist);
        }
    }

    fn flush_drops(&mut self, el: &ActiveEventLoop) {
        if self.drops.is_empty() || self.drop_time.elapsed() < Duration::from_millis(120) {
            return;
        }
        let drops = std::mem::take(&mut self.drops);
        let k = drops[0].0;
        let files: Vec<String> = drops.into_iter().map(|(_, f)| f).collect();
        let picture = |f: &str| {
            let ext = std::path::Path::new(f).extension().map(|e| e.to_string_lossy().to_ascii_lowercase());
            matches!(ext.as_deref(), Some("jpg" | "jpeg" | "png" | "gif" | "bmp"))
        };
        match k {
            // A picture on a track of the library: its cover.
            WindowKey::Library if files.len() == 1 && picture(&files[0]) => {
                let input = self.input(WindowKey::Library, false, false);
                let targets = self.skins[WindowKey::Library.index()].library_cover_targets(&mut self.model, input.x, input.y);
                if targets.is_empty() {
                    self.model.flash("LIBRARY: LET THE PICTURE GO ON A TRACK OR ITS COVER");
                } else {
                    self.model.library_cover_dropped(&files[0], targets);
                }
            }
            WindowKey::Playlist => {
                // Insert at the row under the cursor.
                let input = self.input(WindowKey::Playlist, false, false);
                let at = self.skins[WindowKey::Playlist.index()].row_at(&self.model, input.y);
                self.model.add(files, AddMode::Insert(at.min(self.model.playlist.len())));
            }
            _ if self.mods.shift_key() => self.model.add(files, AddMode::Append),
            _ => self.model.add(files, AddMode::ReplaceAndPlay),
        }
        self.after_model(el);
    }

    /// Per-frame work: vis analysis, marquee, time display.
    fn tick(&mut self) {
        let sh = self.model.engine.shared().clone();
        let state = sh.state();
        if state == PlayState::Playing {
            let played = sh.played_frames();
            sh.vis.lock().read(played, self.model.vis.samples_mut());
        } else {
            self.model.vis.silence();
        }
        let rate = sh.sample_rate();
        let settings = self.model.settings().clone();
        self.model.vis.analyze(&settings, rate);
        self.skins[WindowKey::Main.index()].tick(&self.model, &mut self.fonts);
        self.render(WindowKey::Main);
        let live = self.tick_library(state);
        if self.last_slow.elapsed() >= Duration::from_millis(500) {
            self.last_slow = Instant::now();
            self.slow_tick(state, &settings, live);
        } else if settings.pe_width >= 350 && state == PlayState::Playing {
            self.render(WindowKey::Playlist);
        }
        // Here, not after each command: the engine pauses and stops on its
        // own thread, and nothing else wakes the UI to tell the desktop.
        self.media.update(self.model.media_snapshot());
        self.hotkeys.want(settings.global_hotkeys);
    }

    /// Twice a second: a changed skin, the bar's screen area, the clocks
    /// of the playlist and the library, and the library's timed scan.
    fn slow_tick(&mut self, state: PlayState, settings: &popamp_core::config::Settings, library_drawn: bool) {
        self.model.reload_skin_if_changed();
        self.model.library_rescan_if_due(Instant::now());
        if bar::edge(&self.model).is_some() && self.update_bar_area() {
            self.apply_layout(WindowKey::Main);
        }
        if state != PlayState::Stopped || settings.pe_width >= 350 {
            self.render(WindowKey::Playlist);
        }
        if state != PlayState::Stopped && !library_drawn {
            self.render(WindowKey::Library); // its clock
        }
    }

    /// Draws the library again when its strip moves: a visualizer while
    /// there is sound or its bars are still falling, a scrolling title.
    /// True when it was drawn.
    fn tick_library(&mut self, state: PlayState) -> bool {
        let k = WindowKey::Library;
        let scrolled = self.skins[k.index()].tick(&self.model, &mut self.fonts);
        let vis = self.skins[k.index()].shows_vis(&self.model) && (state == PlayState::Playing || !self.model.vis.at_rest());
        if scrolled || vis {
            self.render(k);
        }
        scrolled || vis
    }
}

/// Shows `frame` pixel for pixel: menus, tooltips and dialogs are drawn at
/// the screen's resolution.
fn present_unscaled(surface: &mut Surface, frame: &Bitmap) {
    surface.present((frame.w, frame.h), |buf| Scale::ONE.present(frame, buf, frame.w as usize, frame.h as usize));
}

/// The key for a dialog form; `None` for keys it ignores and for
/// shortcuts with Ctrl or Alt.
fn form_key(ev: &KeyEvent, with_modifier: bool) -> Option<widgets::Key> {
    use widgets::Key as F;
    Some(match &ev.logical_key {
        Key::Named(n) => match n {
            NamedKey::Enter => F::Enter,
            NamedKey::Escape => F::Escape,
            NamedKey::Tab => F::Tab,
            NamedKey::Backspace => F::Backspace,
            NamedKey::Delete => F::Delete,
            NamedKey::ArrowLeft => F::Left,
            NamedKey::ArrowRight => F::Right,
            NamedKey::ArrowUp => F::Up,
            NamedKey::ArrowDown => F::Down,
            NamedKey::Home => F::Home,
            NamedKey::End => F::End,
            NamedKey::PageUp => F::PageUp,
            NamedKey::PageDown => F::PageDown,
            NamedKey::Space if !with_modifier => F::Char(' '),
            _ => return None,
        },
        _ if with_modifier => return None,
        _ => F::Char(ev.text.as_ref()?.chars().next()?),
    })
}

/// The application logo as the window icon (taskbar, alt-tab).
fn window_icon() -> Option<winit::window::Icon> {
    static PNG: &[u8] = include_bytes!("../../../assets/logo/png/popamp-64.png");
    let img = image::load_from_memory(PNG).ok()?.to_rgba8();
    let (w, h) = img.dimensions();
    winit::window::Icon::from_rgba(img.into_raw(), w, h).ok()
}

impl ApplicationHandler<AppEvent> for App {
    fn new_events(&mut self, el: &ActiveEventLoop, cause: StartCause) {
        self.pump_children(el);
        // A shaped window looks where the cursor is 30 times a second, on a
        // clock of its own, so the windows are not drawn more often for it.
        let tracking = self.platform.tracks_cursor();
        if tracking && Instant::now() >= self.next_poll {
            self.next_poll = Instant::now() + Duration::from_millis(33);
            let windows: Vec<&Window> =
                WindowKey::ALL.iter().filter_map(|&k| self.win(k)).filter_map(|w| w.window.top()).map(|t| &**t).collect();
            self.platform.track_cursor(&windows, self.drag.is_some() || self.capture.is_some());
        }
        let frame_due = Instant::now() >= self.next_frame || (!tracking && matches!(cause, StartCause::ResumeTimeReached { .. }));
        if matches!(cause, StartCause::Poll) || frame_due {
            let s = self.model.settings();
            let fps = if self.model.state() == PlayState::Playing && s.sa != 0 { s.vis_fps.max(1) } else { 6 };
            self.next_frame = Instant::now() + Duration::from_millis(1000 / fps as u64);
            self.tick();
            self.update_tooltip(el);
            self.flush_drops(el);
            if let Some((dir, frames)) = &mut self.snapshot {
                *frames += 1;
                if *frames == 8 {
                    let dir = dir.clone();
                    let _ = std::fs::create_dir_all(&dir);
                    for k in WindowKey::ALL {
                        if let Some(w) = self.win(k).filter(|w| w.visible) {
                            let name = format!("{k:?}.png").to_lowercase();
                            if let Err(e) = w.frame.save_png(&dir.join(name)) {
                                log::error!("snapshot: {e}");
                            }
                        }
                    }
                    self.model.save_session();
                    el.exit();
                }
            }
            if self.aside_until.is_some_and(|t| Instant::now() >= t) {
                self.bring_back((0, 0));
            }
            if let Some(t) = self.popup_focus_check {
                if t.elapsed() > Duration::from_millis(150) {
                    self.popup_focus_check = None;
                    let any_focused = WindowKey::ALL.iter().any(|&k| self.win(k).is_some_and(|w| w.focused));
                    if !any_focused {
                        self.popups.clear();
                    }
                }
            }
        }
        let wake = if self.platform.tracks_cursor() { self.next_frame.min(self.next_poll) } else { self.next_frame };
        el.set_control_flow(ControlFlow::WaitUntil(wake));
    }

    fn about_to_wait(&mut self, el: &ActiveEventLoop) {
        self.pump_children(el);
    }

    /// Under X11 a window being dragged follows the mouse itself, not the
    /// window's mouse events: winit drops one where the mouse has kept its
    /// place in the window, as it does when the window has just caught up
    /// with it, and the window would stay a step behind.
    fn device_event(&mut self, _el: &ActiveEventLoop, _id: DeviceId, ev: DeviceEvent) {
        if let (true, DeviceEvent::MouseMotion { .. }, Some(&Drag::Move { held, .. })) = (cfg!(linux_like), ev, self.drag.as_ref()) {
            self.drag_to(held);
        }
    }

    fn resumed(&mut self, el: &ActiveEventLoop) {
        if self.wins[0].is_none() {
            self.create_windows(el);
            self.after_model(el);
        }
    }

    fn user_event(&mut self, el: &ActiveEventLoop, ev: AppEvent) {
        match ev {
            AppEvent::Engine(e) => {
                self.model.on_engine(e);
                if let Some(cur) = self.model.playlist.current() {
                    self.skins[WindowKey::Playlist.index()].ensure_visible(&self.model, cur);
                }
                self.render(WindowKey::Main);
                self.render(WindowKey::Playlist);
                self.render(WindowKey::Library);
            }
            AppEvent::Meta(id, m) => self.model.on_meta(id, m),
            AppEvent::Expanded { items, mode } => self.model.on_expanded(items, mode),
            AppEvent::Scan(ev) => self.model.on_scan(ev),
            AppEvent::Update(found) => {
                self.model.on_update(found);
                self.render(WindowKey::Main);
            }
            AppEvent::Library(ev) => {
                self.model.on_library(ev);
                self.render(WindowKey::Library);
            }
            AppEvent::Dialog(r) if matches!(r.kind, crate::dialogs::DialogKind::PickCover { .. }) => {
                if let (Some(i), Some(f)) = (self.panels.iter().position(|p| p.panel.kind() == PanelKind::FileInfo), r.values.first()) {
                    self.panels[i].panel.cover_picked(std::path::Path::new(f));
                    self.render_panel(i);
                }
            }
            AppEvent::Dialog(r) => self.model.on_dialog(r),
            AppEvent::DialogFailed(why) => {
                log::warn!("no dialog: {why}");
                self.model.flash(why.short());
            }
            #[cfg(feature = "plugins")]
            AppEvent::Plugins(n, list, effects) => self.model.on_plugins(n, list, effects),
            #[cfg(feature = "plugins")]
            AppEvent::PluginStopped(id, why) => self.model.on_plugin_stopped(&id, &why),
            #[cfg(feature = "visualizers")]
            AppEvent::VisualizerStopped(id, why) => {
                self.close_visualizer();
                self.model.on_plugin_stopped(&id, &why);
            }
            AppEvent::Museum(f) => {
                if let Some(i) = self.panels.iter().position(|p| p.panel.kind() == PanelKind::SkinMuseum) {
                    self.panels[i].panel.fetched(f, &mut self.model);
                    self.panel_changed(i);
                }
            }
            AppEvent::Gallery(f) => {
                if let Some(i) = self.panels.iter().position(|p| p.panel.kind() == PanelKind::Gallery) {
                    self.panels[i].panel.gallery_fetched(f, &mut self.model);
                    self.panel_changed(i);
                }
            }
            AppEvent::Remote(req) => {
                let reply = self.model.on_remote(req.remotes);
                let _ = req.reply.send(reply);
            }
        }
        self.after_model(el);
    }

    fn window_event(&mut self, el: &ActiveEventLoop, id: WindowId, ev: WindowEvent) {
        #[cfg(feature = "visualizers")]
        if self.visualizer.as_ref().is_some_and(|v| v.window.id() == id) {
            if !self.visualizer.as_ref().is_some_and(|v| v.event(ev)) {
                self.close_visualizer();
            }
            return;
        }
        if let Some(i) = self.panels.iter().position(|p| p.window.id() == id) {
            return self.panel_event(el, i, ev);
        }
        let id = WinId::Top(id);
        if let Some(level) = self.popups.iter().position(|p| p.window.id() == id) {
            match ev {
                WindowEvent::CursorMoved { position, .. } => self.popup_pointed(el, level, (position.x as i32, position.y as i32)),
                WindowEvent::MouseInput { state: ElementState::Released, button: MouseButton::Left | MouseButton::Right, .. } => {
                    self.popup_released(el, level)
                }
                WindowEvent::RedrawRequested => self.render_popup(level),
                _ => {}
            }
            return;
        }
        if self.tip.as_ref().and_then(|t| t.shown.as_ref()).is_some_and(|(w, _)| w.id() == id) {
            if let WindowEvent::RedrawRequested = ev {
                self.render_tooltip();
            }
            return;
        }
        let Some(k) = self.kind_of(id) else { return };
        match ev {
            WindowEvent::Resized(size) if k == WindowKey::Library && self.compositor_maximizes() => {
                let area = Screen { x: 0, y: 0, w: size.width as i32, h: size.height as i32 };
                if self.model.library_area != Some(area) && area.w > 0 && area.h > 0 {
                    self.model.library_area = Some(area);
                    self.apply_layout(WindowKey::Library);
                    self.render(WindowKey::Library);
                }
            }
            WindowEvent::CursorLeft { .. } => self.tip = None,
            // As after the main window is dragged under Wayland.
            WindowEvent::CursorEntered { .. } => self.mouse_back(),
            WindowEvent::CloseRequested => self.run(el, Command::Exit),
            WindowEvent::RedrawRequested => {
                if let Some(w) = self.win_mut(k) {
                    w.surface.ready();
                }
                self.render(k)
            }
            WindowEvent::ScaleFactorChanged { scale_factor, mut inner_size_writer } if self.platform.on_wayland() => {
                // The window keeps its size in screen pixels, and the
                // windows that are parts of it go where that puts them.
                let (scale, (w, h)) = (self.scale(k), self.logical_size(k));
                let _ = inner_size_writer.request_inner_size(PhysicalSize::new(scale.px(w) as u32, scale.px(h) as u32));
                self.platform.set_scale(scale_factor);
                for w in self.wins.iter().flatten() {
                    w.window.move_to(w.pos);
                    w.window.request_redraw();
                }
            }
            WindowEvent::Focused(f) if self.platform.on_wayland() => {
                // Hidden while the maximized library has the focus, back
                // when the main window has it; a dialog changes nothing.
                match (k, f) {
                    (WindowKey::Library, true) => self.cover_companions(self.compositor_maximizes()),
                    (WindowKey::Main, true) => self.cover_companions(false),
                    _ => {}
                }
                self.active = f;
                if !f && !self.popups.is_empty() {
                    self.popup_focus_check = Some(Instant::now());
                }
                self.set_key_focus(self.key_focus);
            }
            // Moved to a screen with other pixels per point: the window keeps
            // its size in points and is drawn again at the new density.
            WindowEvent::ScaleFactorChanged { .. } if platform::DESKTOP_IN_POINTS => self.render(k),
            WindowEvent::Focused(f) => {
                if let Some(w) = self.win_mut(k) {
                    w.focused = f;
                }
                if f && k == WindowKey::Main {
                    // Back from the Dock: so are the companions.
                    self.show_hidden_companions();
                }
                // On macOS raising does not move the focus. X11 window
                // managers may focus a window brought up, which would come
                // back here for ever: there the click raises (`mouse_down`).
                if f && cfg!(target_os = "macos") {
                    self.raise_with_companions(k);
                }
                if !f && !self.popups.is_empty() {
                    self.popup_focus_check = Some(Instant::now());
                }
                if !f && self.skins[k.index()].popup_open() {
                    self.skins[k.index()].close_popup();
                }
                self.render(k);
            }
            WindowEvent::Moved(p) => {
                let factor = self.win(k).map_or(1.0, |w| w.window.scale_factor());
                let p = {
                    let (x, y) = platform::from_screen(p.x as f64, p.y as f64, factor);
                    PhysicalPosition::new(x, y)
                };
                // Windows moves a window out of a work area that shrank,
                // the bar out of the strip just reserved for it: it goes back.
                if let Some(w) = self.win(k).filter(|w| w.role == WindowRole::Bar && w.strip.is_some() && w.pos != (p.x, p.y)) {
                    log::debug!(target: "moves", "{k:?} moved to ({},{}) by the system; back to {:?}", p.x, p.y, w.pos);
                    w.window.move_to(w.pos);
                    return;
                }
                log::debug!(target: "moves", "{k:?} moved to ({},{}); we had {:?}, dragging {}", p.x, p.y, self.win(k).map(|w| w.pos), self.drag.is_some());
                // Always trust the WM: it may hold a window back (at a panel,
                // say), and the pointer's screen position is counted from
                // where the window really is.
                if let Some(w) = self.win_mut(k) {
                    w.pos = (p.x, p.y);
                }
                self.store_pos(k, (p.x, p.y));
            }
            WindowEvent::ModifiersChanged(m) => self.mods = m.state(),
            WindowEvent::CursorMoved { position, .. } => {
                if let Some(w) = self.win_mut(k) {
                    w.cursor = platform::from_screen(position.x, position.y, w.window.scale_factor());
                }
                self.mouse_moved(k);
            }
            WindowEvent::MouseInput { state, button, .. } => {
                // Ctrl+click is a right click on a Mac, pressed and released.
                #[cfg(target_os = "macos")]
                let button = crate::mac_keys::button(button, self.mac_mods());
                match state {
                    ElementState::Pressed => self.mouse_down(el, k, button),
                    ElementState::Released if button == MouseButton::Left => self.mouse_up(el, k),
                    _ => {}
                }
            }
            WindowEvent::MouseWheel { delta, .. } => self.wheel(
                el,
                k,
                match delta {
                    MouseScrollDelta::LineDelta(_, y) => y.round() as i32,
                    MouseScrollDelta::PixelDelta(p) => (p.y / 20.0).round() as i32,
                },
            ),
            WindowEvent::KeyboardInput { event, .. } => {
                let k = if self.platform.on_wayland() { self.key_focus } else { k };
                self.key(el, k, &event)
            }
            WindowEvent::DroppedFile(path) => {
                self.drops.push((k, path.to_string_lossy().into_owned()));
                self.drop_time = Instant::now();
            }
            _ => {}
        }
    }

    fn exiting(&mut self, _el: &ActiveEventLoop) {
        if !self.model.quit {
            self.model.save_session();
        }
    }
}

/// Where a menu `size` big opened at `at` goes: below and right of it,
/// else moved onto `monitor`. With `flip` it goes above or left of `at`
/// instead, so that it does not cover the point clicked: a second click
/// there would pick an item.
fn menu_origin(at: (i32, i32), size: (i32, i32), monitor: Screen, flip: bool) -> (i32, i32) {
    let place = |at: i32, len: i32, start: i32, room: i32| {
        if at + len <= start + room {
            at
        } else if flip && at - len >= start {
            at - len
        } else {
            (start + room - len).max(start)
        }
    };
    (place(at.0, size.0, monitor.x, monitor.w), place(at.1, size.1, monitor.y, monitor.h))
}

/// The pixels of a window `size` skin pixels big that `shape` shows.
fn shape_mask(model: &Model, mode: &Mode, shape: &Shape, size: (i32, i32), scale: Scale) -> Option<Mask> {
    let (w, h, skin) = (size.0, size.1, &model.skin);
    match shape {
        Shape::Polygons(polys) => Some(Mask { w, h, on: region_mask(polys, w, h), scale }),
        Shape::Picture(Paint { picture: Picture::Svg(id), src, .. }) => {
            let (pw, ph) = (scale.px(w), scale.px(h));
            let r = skin.svgs.raster(*id, *src, pw, ph)?;
            Some(Mask { w: pw, h: ph, on: r.px.iter().map(|p| p >> 24 >= 128).collect(), scale: Scale::ONE })
        }
        Shape::Picture(_) => None,
        Shape::Pictures => Some(crate::skinwin::image_mask(model, mode, size, if skin.vector { scale } else { Scale::ONE }, scale)),
    }
}

/// Screen rectangle of a shown window: x, y, width, height.
type Rect = (i32, i32, i32, i32);

/// Where the maximized library goes: the usable part of the monitor with
/// the middle of its own rectangle `normal`, else of the first monitor.
fn library_area(monitors: &[Screen], work_areas: &[Screen], normal: Rect) -> Option<Screen> {
    let (cx, cy) = (normal.0 + normal.2 / 2, normal.1 + normal.3 / 2);
    let on = |m: &&Screen| cx >= m.x && cx < m.x + m.w && cy >= m.y && cy < m.y + m.h;
    let monitor = monitors.iter().find(on).or(monitors.first())?;
    Some(bar::usable(*monitor, work_areas, None))
}

/// The screen a menu opened at `at` is kept on: the one under it where the
/// desktop counts in points (screens of other densities side by side on a
/// Mac), else the main window's, as Popamp does elsewhere (under Wayland
/// none: a window there does not know where it is).
fn menu_screen(points: bool, screens: &[(Screen, f64)], at: (i32, i32), main: Option<Screen>) -> Option<Screen> {
    if !points {
        return main;
    }
    screens.iter().map(|&(s, _)| s).find(|s| contains(*s, at)).or(main)
}

/// Whether `at` is on `screen`.
fn contains(screen: Screen, at: (i32, i32)) -> bool {
    at.0 >= screen.x && at.0 < screen.x + screen.w && at.1 >= screen.y && at.1 < screen.y + screen.h
}

/// The drawing percent on the screen holding `at`, from each screen's
/// pixels per desktop unit; `fallback` off every screen.
fn pct_on(screens: &[(Screen, f64)], at: (i32, i32), fallback: i32) -> i32 {
    screens.iter().find(|(s, _)| contains(*s, at)).map_or(fallback, |&(_, f)| ((f * 100.0).round() as i32).max(100))
}

/// The pixels of a window `size` skin pixels big at `layout` in the
/// desktop's units, on a screen with `factor` pixels to each: the window's
/// own size, rounded once, so the drawing reaches every edge.
fn surface_size(layout: Scale, size: (i32, i32), factor: f64) -> (u32, u32) {
    let px = |n: i32| ((layout.px(n) as f64) * factor).round().max(1.0) as u32;
    (px(size.0), px(size.1))
}

/// The windows to bring up when `focused` is clicked, back to front: the
/// other visible ones, then `focused` itself on top.
fn raise_order(focused: WindowKey, visible: impl Fn(WindowKey) -> bool) -> Vec<WindowKey> {
    let others = WindowKey::ALL.into_iter().filter(|&k| k != focused && visible(k));
    others.chain([focused]).collect()
}

/// How far a group of windows may go from where a drag started: X11
/// window managers hold each window inside the work area, and a group that
/// went on would slide apart, the held windows staying behind.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Reach {
    /// The most negative and the most positive step on (x, y).
    lo: (i32, i32),
    hi: (i32, i32),
}

impl Reach {
    const FREE: Reach = Reach { lo: (i32::MIN, i32::MIN), hi: (i32::MAX, i32::MAX) };

    fn clamp(&self, dx: i32, dy: i32) -> (i32, i32) {
        (dx.max(self.lo.0).min(self.hi.0), dy.max(self.lo.1).min(self.hi.1))
    }
}

/// The [`Reach`] of a group from its windows' screen rectangles at the
/// start. An edge holds a window that starts inside it, and only where no
/// monitor lies beyond that edge.
fn reach(group: &[Rect], monitors: &[Screen], work_areas: &[Screen]) -> Reach {
    let mut r = Reach::FREE;
    for &(x, y, w, h) in group {
        let (cx, cy) = (x + w / 2, y + h / 2);
        let Some(&m) = monitors.iter().find(|m| cx >= m.x && cx < m.x + m.w && cy >= m.y && cy < m.y + m.h) else { continue };
        let a = bar::usable(m, work_areas, None);
        let [left, right, up, down] = monitors_beside(monitors, m);
        let (lx, hx) = edge_reach(x, w, a.x, a.w, (left, right));
        let (ly, hy) = edge_reach(y, h, a.y, a.h, (up, down));
        r.lo = (r.lo.0.max(lx), r.lo.1.max(ly));
        r.hi = (r.hi.0.min(hx), r.hi.1.min(hy));
    }
    r
}

/// On one axis, how far a window at `pos`, `len` long, may go before and
/// after inside an area from `from`, `size` long; free where a monitor lies
/// beyond the area (`open`) or the window starts outside it.
fn edge_reach(pos: i32, len: i32, from: i32, size: i32, open: (bool, bool)) -> (i32, i32) {
    let lo = if pos >= from && !open.0 { from - pos } else { i32::MIN };
    let hi = if pos + len <= from + size && !open.1 { from + size - (pos + len) } else { i32::MAX };
    (lo, hi)
}

/// Whether another monitor lies left of, right of, above and below `m`.
fn monitors_beside(monitors: &[Screen], m: Screen) -> [bool; 4] {
    let rows = |o: &&Screen| o.y < m.y + m.h && m.y < o.y + o.h;
    let columns = |o: &&Screen| o.x < m.x + m.w && m.x < o.x + o.w;
    [
        monitors.iter().filter(rows).any(|o| o.x + o.w <= m.x),
        monitors.iter().filter(rows).any(|o| o.x >= m.x + m.w),
        monitors.iter().filter(columns).any(|o| o.y + o.h <= m.y),
        monitors.iter().filter(columns).any(|o| o.y >= m.y + m.h),
    ]
}

/// New positions after windows change size. `anchor`, then the first shown
/// window of every other docked group, stays put; a window docked to a placed
/// one goes back against the same edge, its offset along that edge passed
/// through `rescale`.
fn follow_docked(old: &[Option<Rect>], sizes: &[(i32, i32)], anchor: usize, rescale: impl Fn(i32) -> i32) -> Vec<(i32, i32)> {
    let mut pos: Vec<Option<(i32, i32)>> = vec![None; old.len()];
    for root in std::iter::once(anchor).chain(0..old.len()) {
        let Some(r) = old[root].filter(|_| pos[root].is_none()) else { continue };
        pos[root] = Some((r.0, r.1));
        let mut todo = vec![root];
        while let Some(a) = todo.pop() {
            let (Some(ra), Some(pa)) = (old[a], pos[a]) else { continue };
            let now = (pa.0, pa.1, sizes[a].0, sizes[a].1);
            for b in 0..old.len() {
                let Some(rb) = old[b].filter(|_| pos[b].is_none()) else { continue };
                pos[b] = reattach(ra, rb, now, sizes[b], &rescale);
                if pos[b].is_some() {
                    todo.push(b);
                }
            }
        }
    }
    pos.into_iter().map(|p| p.unwrap_or((0, 0))).collect()
}

/// Where the windows go: sizes in screen pixels and top-left corners, in
/// [`WindowKey::ALL`] order.
struct Plan {
    sizes: Vec<(i32, i32)>,
    pos: Vec<(i32, i32)>,
}

/// The length of a resizable side that wants to be `want` skin pixels:
/// `want` itself, unless the side would end past `limit` further out than
/// it ended before (`was`); then the longest allowed length that ends
/// inside `limit`, or the shortest allowed one. `end` gives the screen
/// position a length ends at.
fn fit_grown(dim: Dim, want: i32, was: i32, end: impl Fn(i32) -> i32, limit: i32) -> i32 {
    let Dim::Free { min, step } = dim else { return want };
    let mut len = want;
    while step > 0 && len - step >= min && end(len) > limit && end(len) > was {
        len -= step;
    }
    len
}

/// Where `b`, which touched `a` before, goes now that `a` is at `now` and
/// `b` has `size`; `None` if they did not touch.
fn reattach(a: Rect, b: Rect, now: Rect, size: (i32, i32), rescale: &impl Fn(i32) -> i32) -> Option<(i32, i32)> {
    let columns_overlap = b.0 < a.0 + a.2 && b.0 + b.2 > a.0;
    let rows_overlap = b.1 < a.1 + a.3 && b.1 + b.3 > a.1;
    let touch = |gap: i32| gap.abs() <= 1;
    let x = now.0 + rescale(b.0 - a.0);
    let y = now.1 + rescale(b.1 - a.1);
    let sides = [
        (columns_overlap && touch(b.1 - (a.1 + a.3)), (x, now.1 + now.3)),  // below
        (columns_overlap && touch(a.1 - (b.1 + b.3)), (x, now.1 - size.1)), // above
        (rows_overlap && touch(b.0 - (a.0 + a.2)), (now.0 + now.2, y)),     // right
        (rows_overlap && touch(a.0 - (b.0 + b.2)), (now.0 - size.0, y)),    // left
    ];
    sides.into_iter().find(|s| s.0).map(|s| s.1)
}

#[cfg(test)]
mod tests {
    use super::*;

    const MONITOR: [Screen; 1] = [Screen { x: 0, y: 0, w: 1920, h: 1080 }];
    const BELOW_PANEL: [Screen; 1] = [Screen { x: 0, y: 32, w: 1920, h: 1048 }];

    #[test]
    fn the_maximized_library_fills_its_own_monitor_less_the_panels() {
        let two = [Screen { x: 0, y: 0, w: 1920, h: 1080 }, Screen { x: 1920, y: 0, w: 1280, h: 1024 }];
        let work = [Screen { x: 0, y: 32, w: 1920, h: 1048 }, Screen { x: 1920, y: 0, w: 1280, h: 1024 }];
        assert_eq!(library_area(&two, &work, (100, 100, 550, 348)), Some(work[0]));
        assert_eq!(library_area(&two, &work, (1800, 100, 550, 348)), Some(work[1]), "where most of it is: its middle");
        assert_eq!(library_area(&two, &work, (-900, 100, 550, 348)), Some(work[0]), "off every monitor: the first");
        assert_eq!(library_area(&two, &[], (100, 100, 550, 348)), Some(two[0]), "no work areas known: the monitor");
        assert_eq!(library_area(&[], &work, (100, 100, 550, 348)), None);
    }

    #[test]
    fn a_docked_group_stops_where_the_window_manager_holds_one_of_it() {
        // Main against the panel and the right edge, the equalizer and the
        // playlist under it, the playlist slid 53 px to the left.
        let group = [(1645, 32, 275, 116), (1645, 148, 275, 116), (1592, 264, 275, 232)];
        let r = reach(&group, &MONITOR, &BELOW_PANEL);
        assert_eq!((r.lo, r.hi), ((-1592, 0), (0, 1080 - 496)));
        assert_eq!(r.clamp(200, -100), (0, 0), "held at the right edge and the panel");
        assert_eq!(r.clamp(-300, 50), (-300, 50), "free to go left and down");
        assert_eq!(reach(&group, &MONITOR, &[]).lo.1, -32, "no work area: the monitor's top");
    }

    #[test]
    fn a_group_goes_freely_to_a_monitor_beside_it_or_from_beyond_an_edge() {
        let side_by_side = [Screen { x: 0, y: 0, w: 1920, h: 1080 }, Screen { x: 1920, y: 0, w: 1920, h: 1080 }];
        let r = reach(&[(1645, 100, 275, 116)], &side_by_side, &side_by_side);
        assert_eq!(r.hi.0, i32::MAX);
        let stacked = [Screen { x: 0, y: 0, w: 1920, h: 1080 }, Screen { x: 0, y: 1080, w: 1920, h: 1080 }];
        assert_eq!(reach(&[(100, 1200, 275, 116)], &stacked, &stacked).lo.1, i32::MIN);
        assert_eq!(reach(&[(100, -50, 275, 116)], &MONITOR, &MONITOR).lo.1, i32::MIN, "already above: left alone");
    }

    #[test]
    fn a_menu_is_drawn_at_the_density_of_the_screen_it_opens_on() {
        let retina = (Screen { x: 0, y: 0, w: 1728, h: 1117 }, 2.0);
        let plain = (Screen { x: 1728, y: 0, w: 2560, h: 1440 }, 1.0);
        assert_eq!(pct_on(&[retina, plain], (2000, 300), 150), 100);
        assert_eq!(pct_on(&[retina, plain], (100, 300), 150), 200);
        assert_eq!(pct_on(&[retina, plain], (-50, -50), 150), 150, "off every screen: the fallback");
    }

    #[test]
    fn the_drawing_fills_the_window_to_the_last_pixel() {
        // x1.5 on Retina: 275 skin pixels are 413 points, 826 pixels.
        assert_eq!(surface_size(Scale::new(150, popamp_core::config::Scaling::Sharp), (275, 116), 2.0), (826, 348));
        assert_eq!(surface_size(Scale::new(200, popamp_core::config::Scaling::Sharp), (275, 116), 1.0), (550, 232));
    }

    #[test]
    fn a_menu_stays_on_the_screen_under_it_only_in_points() {
        let left = Screen { x: 0, y: 0, w: 1512, h: 982 };
        let right = Screen { x: 1512, y: 0, w: 2560, h: 1440 };
        let screens = [(left, 2.0), (right, 1.0)];
        assert_eq!(menu_screen(true, &screens, (2000, 50), Some(left)), Some(right));
        assert_eq!(menu_screen(true, &screens, (-50, 50), Some(left)), Some(left), "off every screen: the main one");
        assert_eq!(menu_screen(false, &screens, (2000, 50), Some(left)), Some(left), "elsewhere: the main window's");
        assert_eq!(menu_screen(false, &screens, (2000, 50), None), None, "Wayland: none");
    }

    #[test]
    fn the_window_clicked_comes_up_last_with_its_visible_companions_under_it() {
        let shown = |k: WindowKey| k != WindowKey::Eq;
        assert_eq!(raise_order(WindowKey::Main, shown), [WindowKey::Playlist, WindowKey::Library, WindowKey::Main]);
        assert_eq!(raise_order(WindowKey::Playlist, |_| true), [WindowKey::Main, WindowKey::Eq, WindowKey::Library, WindowKey::Playlist]);
    }

    fn x15(d: i32) -> i32 {
        d * 3 / 2
    }

    #[test]
    fn menus_open_beside_the_click_and_stay_on_the_monitor() {
        let m = Screen { x: 0, y: 0, w: 1920, h: 1080 };
        assert_eq!(menu_origin((100, 100), (200, 300), m, true), (100, 100));
        assert_eq!(menu_origin((11, 1066), (200, 300), m, true), (11, 766), "above a bottom bar's button, not over it");
        assert_eq!(menu_origin((1900, 100), (200, 300), m, true), (1700, 100));
        assert_eq!(menu_origin((100, 500), (200, 1200), m, true), (100, 0), "too tall either way: from the top");
        assert_eq!(menu_origin((300, 1000), (200, 300), m, false), (300, 780), "a submenu only moves onto the monitor");
    }

    #[test]
    fn a_docked_chain_moves_down_as_a_whole() {
        // Main, EQ and playlist stacked at x1, then all grown to x1.5.
        let old = [Some((100, 100, 275, 116)), Some((100, 216, 275, 116)), Some((100, 332, 275, 232))];
        let sizes = [(413, 174), (413, 174), (413, 348)];
        assert_eq!(follow_docked(&old, &sizes, 0, x15), [(100, 100), (100, 274), (100, 448)]);
    }

    #[test]
    fn a_window_on_the_left_moves_left_as_it_grows() {
        // Playlist left of main, EQ under main; x1 to x1.5 and back.
        let old = [Some((500, 0, 275, 116)), Some((500, 116, 275, 116)), Some((225, 0, 275, 232))];
        let sizes = [(413, 174), (413, 174), (413, 348)];
        let grown = follow_docked(&old, &sizes, 0, x15);
        assert_eq!(grown, [(500, 0), (500, 174), (87, 0)]);
        let old = [Some((500, 0, 413, 174)), Some((500, 174, 413, 174)), Some((87, 0, 413, 348))];
        let sizes = [(275, 116), (275, 116), (275, 232)];
        assert_eq!(follow_docked(&old, &sizes, 0, |d| d * 2 / 3), [(500, 0), (500, 116), (225, 0)]);
    }

    #[test]
    fn offsets_along_an_edge_scale_with_the_windows() {
        // EQ right of main and 20 pixels lower.
        let old = [Some((0, 0, 275, 116)), Some((275, 20, 275, 116)), None];
        let sizes = [(413, 174), (413, 174), (0, 0)];
        assert_eq!(follow_docked(&old, &sizes, 0, x15), [(0, 0), (413, 30), (0, 0)]);
    }

    #[test]
    fn shading_main_pulls_the_window_below_up_and_leaves_the_side_one() {
        let old = [Some((500, 0, 275, 116)), Some((500, 116, 275, 116)), Some((225, 0, 275, 232))];
        let sizes = [(275, 14), (275, 116), (275, 232)];
        assert_eq!(follow_docked(&old, &sizes, 0, |d| d), [(500, 0), (500, 14), (225, 0)]);
    }

    #[test]
    fn the_resized_window_stays_and_pushes_its_neighbour() {
        // Playlist left of main grows 25 wider: main moves right, not the playlist left.
        let old = [Some((500, 0, 275, 116)), None, Some((225, 0, 275, 232))];
        let sizes = [(275, 116), (0, 0), (300, 232)];
        assert_eq!(follow_docked(&old, &sizes, 2, |d| d), [(525, 0), (0, 0), (225, 0)]);
    }

    #[test]
    fn a_side_that_grows_past_the_screen_stops_at_the_last_step_inside() {
        let dim = Dim::Free { min: 116, step: 29 };
        // A playlist at y 700 on a screen 1000 high, x1.5.
        let end = |n: i32| 700 + x15(n);
        assert_eq!(fit_grown(dim, 232, 700 + x15(174), end, 1000), 174, "203 would end at 1004");
        assert_eq!(fit_grown(dim, 174, 700 + x15(145), end, 1000), 174, "what fits is left alone");
        // Bigger windows: a playlist that fitted at x1 is cut at x1.5.
        assert_eq!(fit_grown(dim, 290, 700 + 290, end, 1000), 174);
        assert_eq!(fit_grown(dim, 290, 700 + 290, |n| 990 + x15(n), 1000), 116, "never under the shortest");
    }

    #[test]
    fn a_side_already_past_the_screen_is_not_cut_until_it_grows() {
        let dim = Dim::Free { min: 116, step: 29 };
        let end = |n: i32| 900 + n;
        assert_eq!(fit_grown(dim, 232, 900 + 232, end, 1000), 232, "parked over the edge by the user");
        assert_eq!(fit_grown(dim, 261, 900 + 232, end, 1000), 232, "growing further is refused");
        assert_eq!(fit_grown(Dim::Fixed(275), 275, 0, end, 1000), 275);
    }

    #[test]
    fn detached_windows_stay_put() {
        let old = [Some((0, 0, 275, 116)), None, Some((500, 500, 275, 232))];
        let sizes = [(413, 174), (0, 0), (413, 348)];
        assert_eq!(follow_docked(&old, &sizes, 0, x15), [(0, 0), (0, 0), (500, 500)]);
    }
}
