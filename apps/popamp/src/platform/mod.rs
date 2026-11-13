//! Everything that depends on the windowing system: X11 or Wayland on
//! Linux, Win32 on Windows, AppKit on macOS. The rest of the program sees
//! only this module.

use crate::scale::Scale;
use std::num::NonZeroU32;
use std::sync::Arc;
use winit::dpi::PhysicalPosition;
use winit::window::{Window, WindowId};

#[cfg(windows)]
mod windows;
#[cfg(windows)]
pub use windows::{attributes, use_parent_console, Platform, OWNED_FOLLOW_OWNER};
#[cfg(linux_like)]
pub mod wayland;
#[cfg(linux_like)]
mod x11;
#[cfg(linux_like)]
pub use x11::{attributes, choose_window_system, Platform, OWNED_FOLLOW_OWNER};
#[cfg(target_os = "macos")]
mod macos;
#[cfg(target_os = "macos")]
pub use macos::{attributes, Platform, OWNED_FOLLOW_OWNER};

/// What a window is for, which decides how the window manager treats it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WindowRole {
    /// The main window: in the taskbar.
    Main,
    /// Equalizer and playlist: tool windows docked to the main one.
    Docked,
    /// A bar skin's main window: a panel at a screen edge.
    Bar,
    /// Pop-up menus: placed and stacked by us, not the window manager.
    Popup,
    /// Tooltips: like menus, but never focused.
    Tooltip,
    /// Popamp's own dialogs: ordinary windows with a title bar.
    Dialog,
}

/// A window of ours: one the system knows as a window, or under Wayland a
/// surface placed against the main window (see `wayland.rs`).
pub enum Win {
    Top(Arc<Window>),
    #[cfg(linux_like)]
    Sub(Box<wayland::Sub>),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WinId {
    Top(WindowId),
    #[cfg(linux_like)]
    Sub(u32),
}

impl Win {
    pub fn id(&self) -> WinId {
        match self {
            Win::Top(w) => WinId::Top(w.id()),
            #[cfg(linux_like)]
            Win::Sub(s) => WinId::Sub(s.id()),
        }
    }

    /// The system's window, when it is one.
    pub fn top(&self) -> Option<&Arc<Window>> {
        match self {
            Win::Top(w) => Some(w),
            #[cfg(linux_like)]
            Win::Sub(_) => None,
        }
    }

    /// Puts the top-left corner at `pos`, in screen pixels.
    pub fn move_to(&self, pos: (i32, i32)) {
        match self {
            Win::Top(w) => w.set_outer_position(PhysicalPosition::new(pos.0, pos.1)),
            #[cfg(linux_like)]
            Win::Sub(s) => s.move_to(pos),
        }
    }

    pub fn request_redraw(&self) {
        match self {
            Win::Top(w) => w.request_redraw(),
            #[cfg(linux_like)]
            Win::Sub(s) => s.request_redraw(),
        }
    }
}

/// What a window is drawn into.
pub enum Surface {
    Soft(softbuffer::Surface<Arc<Window>, Arc<Window>>),
    #[cfg(linux_like)]
    Shm(Box<wayland::Shm>),
}

impl Surface {
    /// The usual one, for a window of the system.
    pub fn soft(window: &Arc<Window>) -> Result<Surface, String> {
        softbuffer::Context::new(window.clone())
            .and_then(|c| softbuffer::Surface::new(&c, window.clone()))
            .map(Surface::Soft)
            .map_err(|e| format!("softbuffer: {e}"))
    }

    /// Shows a picture `size` screen pixels big, which `draw` paints.
    pub fn present(&mut self, size: (i32, i32), draw: impl FnOnce(&mut [u32])) {
        match self {
            Surface::Soft(s) => {
                let (Some(w), Some(h)) = (NonZeroU32::new(size.0.max(0) as u32), NonZeroU32::new(size.1.max(0) as u32)) else { return };
                if s.resize(w, h).is_err() {
                    return;
                }
                if let Ok(mut buf) = s.buffer_mut() {
                    draw(&mut buf);
                    if let Err(e) = buf.present() {
                        log::debug!("present failed: {e}");
                    }
                }
            }
            #[cfg(linux_like)]
            Surface::Shm(s) => s.present(size, draw),
        }
    }

    /// Under Wayland the picture carries the window's shape; elsewhere
    /// the window does (`Platform::set_shape`).
    #[cfg_attr(not(linux_like), allow(unused_variables))]
    pub fn set_shape(&mut self, mask: Option<&Mask>, size: (i32, i32)) {
        #[cfg(linux_like)]
        if let Surface::Shm(s) = self {
            s.set_shape(mask, size);
        }
    }

    /// Takes a window `size` screen pixels big off the screen where its
    /// picture decides that (Wayland).
    #[cfg_attr(not(linux_like), allow(unused_variables))]
    pub fn hide(&mut self, size: (i32, i32)) {
        #[cfg(linux_like)]
        if let Surface::Shm(s) = self {
            s.hide(size);
        }
    }

    /// The system asked for the window to be drawn: it may be from now on.
    pub fn ready(&mut self) {
        #[cfg(linux_like)]
        if let Surface::Shm(s) = self {
            s.ready();
        }
    }
}

/// Without Wayland every window is the system's.
#[cfg(not(linux_like))]
impl Platform {
    pub fn start(&mut self, _el: &winit::event_loop::ActiveEventLoop) {}

    pub fn on_wayland(&self) -> bool {
        false
    }

    pub fn surface(&mut self, window: &Arc<Window>, _pos: (i32, i32)) -> Result<Surface, String> {
        Surface::soft(window)
    }

    pub fn child(&self, _pos: (i32, i32), _input: bool) -> Option<(Win, Surface)> {
        None
    }

    pub fn set_scale(&self, _scale: f64) {}

    pub fn set_cursor(&mut self, _cursor: Option<&popamp_skin::cursors::CursorImage>) {}
}

/// Which pixels of a window show: `w` x `h` of them, each `scale` times
/// bigger on the screen.
pub struct Mask {
    pub w: i32,
    pub h: i32,
    pub on: Vec<bool>,
    pub scale: Scale,
}

/// The visible part of a window as `(x, y, w, h)` rectangles in screen
/// pixels: one per horizontal run of the mask.
#[cfg_attr(target_os = "macos", allow(dead_code))]
fn shape_rects(mask: Option<&Mask>, size: (i32, i32)) -> Vec<(i16, i16, u16, u16)> {
    let whole = vec![(0, 0, size.0 as u16, size.1 as u16)];
    let Some(Mask { w, h, on: mask, scale }) = mask else { return whole };
    let (w, h, scale) = (*w, *h, *scale);
    let mut out = Vec::new();
    for y in 0..h {
        let mut x = 0;
        while x < w {
            if !mask[(y * w + x) as usize] {
                x += 1;
                continue;
            }
            let start = x;
            while x < w && mask[(y * w + x) as usize] {
                x += 1;
            }
            let (x0, y0) = (scale.px(start), scale.px(y));
            out.push((x0 as i16, y0 as i16, (scale.px(x) - x0) as u16, (scale.px(y + 1) - y0) as u16));
        }
    }
    // A region that hides everything is a broken skin: ignore it.
    if out.is_empty() {
        whole
    } else {
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use popamp_core::config::Scaling;

    fn region(polys: &[Vec<(i32, i32)>], w: i32, h: i32, scale: Scale) -> Mask {
        Mask { w, h, on: popamp_skin::textcfg::region_mask(polys, w, h), scale }
    }

    #[test]
    fn no_mask_is_the_whole_window() {
        assert_eq!(shape_rects(None, (550, 232)), [(0, 0, 550, 232)]);
    }

    #[test]
    fn masks_become_scaled_row_runs() {
        // A 4x2 rectangle at (1, 0) inside a 6x3 window.
        let rect = vec![vec![(1, 0), (5, 0), (5, 2), (1, 2)]];
        let x2 = Scale::new(200, Scaling::Sharp);
        assert_eq!(shape_rects(Some(&region(&rect, 6, 3, x2)), (12, 6)), [(2, 0, 8, 2), (2, 2, 8, 2)]);
        // At x1.5 the runs meet without gaps: rows 0..2 and 2..3.
        let x15 = Scale::new(150, Scaling::Sharp);
        assert_eq!(shape_rects(Some(&region(&rect, 6, 3, x15)), (9, 5)), [(2, 0, 6, 2), (2, 2, 6, 1)]);
    }

    #[test]
    fn empty_masks_are_ignored() {
        let nothing = vec![vec![(0, 0), (0, 0), (0, 0)]];
        assert_eq!(shape_rects(Some(&region(&nothing, 10, 4, Scale::ONE)), (10, 4)), [(0, 0, 10, 4)]);
    }
}
