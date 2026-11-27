//! Wayland: a program cannot place its windows there, so only the main
//! window is a window (winit's). The equalizer, the playlist, menus and
//! tooltips are popups of it (xdg_popup), which a program does place, each
//! against the main window. winit knows nothing of them: they are made,
//! drawn and given the mouse here, on winit's connection.
//!
//! They are popups rather than subsurfaces because a compositor may show a
//! window's subsurfaces only within the window's outline: COSMIC cuts every
//! floating window to its rounded rectangle, and the equalizer and playlist
//! were then not seen at all, the menus only where they lay over the main
//! window. A popup is drawn apart from the window. Subsurfaces remain where
//! the compositor has no popups to give, or with `POPAMP_WAYLAND_SUBSURFACES=1`.
//!
//! Positions are the same "screen" pixels the rest of the program counts
//! in. The main window never moves in them, since nobody tells where the
//! compositor has it; a subsurface sits where its position is from the
//! main window's.

use super::{shape_rects, Mask};
use crate::bar::Screen;
use popamp_skin::cursors::CursorImage;
use std::cell::{Cell, RefCell};
use std::collections::{HashMap, HashSet, VecDeque};
use std::os::fd::AsFd;
use std::rc::Rc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use wayland_backend::client::{Backend, ObjectId};
use wayland_client::globals::{registry_queue_init, GlobalListContents};
use wayland_client::protocol::wl_buffer::{self, WlBuffer};
use wayland_client::protocol::wl_callback::{self, WlCallback};
use wayland_client::protocol::wl_compositor::WlCompositor;
use wayland_client::protocol::wl_output::WlOutput;
use wayland_client::protocol::wl_pointer::{self, WlPointer};
use wayland_client::protocol::wl_region::WlRegion;
use wayland_client::protocol::wl_registry::{self, WlRegistry};
use wayland_client::protocol::wl_seat::WlSeat;
use wayland_client::protocol::wl_shm::{self, WlShm};
use wayland_client::protocol::wl_shm_pool::WlShmPool;
use wayland_client::protocol::wl_subcompositor::WlSubcompositor;
use wayland_client::protocol::wl_subsurface::WlSubsurface;
use wayland_client::protocol::wl_surface::WlSurface;
use wayland_client::{delegate_noop, event_created_child, Connection, Dispatch, EventQueue, Proxy, QueueHandle, WEnum};
use wayland_protocols::ext::foreign_toplevel_list::v1::client::ext_foreign_toplevel_handle_v1::{self, ExtForeignToplevelHandleV1};
use wayland_protocols::ext::foreign_toplevel_list::v1::client::ext_foreign_toplevel_list_v1::{self, ExtForeignToplevelListV1};
use wayland_protocols::wp::relative_pointer::zv1::client::zwp_relative_pointer_manager_v1::ZwpRelativePointerManagerV1;
use wayland_protocols::wp::relative_pointer::zv1::client::zwp_relative_pointer_v1::{self, ZwpRelativePointerV1};
use wayland_protocols::wp::viewporter::client::wp_viewport::WpViewport;
use wayland_protocols::wp::viewporter::client::wp_viewporter::WpViewporter;
use wayland_protocols::xdg::shell::client::xdg_popup::{self, XdgPopup};
use wayland_protocols::xdg::shell::client::xdg_positioner::{self, XdgPositioner};
use wayland_protocols::xdg::shell::client::xdg_surface::{self, XdgSurface};
use wayland_protocols::xdg::shell::client::xdg_toplevel::XdgToplevel;
use wayland_protocols::xdg::shell::client::xdg_wm_base::{self, XdgWmBase};
use wayland_protocols::xdg::xdg_output::zv1::client::zxdg_output_manager_v1::ZxdgOutputManagerV1;
use wayland_protocols::xdg::xdg_output::zv1::client::zxdg_output_v1::{self, ZxdgOutputV1};
use wayland_protocols_wlr::layer_shell::v1::client::zwlr_layer_shell_v1::{self, ZwlrLayerShellV1};
use wayland_protocols_wlr::layer_shell::v1::client::zwlr_layer_surface_v1::{self, ZwlrLayerSurfaceV1};
use winit::event::MouseButton;
use winit::platform::wayland::WindowExtWayland as _;
use winit::raw_window_handle::{HasDisplayHandle, HasWindowHandle, RawDisplayHandle, RawWindowHandle};
use winit::window::Window;

/// COSMIC's word on where each window is: its toplevel-info protocol,
/// cut to what is read here (protocols/).
#[allow(dead_code, non_upper_case_globals, non_camel_case_types, unused_imports, missing_docs, clippy::all)]
mod cosmic {
    use wayland_client;
    use wayland_client::protocol::*;
    use wayland_protocols::ext::foreign_toplevel_list::v1::client::*;
    pub mod __interfaces {
        use wayland_client::protocol::__interfaces::*;
        use wayland_protocols::ext::foreign_toplevel_list::v1::client::__interfaces::*;
        wayland_scanner::generate_interfaces!("protocols/cosmic-toplevel-info-unstable-v1.xml");
    }
    use self::__interfaces::*;
    wayland_scanner::generate_client_code!("protocols/cosmic-toplevel-info-unstable-v1.xml");
}
use cosmic::zcosmic_toplevel_handle_v1::{self, ZcosmicToplevelHandleV1};
use cosmic::zcosmic_toplevel_info_v1::{self, ZcosmicToplevelInfoV1};

/// What happened to a subsurface, named by its `Sub::id`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Event {
    /// The mouse is at this point, in screen pixels.
    Moved(u32, (i32, i32)),
    Left(u32),
    /// A mouse button went down (`true`) or up.
    Button(u32, MouseButton, bool),
    /// The wheel turned by this many steps, away from the user if positive.
    Wheel(u32, i32),
    /// It is to be drawn again.
    Redraw(u32),
    /// COSMIC moved the main window by this much, in screen pixels.
    MainMoved((i32, i32)),
}

type Point = (i32, i32);

/// Where the compositor has a subsurface, which runs behind where it was
/// sent: the mouse is reported from the place the compositor knows.
#[derive(Default)]
struct Place {
    /// The position the compositor has confirmed.
    known: Cell<Point>,
    /// Positions sent and not confirmed yet, oldest first.
    sent: RefCell<VecDeque<Point>>,
}

/// A monitor's place among the others and its size, as far as told.
type Output = (Option<Point>, Option<Point>);

/// What the main window and its subsurfaces share.
struct Shared {
    /// The main window's position.
    origin: Cell<Point>,
    /// Screen pixels in one of the compositor's units.
    scale: Cell<f64>,
    places: RefCell<HashMap<u32, Rc<Place>>>,
    redraws: RefCell<Vec<u32>>,
    /// Popups: the configure each is to acknowledge before it is next shown.
    configures: RefCell<HashMap<u32, u32>>,
    /// Popups the compositor took off the screen, to be made again.
    dismissed: RefCell<HashSet<u32>>,
    /// Where the compositor is to put each popup once it acknowledges the
    /// configure: until then it is still where it was, and the mouse is
    /// reported from there.
    placed: RefCell<HashMap<u32, Point>>,
    /// Positions asked for with `reposition` and not yet answered, by
    /// popup, with the token each was sent with.
    asked: RefCell<HashMap<u32, Vec<(u32, Point)>>>,
    /// The bar's width as the compositor gives it, in its units: the
    /// monitor's less the panels beside it.
    bar_width: Cell<Option<i32>>,
    /// The monitors by their wl_output: where each is among the others and
    /// how big, in the compositor's units, as far as it has told.
    outputs: RefCell<HashMap<ObjectId, Output>>,
    /// Where COSMIC has the main window: its corner from that of each
    /// monitor it is on, in the compositor's units.
    main_at: RefCell<HashMap<ObjectId, Point>>,
    /// The main window's corner among the monitors when COSMIC last told,
    /// in its units.
    main_corner: Cell<Option<Point>>,
    /// The size of the room for the windows on the main window's monitor,
    /// in the compositor's units, as a `Probe` finds it, and the sides of
    /// the panels.
    room: RefCell<Option<(ObjectId, Point, [bool; 4])>>,
    /// The probe out, to be unmade once it has told.
    probe: RefCell<Option<(WlSurface, ZwlrLayerSurfaceV1)>>,
    /// The main window moved: the room is to be found again from it.
    reprobe: Cell<bool>,
}

impl Shared {
    /// A length in screen pixels in the compositor's units.
    fn units(&self, px: i32) -> i32 {
        (px as f64 / self.scale.get()).round() as i32
    }
}

/// The picture the mouse has over the subsurfaces.
struct Pointer {
    surface: WlSurface,
    hot: Point,
    /// The buffer of a skin's cursor; the theme owns the arrow's.
    _own: Option<Buf>,
}

/// A button held over a subsurface: which one, and where the mouse is.
struct Held {
    id: u32,
    at: (f64, f64),
    /// The compositor told how far the mouse moved since the press. Until it
    /// does, the mouse is followed where the compositor reports it: an
    /// absolute device, as a graphics tablet or a virtual machine's mouse,
    /// moves the pointer without any such movement.
    moved: bool,
}

struct State {
    shared: Rc<Shared>,
    events: Vec<Event>,
    /// The subsurface under the mouse and the serial it entered with.
    over: Option<(u32, u32)>,
    pointer: Option<Pointer>,
    /// Wheel movement not yet a whole step.
    wheel: f64,
    /// Where the mouse last was, in screen pixels.
    last: (f64, f64),
    /// While a button is held over a subsurface, where the mouse is, from
    /// how far it moved rather than from where the subsurface is: one being
    /// dragged is not where the compositor reports the mouse from until a
    /// while after it moves, and the mouse read from it would run ahead of
    /// itself. `None` too where the compositor tells no such movement.
    held: Option<Held>,
    /// The compositor tells how far the mouse moves.
    relative: bool,
    /// The serial of the last press of a button over a subsurface, which
    /// lets the main window be moved from it.
    pressed: Option<u32>,
    /// The windows on the screen, to find the main one among.
    toplevels: HashMap<ObjectId, Toplevel>,
    /// What tells where a window is (COSMIC).
    info: Option<ZcosmicToplevelInfoV1>,
    /// The main window among `toplevels`, and what tells where it is.
    followed: Option<(ObjectId, ZcosmicToplevelHandleV1)>,
    /// The main window's title, which tells it from the dialogs.
    title: String,
    /// The monitors, by the name of their global.
    outputs: HashMap<u32, WlOutput>,
    xdg_outputs: Option<ZxdgOutputManagerV1>,
}

/// A window on the screen, anyone's.
struct Toplevel {
    handle: ExtForeignToplevelHandleV1,
    title: String,
    app_id: String,
}

impl State {
    /// Asks where the main window is, once it is among the windows.
    fn follow_main(&mut self, qh: &QueueHandle<State>) {
        let Some(info) = self.info.as_ref().filter(|_| self.followed.is_none()) else { return };
        let Some(main) = self.toplevels.values().find(|t| t.app_id == crate::flatpak::desktop_id() && t.title == self.title) else {
            return;
        };
        log::debug!(target: "moves", "following the main window, {}", main.handle.id());
        self.followed = Some((main.handle.id(), info.get_cosmic_toplevel(&main.handle, qh, ())));
    }

    /// Learns where the monitor of global `name` is and how big.
    fn add_output(&mut self, registry: &WlRegistry, name: u32, version: u32, qh: &QueueHandle<State>) {
        let Some(manager) = &self.xdg_outputs else { return };
        let output: WlOutput = registry.bind(name, version.min(4), qh, ());
        manager.get_xdg_output(&output, qh, output.id());
        self.shared.outputs.borrow_mut().insert(output.id(), (None, None));
        self.outputs.insert(name, output);
    }
}

delegate_noop!(State: WlCompositor);
delegate_noop!(State: WlSubcompositor);
delegate_noop!(State: WlSubsurface);
delegate_noop!(State: WlRegion);
delegate_noop!(State: WlShmPool);
delegate_noop!(State: WpViewporter);
delegate_noop!(State: WpViewport);
delegate_noop!(State: ignore WlSurface);
delegate_noop!(State: ignore WlShm);
delegate_noop!(State: ignore WlSeat);
delegate_noop!(State: XdgPositioner);
delegate_noop!(State: ZwpRelativePointerManagerV1);
delegate_noop!(State: ZwlrLayerShellV1);
delegate_noop!(State: ZxdgOutputManagerV1);
delegate_noop!(State: ignore WlOutput);

/// Where a monitor is among the others, and how big, in the compositor's units.
impl Dispatch<ZxdgOutputV1, ObjectId> for State {
    fn event(state: &mut State, _: &ZxdgOutputV1, event: zxdg_output_v1::Event, output: &ObjectId, _: &Connection, _: &QueueHandle<State>) {
        let mut outputs = state.shared.outputs.borrow_mut();
        let Some((pos, size)) = outputs.get_mut(output) else { return };
        match event {
            zxdg_output_v1::Event::LogicalPosition { x, y } => *pos = Some((x, y)),
            zxdg_output_v1::Event::LogicalSize { width, height } => *size = Some((width, height)),
            _ => {}
        }
    }
}

/// A window came on the screen.
impl Dispatch<ExtForeignToplevelListV1, ()> for State {
    fn event(
        state: &mut State,
        _: &ExtForeignToplevelListV1,
        event: ext_foreign_toplevel_list_v1::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<State>,
    ) {
        if let ext_foreign_toplevel_list_v1::Event::Toplevel { toplevel } = event {
            state.toplevels.insert(toplevel.id(), Toplevel { handle: toplevel, title: String::new(), app_id: String::new() });
        }
    }

    event_created_child!(State, ExtForeignToplevelListV1, [
        ext_foreign_toplevel_list_v1::EVT_TOPLEVEL_OPCODE => (ExtForeignToplevelHandleV1, ()),
    ]);
}

/// What a window is called; the main one is followed once known.
impl Dispatch<ExtForeignToplevelHandleV1, ()> for State {
    fn event(
        state: &mut State,
        handle: &ExtForeignToplevelHandleV1,
        event: ext_foreign_toplevel_handle_v1::Event,
        _: &(),
        _: &Connection,
        qh: &QueueHandle<State>,
    ) {
        use ext_foreign_toplevel_handle_v1::Event as E;
        let id = handle.id();
        match event {
            E::Title { title } => {
                if let Some(t) = state.toplevels.get_mut(&id) {
                    t.title = title;
                }
            }
            E::AppId { app_id } => {
                if let Some(t) = state.toplevels.get_mut(&id) {
                    t.app_id = app_id;
                }
            }
            E::Done => state.follow_main(qh),
            E::Closed => {
                state.toplevels.remove(&id);
                handle.destroy();
                // The main window made again is another one.
                if state.followed.as_ref().is_some_and(|(f, _)| *f == id) {
                    if let Some((_, cosmic)) = state.followed.take() {
                        cosmic.destroy();
                    }
                    state.shared.main_at.borrow_mut().clear();
                    state.shared.main_corner.set(None);
                    state.follow_main(qh);
                }
            }
            _ => {}
        }
    }
}

impl Dispatch<ZcosmicToplevelInfoV1, ()> for State {
    fn event(_: &mut State, _: &ZcosmicToplevelInfoV1, _: zcosmic_toplevel_info_v1::Event, _: &(), _: &Connection, _: &QueueHandle<State>) {
    }

    event_created_child!(State, ZcosmicToplevelInfoV1, [
        zcosmic_toplevel_info_v1::EVT_TOPLEVEL_OPCODE => (ZcosmicToplevelHandleV1, ()),
    ]);
}

/// Where the main window is on each monitor it is on.
impl Dispatch<ZcosmicToplevelHandleV1, ()> for State {
    fn event(
        state: &mut State,
        _: &ZcosmicToplevelHandleV1,
        event: zcosmic_toplevel_handle_v1::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<State>,
    ) {
        let shared = state.shared.clone();
        let mut main_at = shared.main_at.borrow_mut();
        match event {
            zcosmic_toplevel_handle_v1::Event::Geometry { output, x, y, .. } => {
                log::debug!(target: "moves", "main window at {x},{y} on {}", output.id());
                main_at.insert(output.id(), (x, y));
                // How far it went, which the windows apart from it may go back by.
                let Some(pos) = shared.outputs.borrow().get(&output.id()).and_then(|o| o.0) else { return };
                let corner = (pos.0 + x, pos.1 + y);
                if shared.main_corner.get() != Some(corner) {
                    shared.reprobe.set(true);
                }
                if let Some(old) = shared.main_corner.replace(Some(corner)).filter(|&old| old != corner) {
                    let px = |v: i32| (v as f64 * shared.scale.get()).round() as i32;
                    state.events.push(Event::MainMoved((px(corner.0 - old.0), px(corner.1 - old.1))));
                }
            }
            zcosmic_toplevel_handle_v1::Event::OutputLeave { output } => {
                main_at.remove(&output.id());
            }
            _ => {}
        }
    }
}

/// The bar may be shown, after it acknowledges this; it is drawn again for that.
impl Dispatch<ZwlrLayerSurfaceV1, u32> for State {
    fn event(
        state: &mut State,
        _: &ZwlrLayerSurfaceV1,
        event: zwlr_layer_surface_v1::Event,
        id: &u32,
        _: &Connection,
        _: &QueueHandle<State>,
    ) {
        match event {
            zwlr_layer_surface_v1::Event::Configure { serial, width, height } => {
                log::debug!("Wayland: bar configured {width}x{height}");
                state.shared.configures.borrow_mut().insert(*id, serial);
                state.shared.redraws.borrow_mut().push(*id);
                if width > 0 {
                    state.shared.bar_width.set(Some(width as i32));
                }
            }
            zwlr_layer_surface_v1::Event::Closed => log::warn!("Wayland: the compositor took the bar away"),
            _ => {}
        }
    }
}

/// The mouse moved by this much, wherever its pointer is drawn.
impl Dispatch<ZwpRelativePointerV1, ()> for State {
    fn event(
        state: &mut State,
        _: &ZwpRelativePointerV1,
        event: zwp_relative_pointer_v1::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<State>,
    ) {
        if let zwp_relative_pointer_v1::Event::RelativeMotion { dx, dy, .. } = event {
            let scale = state.shared.scale.get();
            if let Some(held) = &mut state.held {
                held.moved = true;
                held.at.0 += dx * scale;
                held.at.1 += dy * scale;
                let p = (held.at.0.round() as i32, held.at.1.round() as i32);
                state.events.push(Event::Moved(held.id, p));
            }
        }
    }
}

/// The compositor asks whether the program still answers.
impl Dispatch<XdgWmBase, ()> for State {
    fn event(_: &mut State, wm: &XdgWmBase, event: xdg_wm_base::Event, _: &(), _: &Connection, _: &QueueHandle<State>) {
        if let xdg_wm_base::Event::Ping { serial } = event {
            wm.pong(serial);
        }
    }
}

/// A popup may be shown, after it acknowledges this; it is drawn again for that.
impl Dispatch<XdgSurface, u32> for State {
    fn event(state: &mut State, _: &XdgSurface, event: xdg_surface::Event, id: &u32, _: &Connection, _: &QueueHandle<State>) {
        if let xdg_surface::Event::Configure { serial } = event {
            state.shared.configures.borrow_mut().insert(*id, serial);
            state.shared.redraws.borrow_mut().push(*id);
        }
    }
}

/// Where the compositor puts a popup, from the main window's corner.
impl Dispatch<XdgPopup, u32> for State {
    fn event(state: &mut State, _: &XdgPopup, event: xdg_popup::Event, id: &u32, _: &Connection, _: &QueueHandle<State>) {
        let shared = &state.shared;
        match event {
            xdg_popup::Event::Configure { x, y, .. } => {
                // The popup goes where it was asked to be, as it may not be
                // moved to fit the screen; the compositor's figures are
                // not used, since they need not count from the same corner.
                log::debug!(target: "moves", "popup {id} configured at {x},{y}, asked {:?}", shared.placed.borrow().get(id));
            }
            xdg_popup::Event::Repositioned { token } => {
                let mut asked = shared.asked.borrow_mut();
                let list = asked.entry(*id).or_default();
                if let Some(i) = list.iter().position(|&(t, _)| t == token) {
                    shared.placed.borrow_mut().insert(*id, list[i].1);
                    list.drain(..=i);
                }
            }
            xdg_popup::Event::PopupDone => {
                log::debug!("Wayland: popup {id} dismissed");
                shared.dismissed.borrow_mut().insert(*id);
                shared.redraws.borrow_mut().push(*id);
            }
            _ => {}
        }
    }
}

/// A layer of the desktop never shown, across the whole of a monitor but
/// for its panels: the size the compositor gives it is the room left for
/// the windows, as for one maximized.
struct Probe(ObjectId, [bool; 4]);

impl Dispatch<ZwlrLayerSurfaceV1, Probe> for State {
    fn event(
        state: &mut State,
        _: &ZwlrLayerSurfaceV1,
        event: zwlr_layer_surface_v1::Event,
        probe: &Probe,
        _: &Connection,
        _: &QueueHandle<State>,
    ) {
        if let zwlr_layer_surface_v1::Event::Configure { width, height, .. } = event {
            log::debug!(target: "moves", "room on {}: {width}x{height}", probe.0);
            *state.shared.room.borrow_mut() = Some((probe.0.clone(), (width as i32, height as i32), probe.1));
        }
    }
}

/// Monitors come and go.
impl Dispatch<WlRegistry, GlobalListContents> for State {
    fn event(
        state: &mut State,
        registry: &WlRegistry,
        event: wl_registry::Event,
        _: &GlobalListContents,
        _: &Connection,
        qh: &QueueHandle<State>,
    ) {
        match event {
            wl_registry::Event::Global { name, interface, version } if interface == WlOutput::interface().name => {
                state.add_output(registry, name, version, qh);
            }
            wl_registry::Event::GlobalRemove { name } => {
                if let Some(output) = state.outputs.remove(&name) {
                    state.shared.outputs.borrow_mut().remove(&output.id());
                    state.shared.main_at.borrow_mut().remove(&output.id());
                }
            }
            _ => {}
        }
    }
}

/// A buffer the compositor has let go of is free to draw into again.
impl Dispatch<WlBuffer, Arc<AtomicBool>> for State {
    fn event(_: &mut State, _: &WlBuffer, event: wl_buffer::Event, busy: &Arc<AtomicBool>, _: &Connection, _: &QueueHandle<State>) {
        if let wl_buffer::Event::Release = event {
            busy.store(false, Ordering::Relaxed);
        }
    }
}

/// The compositor has taken everything sent before this, the oldest
/// unconfirmed position of subsurface `id` with it.
impl Dispatch<WlCallback, u32> for State {
    fn event(state: &mut State, _: &WlCallback, _: wl_callback::Event, id: &u32, _: &Connection, _: &QueueHandle<State>) {
        if let Some(place) = state.shared.places.borrow().get(id) {
            if let Some(pos) = place.sent.borrow_mut().pop_front() {
                place.known.set(pos);
            }
        }
    }
}

impl Dispatch<WlPointer, ()> for State {
    fn event(state: &mut State, pointer: &WlPointer, event: wl_pointer::Event, _: &(), _: &Connection, _: &QueueHandle<State>) {
        use wl_pointer::Event as E;
        let at = |state: &mut State, id: u32, x: f64, y: f64| {
            let (shared, scale) = (&state.shared, state.shared.scale.get());
            let known = shared.places.borrow().get(&id).map_or((0, 0), |p| p.known.get());
            state.last = (known.0 as f64 + x * scale, known.1 as f64 + y * scale);
            (known.0 + (x * scale) as i32, known.1 + (y * scale) as i32)
        };
        match event {
            E::Enter { serial, surface, surface_x, surface_y } => {
                let id = surface.id().protocol_id();
                // The main window and the dialogs are winit's.
                if !state.shared.places.borrow().contains_key(&id) {
                    state.over = None;
                    return;
                }
                state.over = Some((id, serial));
                if let Some(p) = &state.pointer {
                    pointer.set_cursor(serial, Some(&p.surface), p.hot.0, p.hot.1);
                }
                let p = at(state, id, surface_x, surface_y);
                state.events.push(Event::Moved(id, p));
            }
            E::Leave { .. } => {
                state.held = None;
                if let Some((id, _)) = state.over.take() {
                    state.events.push(Event::Left(id));
                }
            }
            E::Motion { surface_x, surface_y, .. } => {
                if state.held.as_ref().is_some_and(|h| h.moved) {
                    return;
                }
                if let Some((id, _)) = state.over {
                    let p = at(state, id, surface_x, surface_y);
                    let last = state.last;
                    if let Some(held) = &mut state.held {
                        held.at = last;
                    }
                    state.events.push(Event::Moved(id, p));
                }
            }
            E::Button { button, state: pressed, serial, .. } => {
                let Some((id, _)) = state.over else { return };
                state.pressed = Some(serial);
                // The button codes of linux/input-event-codes.h.
                let button = match button {
                    0x110 => MouseButton::Left,
                    0x111 => MouseButton::Right,
                    0x112 => MouseButton::Middle,
                    _ => return,
                };
                let pressed = pressed == WEnum::Value(wl_pointer::ButtonState::Pressed);
                state.held = (pressed && state.relative).then_some(Held { id, at: state.last, moved: false });
                state.events.push(Event::Button(id, button, pressed));
            }
            E::Axis { axis: WEnum::Value(wl_pointer::Axis::VerticalScroll), value, .. } => {
                let Some((id, _)) = state.over else { return };
                // A wheel's click is 10 or 15 of these; a touchpad sends many small ones.
                state.wheel -= value / 10.0;
                let steps = state.wheel.trunc();
                if steps != 0.0 {
                    state.wheel -= steps;
                    state.events.push(Event::Wheel(id, steps as i32));
                }
            }
            _ => {}
        }
    }
}

/// A shared-memory picture a surface can show.
struct Buf {
    buffer: WlBuffer,
    pool: WlShmPool,
    map: memmap2::MmapMut,
    size: Point,
    /// The compositor is showing it.
    busy: Arc<AtomicBool>,
}

impl Buf {
    fn new(shm: &WlShm, qh: &QueueHandle<State>, (w, h): Point) -> Result<Buf, String> {
        let err = |e: std::io::Error| format!("no memory for a window picture: {e}");
        let dir = std::env::var_os("XDG_RUNTIME_DIR").map_or_else(std::env::temp_dir, Into::into);
        let path = dir.join(format!("popamp-{}-{:016x}", std::process::id(), fastrand::u64(..)));
        let file = std::fs::OpenOptions::new().read(true).write(true).create_new(true).open(&path).map_err(err)?;
        // Only the handle is needed: the compositor gets it over the socket.
        let _ = std::fs::remove_file(&path);
        let len = w as usize * h as usize * 4;
        file.set_len(len as u64).map_err(err)?;
        // SAFETY: the file is new, unnamed and ours; only the compositor
        // reads it, and it never changes its size.
        let map = unsafe { memmap2::MmapMut::map_mut(&file) }.map_err(err)?;
        let pool = shm.create_pool(file.as_fd(), len as i32, qh, ());
        let busy = Arc::new(AtomicBool::new(false));
        let buffer = pool.create_buffer(0, w, h, w * 4, wl_shm::Format::Argb8888, qh, busy.clone());
        Ok(Buf { buffer, pool, map, size: (w, h), busy })
    }

    fn pixels(&mut self) -> &mut [u32] {
        // SAFETY: a mapping starts on a page, so it is aligned for u32,
        // and its length is a whole number of pixels.
        unsafe { std::slice::from_raw_parts_mut(self.map.as_mut_ptr().cast(), self.map.len() / 4) }
    }
}

impl Drop for Buf {
    fn drop(&mut self) {
        self.buffer.destroy();
        self.pool.destroy();
    }
}

/// What a `Sub` and its `Shm` both hold of the connection.
#[derive(Clone)]
struct Link {
    conn: Connection,
    qh: QueueHandle<State>,
    compositor: WlCompositor,
    shm: WlShm,
    shared: Rc<Shared>,
}

/// The connection to the compositor, beside winit's use of it.
pub struct Wayland {
    link: Link,
    queue: EventQueue<State>,
    state: State,
    subcompositor: WlSubcompositor,
    viewporter: Option<WpViewporter>,
    /// What the other windows are popups of; `None` makes them subsurfaces.
    popups: Option<(XdgWmBase, Parent)>,
    /// What makes the bar, where the compositor has it.
    layer_shell: Option<ZwlrLayerShellV1>,
    /// The main window's surface, which is winit's.
    main: WlSurface,
    _pointer: WlPointer,
    seat: WlSeat,
    /// The main window's xdg_toplevel, which is winit's.
    toplevel: Option<XdgToplevel>,
    _relative: Option<ZwpRelativePointerV1>,
    /// What tells of the windows on the screen, for `monitors`.
    _toplevels: Option<ExtForeignToplevelListV1>,
    /// The theme the arrow comes from: it owns the arrow's picture.
    _theme: Option<wayland_cursor::CursorTheme>,
}

impl Wayland {
    /// Joins winit's connection for `main`, the main window, which is at
    /// `origin`. `None` when the window is not a Wayland one.
    pub fn join(main: &Window, origin: Point) -> Option<Result<Wayland, String>> {
        let (display, surface, toplevel) = handles(main)?;
        Some(Self::connect(display, surface, toplevel, main.scale_factor(), origin, main.title()))
    }

    /// Takes `main`, a new main window at `origin`, for the one before, which
    /// the caller keeps until this returns. The windows that were parts of
    /// the old one are to be made again.
    pub fn rejoin(&mut self, main: &Window, origin: Point) -> Result<(), String> {
        let err = |e: &dyn std::fmt::Display| format!("Wayland: {e}");
        let (_, surface, toplevel) = handles(main).ok_or("Wayland: not a Wayland window")?;
        let conn = &self.link.conn;
        // SAFETY: as in `connect`.
        let id = unsafe { ObjectId::from_ptr(WlSurface::interface(), surface.cast()) }.map_err(|e| err(&e))?;
        self.main = WlSurface::from_id(conn, id).map_err(|e| err(&e))?;
        if let Some((_, parent)) = &mut self.popups {
            *parent = Parent::Xdg(xdg_surface_of(conn, toplevel).ok_or("Wayland: the new main window has no xdg_surface")?);
        }
        self.link.shared.origin.set(origin);
        self.link.shared.scale.set(main.scale_factor());
        self.state.title = main.title();
        self.toplevel = toplevel_of(conn, toplevel);
        Ok(())
    }

    /// The bar's width in screen pixels, once the compositor has given it.
    pub fn bar_width(&self) -> Option<i32> {
        let shared = &self.link.shared;
        shared.bar_width.get().map(|w| (w as f64 * shared.scale.get()).round() as i32)
    }

    /// Whether the compositor makes bars for programs.
    pub fn has_bar(&self) -> bool {
        self.layer_shell.is_some() && self.popups.is_some()
    }

    /// The bar at `edge`, `size` screen pixels big at `pos`, and what to draw
    /// it into: a layer of the desktop across the monitor, which keeps
    /// maximized windows out of its strip. The main window is taken off the
    /// screen; the windows made from now on are popups of the bar.
    pub fn bar(&mut self, edge: popamp_core::config::BarEdge, pos: Point, size: Point) -> Option<(Sub, Shm)> {
        use zwlr_layer_surface_v1::KeyboardInteractivity;
        let shell = self.layer_shell.as_ref()?;
        let (_, parent) = self.popups.as_mut()?;
        let (link, qh) = (&self.link, &self.link.qh);
        let surface = link.compositor.create_surface(qh, ());
        let id = surface.id().protocol_id();
        link.shared.bar_width.set(None);
        let layer = shell.get_layer_surface(&surface, None, zwlr_layer_shell_v1::Layer::Top, "popamp".into(), qh, id);
        layer.set_anchor(bar_anchor(edge));
        let h = link.shared.units(size.1).max(1);
        layer.set_size(0, h as u32);
        layer.set_exclusive_zone(h);
        layer.set_keyboard_interactivity(KeyboardInteractivity::None);
        surface.commit();
        *parent = Parent::Layer(layer.clone());
        // Taken off the screen, the main window is no window to anyone; it
        // stays for the connection.
        self.main.attach(None, 0, 0);
        self.main.commit();
        let viewport = self.viewporter.as_ref().map(|v| v.get_viewport(&surface, qh, ()));
        let place = Rc::new(Place::default());
        link.shared.places.borrow_mut().insert(id, place.clone());
        let sub = Sub { link: link.clone(), surface: surface.clone(), role: Role::Layer(layer.clone()), main: self.main.clone(), place };
        sub.move_to(pos);
        let mut shm = Shm::new(link.clone(), surface, viewport, true);
        (shm.layer, shm.ready) = (Some(layer), false);
        let _ = link.conn.flush();
        Some((sub, shm))
    }

    fn connect(
        display: *mut std::ffi::c_void,
        surface: *mut std::ffi::c_void,
        toplevel: Option<*mut std::ffi::c_void>,
        scale: f64,
        origin: Point,
        title: String,
    ) -> Result<Wayland, String> {
        let err = |e: &dyn std::fmt::Display| format!("Wayland: {e}");
        // SAFETY: both pointers come from a living winit window, which the
        // caller keeps for as long as it keeps this.
        let conn = Connection::from_backend(unsafe { Backend::from_foreign_display(display.cast()) });
        let main = unsafe { ObjectId::from_ptr(WlSurface::interface(), surface.cast()) }.map_err(|e| err(&e))?;
        let main = WlSurface::from_id(&conn, main).map_err(|e| err(&e))?;
        let (globals, queue) = registry_queue_init::<State>(&conn).map_err(|e| err(&e))?;
        let qh = queue.handle();
        let compositor: WlCompositor = globals.bind(&qh, 1..=4, ()).map_err(|e| err(&e))?;
        let subcompositor: WlSubcompositor = globals.bind(&qh, 1..=1, ()).map_err(|e| err(&e))?;
        let shm: WlShm = globals.bind(&qh, 1..=1, ()).map_err(|e| err(&e))?;
        let seat: WlSeat = globals.bind(&qh, 1..=4, ()).map_err(|e| err(&e))?;
        let viewporter: Option<WpViewporter> = globals.bind(&qh, 1..=1, ()).ok();
        let popups = if std::env::var_os("POPAMP_WAYLAND_SUBSURFACES").is_some_and(|v| v == "1") {
            None
        } else {
            // SAFETY: as above; the toplevel is winit's, of the same window.
            // Popups move with `reposition`, from version 3 on.
            let parent = xdg_surface_of(&conn, toplevel);
            let wm = globals.bind::<XdgWmBase, _, _>(&qh, 3..=6, ()).ok();
            match (wm, parent) {
                (Some(wm), Some(parent)) => Some((wm, Parent::Xdg(parent))),
                (wm, parent) => {
                    log::info!("Wayland: subsurfaces, not popups (xdg_wm_base 3: {}, main window's: {})", wm.is_some(), parent.is_some());
                    None
                }
            }
        };
        let layer_shell = globals.bind::<ZwlrLayerShellV1, _, _>(&qh, 1..=4, ()).ok();
        let pointer = seat.get_pointer(&qh, ());
        let relative = globals.bind::<ZwpRelativePointerManagerV1, _, _>(&qh, 1..=1, ()).ok();
        let relative = relative.map(|m| m.get_relative_pointer(&pointer, &qh, ()));
        log::debug!(target: "moves", "relative pointer: {}", relative.is_some());
        let shared = Rc::new(Shared {
            origin: Cell::new(origin),
            scale: Cell::new(scale),
            places: Default::default(),
            redraws: Default::default(),
            configures: Default::default(),
            dismissed: Default::default(),
            placed: Default::default(),
            asked: Default::default(),
            bar_width: Default::default(),
            outputs: Default::default(),
            main_at: Default::default(),
            main_corner: Default::default(),
            room: Default::default(),
            probe: Default::default(),
            reprobe: Default::default(),
        });
        let main_toplevel = toplevel_of(&conn, toplevel);
        let link = Link { conn, qh, compositor, shm, shared: shared.clone() };
        let state = State {
            shared,
            events: Vec::new(),
            over: None,
            pointer: None,
            wheel: 0.0,
            last: (0.0, 0.0),
            held: None,
            relative: relative.is_some(),
            pressed: None,
            toplevels: HashMap::new(),
            info: None,
            followed: None,
            title,
            outputs: HashMap::new(),
            xdg_outputs: None,
        };
        let mut wayland = Wayland {
            link,
            queue,
            state,
            subcompositor,
            viewporter,
            popups,
            layer_shell,
            main,
            _pointer: pointer,
            seat,
            toplevel: main_toplevel,
            _relative: relative,
            _toplevels: None,
            _theme: None,
        };
        wayland.follow_main(&globals);
        wayland.set_cursor(None);
        Ok(wayland)
    }

    /// Learns from COSMIC where the main window is among the monitors.
    fn follow_main(&mut self, globals: &wayland_client::globals::GlobalList) {
        let (qh, state) = (&self.link.qh, &mut self.state);
        state.info = globals.bind::<ZcosmicToplevelInfoV1, _, _>(qh, 2..=2, ()).ok();
        state.xdg_outputs = globals.bind::<ZxdgOutputManagerV1, _, _>(qh, 2..=3, ()).ok();
        log::debug!(target: "moves", "where the main window is told: {}", state.info.is_some() && state.xdg_outputs.is_some());
        if state.info.is_none() || state.xdg_outputs.is_none() {
            return;
        }
        let outputs: Vec<(u32, u32)> = globals
            .contents()
            .with_list(|list| list.iter().filter(|g| g.interface == WlOutput::interface().name).map(|g| (g.name, g.version)).collect());
        for (name, version) in outputs {
            state.add_output(globals.registry(), name, version, qh);
        }
        self._toplevels = globals.bind::<ExtForeignToplevelListV1, _, _>(qh, 1..=1, ()).ok();
        let _ = self.link.conn.flush();
    }

    /// The monitors placed against the main window, in screen pixels, where
    /// the compositor tells where it is (COSMIC); none elsewhere.
    pub fn monitors(&self) -> Vec<Screen> {
        let shared = &self.link.shared;
        let (outputs, main_at) = (shared.outputs.borrow(), shared.main_at.borrow());
        // The main window's corner among the monitors.
        let main = main_at.iter().find_map(|(o, at)| {
            let pos = outputs.get(o)?.0?;
            Some((pos.0 + at.0, pos.1 + at.1))
        });
        let Some(main) = main else { return Vec::new() };
        let (origin, scale) = (shared.origin.get(), shared.scale.get());
        let px = |v: i32| (v as f64 * scale).round() as i32;
        let room = shared.room.borrow();
        outputs
            .iter()
            .filter_map(|(id, &(pos, size))| {
                let ((mut x, mut y), (mut w, mut h)) = (pos?, size?);
                // Past the panels, on the side each is.
                if let Some((_, (rw, rh), sides)) = room.as_ref().filter(|(o, ..)| o == id) {
                    let (dw, dh) = ((w - rw).max(0), (h - rh).max(0));
                    let share = |a: bool, b: bool, d: i32| {
                        if a && b {
                            d / 2
                        } else if a {
                            d
                        } else {
                            0
                        }
                    };
                    x += share(sides[0], sides[1], dw);
                    y += share(sides[2], sides[3], dh);
                    (w, h) = (w.min(*rw.max(&1)), h.min(*rh.max(&1)));
                }
                Some(Screen { x: origin.0 + px(x - main.0), y: origin.1 + px(y - main.1), w: px(w), h: px(h) })
            })
            .collect()
    }

    /// The screen pixels in one of the compositor's units changed, as when
    /// the main window goes to another monitor.
    pub fn set_scale(&self, scale: f64) {
        self.link.shared.scale.set(scale);
    }

    /// Asks the compositor how much room the panels leave for the windows
    /// on the main window's monitor: `monitors` takes it then.
    fn probe_room(&mut self) {
        let Some(shell) = &self.layer_shell else { return };
        let (link, qh) = (&self.link, &self.link.qh);
        if let Some((surface, layer)) = link.shared.probe.borrow_mut().take() {
            layer.destroy();
            surface.destroy();
        }
        let output = {
            let main_at = link.shared.main_at.borrow();
            self.state.outputs.values().find(|o| main_at.contains_key(&o.id())).cloned()
        };
        let Some(output) = output else { return };
        let surface = link.compositor.create_surface(qh, ());
        let layer = shell.get_layer_surface(
            &surface,
            Some(&output),
            zwlr_layer_shell_v1::Layer::Background,
            "popamp-probe".into(),
            qh,
            Probe(output.id(), panel_sides()),
        );
        use zwlr_layer_surface_v1::Anchor;
        layer.set_anchor(Anchor::Top | Anchor::Bottom | Anchor::Left | Anchor::Right);
        layer.set_size(0, 0);
        layer.set_exclusive_zone(0);
        surface.commit();
        *link.shared.probe.borrow_mut() = Some((surface, layer));
    }

    /// What to draw the main window into.
    pub fn main_surface(&self) -> Shm {
        Shm::new(self.link.clone(), self.main.clone(), None, false)
    }

    /// A subsurface at `pos`, and what to draw it into. Without `input`
    /// the mouse goes through it.
    pub fn sub(&self, pos: Point, input: bool) -> (Sub, Shm) {
        let (link, qh) = (&self.link, &self.link.qh);
        let surface = link.compositor.create_surface(qh, ());
        let role = match &self.popups {
            Some((wm, parent)) => Role::Popup(Rc::new(Popup {
                wm: wm.clone(),
                parent: parent.clone(),
                shown: RefCell::new(None),
                pos: Cell::new(pos),
                size: Cell::new((1, 1)),
                configured: Cell::new(false),
                token: Cell::new(0),
                moving: Cell::new(None),
            })),
            None => {
                let role = self.subcompositor.get_subsurface(&surface, &self.main, qh, ());
                // Shown when drawn, not when the main window next is.
                role.set_desync();
                Role::Sub(role)
            }
        };
        let popup = match &role {
            Role::Popup(p) => Some(p.clone()),
            Role::Sub(_) | Role::Layer(_) => None,
        };
        let viewport = self.viewporter.as_ref().map(|v| v.get_viewport(&surface, qh, ()));
        if !input {
            let nothing = link.compositor.create_region(qh, ());
            surface.set_input_region(Some(&nothing));
            nothing.destroy();
        }
        let place = Rc::new(Place::default());
        place.known.set(pos);
        let id = surface.id().protocol_id();
        if input {
            link.shared.places.borrow_mut().insert(id, place.clone());
        }
        let sub = Sub { link: link.clone(), surface: surface.clone(), role, main: self.main.clone(), place };
        sub.move_to(pos);
        let mut shm = Shm::new(link.clone(), surface, viewport, true);
        shm.popup = popup;
        (sub, shm)
    }

    /// The picture of the mouse over the subsurfaces: a skin's cursor, or
    /// the desktop's arrow.
    pub fn set_cursor(&mut self, cursor: Option<&CursorImage>) {
        let Link { conn, qh, compositor, shm, .. } = &self.link;
        let surface = self.state.pointer.take().map_or_else(|| compositor.create_surface(qh, ()), |p| p.surface);
        let mut own = None;
        let hot;
        match cursor {
            Some(c) => {
                let Ok(mut buf) = Buf::new(shm, qh, (c.w as i32, c.h as i32)) else { return };
                for (px, rgba) in buf.pixels().iter_mut().zip(c.rgba.as_chunks::<4>().0) {
                    // Colours come multiplied by their opacity.
                    let part = |v: u8| v as u32 * rgba[3] as u32 / 255;
                    *px = (rgba[3] as u32) << 24 | part(rgba[0]) << 16 | part(rgba[1]) << 8 | part(rgba[2]);
                }
                surface.attach(Some(&buf.buffer), 0, 0);
                hot = (c.hot_x as i32, c.hot_y as i32);
                own = Some(buf);
            }
            None => {
                if self._theme.is_none() {
                    self._theme = wayland_cursor::CursorTheme::load(conn, shm.clone(), 24).ok();
                }
                let arrow = self
                    ._theme
                    .as_mut()
                    .and_then(|t| ["default", "left_ptr"].into_iter().find_map(|n| t.get_cursor(n).map(|c| c[0].clone())));
                let Some(arrow) = arrow else { return };
                surface.attach(Some(&*arrow), 0, 0);
                let (x, y) = arrow.hotspot();
                hot = (x as i32, y as i32);
            }
        }
        surface.damage(0, 0, i32::MAX, i32::MAX);
        surface.commit();
        if let Some((_, serial)) = self.state.over {
            self._pointer.set_cursor(serial, Some(&surface), hot.0, hot.1);
        }
        self.state.pointer = Some(Pointer { surface, hot, _own: own });
        let _ = conn.flush();
    }

    /// What happened to the subsurfaces since the last call.
    /// Has the compositor move the main window, with the windows that are
    /// its popups, as a button pressed over one of them is held.
    pub fn drag_main(&self) -> bool {
        let (Some(toplevel), Some(serial)) = (&self.toplevel, self.state.pressed) else { return false };
        toplevel._move(&self.seat, serial);
        let _ = self.link.conn.flush();
        true
    }

    pub fn events(&mut self) -> Vec<Event> {
        if let Err(e) = self.queue.dispatch_pending(&mut self.state) {
            log::warn!("Wayland: {e}");
        }
        if self.link.shared.reprobe.take() {
            self.probe_room();
        }
        let _ = self.link.conn.flush();
        let mut events = std::mem::take(&mut self.state.events);
        events.extend(self.link.shared.redraws.borrow_mut().drain(..).map(Event::Redraw));
        events
    }
}

/// What makes a surface a part of the main window.
enum Role {
    Sub(WlSubsurface),
    Popup(Rc<Popup>),
    /// The bar, which the other windows are popups of.
    Layer(ZwlrLayerSurfaceV1),
}

/// What popups are popups of.
#[derive(Clone)]
enum Parent {
    /// The main window.
    Xdg(XdgSurface),
    /// The bar.
    Layer(ZwlrLayerSurfaceV1),
}

/// A popup of the main window or the bar, which its `Sub` places and its
/// `Shm` shows.
/// It is made when it is first shown and unmade when it is hidden: a popup
/// once taken off the screen is not shown again.
struct Popup {
    wm: XdgWmBase,
    parent: Parent,
    shown: RefCell<Option<(XdgSurface, XdgPopup)>>,
    /// Where it is to be, in screen pixels.
    pos: Cell<Point>,
    /// Its size in the compositor's units.
    size: Cell<Point>,
    /// It has acknowledged a configure since it was made.
    configured: Cell<bool>,
    /// The last `reposition`'s token.
    token: Cell<u32>,
    /// Where the next commit puts it.
    moving: Cell<Option<Point>>,
}

impl Popup {
    /// Where the popup goes: `pos` from the main window's corner, never
    /// moved to fit the screen, which would take it off the windows it is
    /// docked to.
    fn positioner(&self, link: &Link) -> XdgPositioner {
        let Link { qh, shared, .. } = link;
        let p = self.wm.create_positioner(qh, ());
        let ((w, h), pos, origin) = (self.size.get(), self.pos.get(), shared.origin.get());
        p.set_size(w.max(1), h.max(1));
        p.set_anchor_rect(0, 0, 1, 1);
        p.set_anchor(xdg_positioner::Anchor::TopLeft);
        p.set_gravity(xdg_positioner::Gravity::BottomRight);
        p.set_constraint_adjustment(xdg_positioner::ConstraintAdjustment::empty());
        p.set_offset(shared.units(pos.0 - origin.0), shared.units(pos.1 - origin.1));
        p
    }

    /// Asks for the popup to be where `pos` and `size` say now.
    fn reposition(&self, link: &Link, id: u32) {
        if let Some((_, popup)) = &*self.shown.borrow() {
            let p = self.positioner(link);
            let token = self.token.get().wrapping_add(1);
            self.token.set(token);
            link.shared.asked.borrow_mut().entry(id).or_default().push((token, self.pos.get()));
            popup.reposition(&p, token);
            p.destroy();
        }
    }

    /// Whether `surface` may be shown, `size` units big. The first time
    /// it makes the popup, which may be shown once the compositor answers.
    fn ready(&self, link: &Link, surface: &WlSurface, size: Point) -> bool {
        let id = surface.id().protocol_id();
        if link.shared.dismissed.borrow_mut().remove(&id) {
            self.hide(link, surface);
        }
        if self.shown.borrow().is_none() {
            self.size.set(size);
            let xdg = self.wm.get_xdg_surface(surface, &link.qh, id);
            let p = self.positioner(link);
            let popup = match &self.parent {
                Parent::Xdg(parent) => xdg.get_popup(Some(parent), &p, &link.qh, id),
                Parent::Layer(bar) => {
                    let popup = xdg.get_popup(None, &p, &link.qh, id);
                    bar.get_popup(&popup);
                    popup
                }
            };
            link.shared.asked.borrow_mut().remove(&id);
            link.shared.placed.borrow_mut().insert(id, self.pos.get());
            p.destroy();
            self.configured.set(false);
            *self.shown.borrow_mut() = Some((xdg, popup));
            surface.commit();
            let _ = link.conn.flush();
            return false;
        }
        if self.size.get() != size {
            self.size.set(size);
            self.reposition(link, id);
        }
        if let Some(serial) = link.shared.configures.borrow_mut().remove(&id) {
            if let Some((xdg, _)) = &*self.shown.borrow() {
                xdg.ack_configure(serial);
                self.configured.set(true);
                // It moves with the commit that follows.
                if let Some(at) = link.shared.placed.borrow_mut().remove(&id) {
                    self.moving.set(Some(at));
                }
            }
        }
        if let (true, Some((xdg, _))) = (self.configured.get(), &*self.shown.borrow()) {
            xdg.set_window_geometry(0, 0, size.0.max(1), size.1.max(1));
        }
        self.configured.get()
    }

    /// Takes the popup off the screen and unmakes it.
    fn hide(&self, link: &Link, surface: &WlSurface) {
        let Some((xdg, popup)) = self.shown.borrow_mut().take() else { return };
        surface.attach(None, 0, 0);
        surface.commit();
        popup.destroy();
        xdg.destroy();
        link.shared.configures.borrow_mut().remove(&surface.id().protocol_id());
        let _ = link.conn.flush();
    }
}

/// A part of the main window: a popup of it, or a subsurface.
pub struct Sub {
    link: Link,
    surface: WlSurface,
    role: Role,
    main: WlSurface,
    place: Rc<Place>,
}

impl Sub {
    pub fn id(&self) -> u32 {
        self.surface.id().protocol_id()
    }

    /// Moves the bar to `edge` of its monitor; nothing for other parts.
    pub fn set_bar_edge(&self, edge: popamp_core::config::BarEdge) {
        if let Role::Layer(layer) = &self.role {
            layer.set_anchor(bar_anchor(edge));
            self.surface.commit();
            let _ = self.link.conn.flush();
        }
    }

    pub fn move_to(&self, pos: Point) {
        let Link { conn, qh, shared, .. } = &self.link;
        let role = match &self.role {
            Role::Sub(role) => role,
            // The bar is where the compositor puts it, and the popups are
            // placed from it.
            Role::Layer(_) => {
                shared.origin.set(pos);
                self.place.known.set(pos);
                return;
            }
            Role::Popup(popup) => {
                popup.pos.set(pos);
                popup.reposition(&self.link, self.id());
                let _ = conn.flush();
                return;
            }
        };
        let origin = shared.origin.get();
        role.set_position(shared.units(pos.0 - origin.0), shared.units(pos.1 - origin.1));
        // A subsurface moves when its parent is next shown.
        self.main.commit();
        self.place.sent.borrow_mut().push_back(pos);
        conn.display().sync(qh, self.id());
        let _ = conn.flush();
    }

    pub fn request_redraw(&self) {
        self.link.shared.redraws.borrow_mut().push(self.id());
    }
}

impl Drop for Sub {
    fn drop(&mut self) {
        self.link.shared.places.borrow_mut().remove(&self.id());
        self.link.shared.dismissed.borrow_mut().remove(&self.id());
        self.link.shared.placed.borrow_mut().remove(&self.id());
        self.link.shared.asked.borrow_mut().remove(&self.id());
        let sub = matches!(self.role, Role::Sub(_));
        match &self.role {
            Role::Sub(role) => role.destroy(),
            Role::Popup(popup) => popup.hide(&self.link, &self.surface),
            Role::Layer(bar) => bar.destroy(),
        }
        self.surface.destroy();
        if sub {
            self.main.commit();
        }
        let _ = self.link.conn.flush();
    }
}

/// Draws a surface from memory shared with the compositor, with see-through
/// pixels where the window's shape has none.
pub struct Shm {
    link: Link,
    surface: WlSurface,
    /// Sizes a subsurface in the compositor's units; winit does that for
    /// the main window.
    viewport: Option<WpViewport>,
    /// A subsurface or popup, not the main window.
    sub: bool,
    /// The popup it is, when it is one.
    popup: Option<Rc<Popup>>,
    /// The bar it is, when it is one.
    layer: Option<ZwlrLayerSurfaceV1>,
    /// The bar's height last asked for, in the compositor's units.
    layer_h: i32,
    bufs: Vec<Buf>,
    /// The pixels the shape shows, for the size they were made for.
    shape: Option<(Point, Vec<bool>)>,
    /// Where the mouse is taken, when not everywhere; in screen pixels.
    input: Option<Vec<(i16, i16, u16, u16)>>,
    input_changed: bool,
    /// The compositor is ready for a picture: a window must wait for the
    /// compositor's first word about it.
    ready: bool,
}

impl Shm {
    fn new(link: Link, surface: WlSurface, viewport: Option<WpViewport>, sub: bool) -> Shm {
        Shm {
            link,
            surface,
            viewport,
            sub,
            popup: None,
            layer: None,
            layer_h: 0,
            bufs: Vec::new(),
            shape: None,
            input: None,
            input_changed: false,
            ready: sub,
        }
    }

    /// The main window may be drawn from now on.
    pub fn ready(&mut self) {
        self.ready = true;
    }

    /// Shows only what `mask` covers of a window `size` screen pixels big,
    /// and takes the mouse only there.
    pub fn set_shape(&mut self, mask: Option<&Mask>, size: Point) {
        let rects = mask.map(|m| shape_rects(Some(m), size));
        self.shape = rects.as_ref().map(|rects| {
            let mut on = vec![false; (size.0 * size.1).max(0) as usize];
            for &(x, y, w, h) in rects {
                for row in y as i32..(y as i32 + h as i32).min(size.1) {
                    let start = (row * size.0 + x as i32) as usize;
                    on[start..start + (w as i32).min(size.0 - x as i32) as usize].fill(true);
                }
            }
            (size, on)
        });
        self.input = rects;
        self.input_changed = true;
    }

    /// Shows a picture `w` x `h` screen pixels big, which `draw` paints.
    pub fn present(&mut self, (w, h): Point, draw: impl FnOnce(&mut [u32])) {
        if let Some(bar) = &self.layer {
            let id = self.surface.id().protocol_id();
            if let Some(serial) = self.link.shared.configures.borrow_mut().remove(&id) {
                bar.ack_configure(serial);
                self.ready = true;
            }
            let uh = self.link.shared.units(h).max(1);
            if self.ready && uh != self.layer_h {
                bar.set_size(0, uh as u32);
                bar.set_exclusive_zone(uh);
                self.layer_h = uh;
            }
        }
        if !self.ready || w <= 0 || h <= 0 {
            return;
        }
        if let Some(popup) = &self.popup {
            let units = (self.link.shared.units(w).max(1), self.link.shared.units(h).max(1));
            if !popup.ready(&self.link, &self.surface, units) {
                return;
            }
        }
        let full = self.whole_units((w, h));
        let Some(i) = self.free_buf(full) else { return };
        let buf = &mut self.bufs[i];
        let shape = self.shape.as_ref().filter(|(size, _)| *size == (w, h)).map(|(_, on)| on);
        let finish = |px: &mut [u32]| match shape {
            Some(on) => px.iter_mut().zip(on).for_each(|(p, on)| *p = if *on { *p | 0xff00_0000 } else { 0 }),
            None => px.iter_mut().for_each(|p| *p |= 0xff00_0000),
        };
        if full == (w, h) {
            draw(buf.pixels());
            finish(buf.pixels());
        } else {
            // Drawn apart, then put into the bigger picture row by row.
            let mut px = vec![0; (w * h) as usize];
            draw(&mut px);
            finish(&mut px);
            let out = buf.pixels();
            out.fill(0);
            for (row, from) in out.chunks_exact_mut(full.0 as usize).zip(px.chunks_exact(w as usize)) {
                row[..w as usize].copy_from_slice(from);
            }
        }
        buf.busy.store(true, Ordering::Relaxed);
        self.surface.attach(Some(&buf.buffer), 0, 0);
        self.commit(full);
    }

    /// `size` made a whole number of the compositor's units, which the
    /// main window's picture must be where a unit is two or three screen
    /// pixels: a compositor may refuse any other.
    fn whole_units(&self, size: Point) -> Point {
        let scale = self.link.shared.scale.get();
        if self.sub || scale.fract() != 0.0 {
            return size;
        }
        let unit = scale as i32;
        let up = |px: i32| (px + unit - 1) / unit * unit;
        (up(size.0), up(size.1))
    }

    /// Takes the window off the screen. The main window stays one for the
    /// compositor, with nothing to see or click: taking its picture away
    /// would be closing it.
    pub fn hide(&mut self, size: Point) {
        if !self.ready {
            return;
        }
        if let Some(popup) = &self.popup {
            popup.hide(&self.link, &self.surface);
            return;
        }
        if self.layer.is_some() {
            // Without a picture the bar is unmade for the compositor; the
            // next commit asks for it again, to be shown once it answers.
            self.surface.attach(None, 0, 0);
            self.surface.commit();
            self.surface.commit();
            (self.ready, self.layer_h) = (false, 0);
            let _ = self.link.conn.flush();
            return;
        }
        if self.sub {
            self.surface.attach(None, 0, 0);
            self.surface.commit();
            let _ = self.link.conn.flush();
            return;
        }
        let size = self.whole_units(size);
        let Some(i) = self.free_buf(size) else { return };
        let buf = &mut self.bufs[i];
        buf.pixels().fill(0);
        buf.busy.store(true, Ordering::Relaxed);
        self.surface.attach(Some(&buf.buffer), 0, 0);
        let (input, shape) = (self.input.replace(Vec::new()), self.shape.take());
        self.input_changed = true;
        self.commit(size);
        (self.input, self.shape, self.input_changed) = (input, shape, true);
    }

    /// Sends what was attached, `size` screen pixels big, to the screen.
    fn commit(&mut self, size: Point) {
        let Link { conn, qh, compositor, shared, .. } = &self.link;
        if self.surface.version() >= 4 {
            self.surface.damage_buffer(0, 0, size.0, size.1);
        } else {
            self.surface.damage(0, 0, i32::MAX, i32::MAX);
        }
        if let Some(viewport) = &self.viewport {
            viewport.set_destination(shared.units(size.0).max(1), shared.units(size.1).max(1));
        }
        if std::mem::take(&mut self.input_changed) {
            let region = self.input.as_ref().map(|rects| {
                let region = compositor.create_region(qh, ());
                let scale = shared.scale.get();
                for &(x, y, w, h) in rects {
                    // Whole units that the rectangle covers at least half of.
                    let unit = |px: i32| (px as f64 / scale).round() as i32;
                    let (x0, y0) = (unit(x as i32), unit(y as i32));
                    region.add(x0, y0, unit(x as i32 + w as i32) - x0, unit(y as i32 + h as i32) - y0);
                }
                region
            });
            self.surface.set_input_region(region.as_ref());
            if let Some(region) = region {
                region.destroy();
            }
        }
        self.surface.commit();
        // A popup that moves with this commit is there for the mouse once
        // the compositor has taken it: what the mouse did before comes
        // from where it was.
        if let Some(at) = self.popup.as_ref().and_then(|p| p.moving.take()) {
            let id = self.surface.id().protocol_id();
            if let Some(place) = shared.places.borrow().get(&id) {
                place.sent.borrow_mut().push_back(at);
                conn.display().sync(qh, id);
            }
        }
        let _ = conn.flush();
    }

    /// The index of a buffer of `size` the compositor is not showing.
    fn free_buf(&mut self, size: Point) -> Option<usize> {
        self.bufs.retain(|b| b.size == size || b.busy.load(Ordering::Relaxed));
        if let Some(i) = self.bufs.iter().position(|b| b.size == size && !b.busy.load(Ordering::Relaxed)) {
            return Some(i);
        }
        // The compositor holds them all: it is behind, and this frame is skipped.
        if self.bufs.len() >= 4 {
            return None;
        }
        match Buf::new(&self.link.shm, &self.link.qh, size) {
            Ok(buf) => {
                self.bufs.push(buf);
                Some(self.bufs.len() - 1)
            }
            Err(e) => {
                log::warn!("{e}");
                None
            }
        }
    }
}

impl Drop for Shm {
    fn drop(&mut self) {
        if let Some(viewport) = &self.viewport {
            viewport.destroy();
        }
    }
}

/// The edges of the monitors COSMIC's panels keep room at, as its settings
/// say: left, right, top and bottom. A panel's width the compositor tells,
/// its side only the settings.
fn panel_sides() -> [bool; 4] {
    let mut sides = [false; 4];
    let home = std::env::var_os("HOME").map(std::path::PathBuf::from).unwrap_or_default();
    let dirs = [home.join(".config/cosmic"), "/usr/share/cosmic".into()];
    let read = |rel: &str| dirs.iter().find_map(|d| std::fs::read_to_string(d.join(rel)).ok()).map(|s| s.trim().to_owned());
    let panels = read("com.system76.CosmicPanel/v1/entries").unwrap_or_default();
    for name in panels.split(|c: char| !c.is_alphanumeric() && c != '-' && c != '_').filter(|n| !n.is_empty()) {
        let at = |key: &str| read(&format!("com.system76.CosmicPanel.{name}/v1/{key}"));
        if at("exclusive_zone").as_deref() == Some("false") || at("autohide").is_some_and(|a| a != "Never" && a != "None") {
            continue;
        }
        match at("anchor").as_deref() {
            Some("Left") => sides[0] = true,
            Some("Right") => sides[1] = true,
            Some("Top") => sides[2] = true,
            Some("Bottom") => sides[3] = true,
            _ => {}
        }
    }
    sides
}

/// The display, surface and xdg_toplevel pointers of winit's `window`;
/// `None` when it is not a Wayland window.
type Handles = (*mut std::ffi::c_void, *mut std::ffi::c_void, Option<*mut std::ffi::c_void>);

fn handles(window: &Window) -> Option<Handles> {
    let (RawDisplayHandle::Wayland(display), RawWindowHandle::Wayland(surface)) =
        (window.display_handle().ok()?.as_raw(), window.window_handle().ok()?.as_raw())
    else {
        return None;
    };
    let toplevel = window.xdg_toplevel().map(|t| t.as_ptr());
    Some((display.display.as_ptr().cast(), surface.surface.as_ptr().cast(), toplevel))
}

/// Winit's xdg_toplevel at `toplevel`.
fn toplevel_of(conn: &Connection, toplevel: Option<*mut std::ffi::c_void>) -> Option<XdgToplevel> {
    // SAFETY: the toplevel is of a living winit window, which the caller keeps.
    let id = unsafe { ObjectId::from_ptr(XdgToplevel::interface(), toplevel?.cast()) }.ok()?;
    XdgToplevel::from_id(conn, id).ok()
}

/// The xdg_surface of winit's window whose xdg_toplevel is `toplevel`.
fn xdg_surface_of(conn: &Connection, toplevel: Option<*mut std::ffi::c_void>) -> Option<XdgSurface> {
    use smithay_client_toolkit::shell::xdg::XdgSurface as _;
    let toplevel = toplevel_of(conn, toplevel)?;
    let window = smithay_client_toolkit::shell::xdg::window::Window::from_xdg_toplevel(&toplevel)?;
    Some(window.xdg_surface().clone())
}

/// Whether the compositor behind `el` makes bars for programs (wlr-layer-shell).
pub fn has_layer_shell(el: &winit::event_loop::ActiveEventLoop) -> bool {
    struct Probe;
    impl Dispatch<WlRegistry, GlobalListContents> for Probe {
        fn event(_: &mut Probe, _: &WlRegistry, _: wl_registry::Event, _: &GlobalListContents, _: &Connection, _: &QueueHandle<Probe>) {}
    }
    let Ok(RawDisplayHandle::Wayland(display)) = el.display_handle().map(|h| h.as_raw()) else { return false };
    // SAFETY: the display is the event loop's, which outlives this call.
    let conn = Connection::from_backend(unsafe { Backend::from_foreign_display(display.display.as_ptr().cast()) });
    let Ok((globals, _queue)) = registry_queue_init::<Probe>(&conn) else { return false };
    let found = globals.contents().with_list(|list| list.iter().any(|g| g.interface == ZwlrLayerShellV1::interface().name));
    found && !std::env::var_os("POPAMP_WAYLAND_SUBSURFACES").is_some_and(|v| v == "1")
}

/// The sides a bar at `edge` holds to: across the monitor, at that edge.
fn bar_anchor(edge: popamp_core::config::BarEdge) -> zwlr_layer_surface_v1::Anchor {
    use zwlr_layer_surface_v1::Anchor;
    let side = match edge {
        popamp_core::config::BarEdge::Top => Anchor::Top,
        popamp_core::config::BarEdge::Bottom => Anchor::Bottom,
    };
    Anchor::Left | Anchor::Right | side
}
