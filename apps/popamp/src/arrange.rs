//! Arranging the windows to fill a screen: the main window goes into a top
//! corner, the equalizer and the playlist under it, as wide as it, and the
//! library beside it on its free side. The playlist and the library are
//! stretched to the screen's edges, past their layout's steps.

use crate::bar::Screen;
use crate::scale::Scale;
use popamp_skin::layout::Dim;

/// The top corner the main window goes into.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Corner {
    Left,
    Right,
}

/// A window the arrangement sizes: the lengths its layout allows, and its
/// size now in skin pixels, which a fixed side keeps.
#[derive(Debug, Clone, Copy)]
pub struct Sized {
    pub width: Dim,
    pub height: Dim,
    pub now: (i32, i32),
}

/// The shown windows, sizes in skin pixels.
#[derive(Debug, Clone, Copy)]
pub struct Windows {
    pub main: (i32, i32),
    /// Where the main window stays when Popamp cannot move it (Wayland).
    pub main_at: Option<(i32, i32)>,
    pub eq: Option<(i32, i32)>,
    pub playlist: Option<Sized>,
    pub library: Option<Sized>,
}

/// Where the windows go, in screen pixels, with the new skin sizes of the
/// playlist and the library.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Placed {
    pub main: (i32, i32),
    pub eq: Option<(i32, i32)>,
    pub playlist: Option<((i32, i32), (i32, i32))>,
    pub library: Option<((i32, i32), (i32, i32))>,
}

/// Arranges `windows` in `area`, the usable part of a screen.
pub fn arrange(corner: Corner, area: Screen, windows: &Windows, scale: Scale) -> Placed {
    let px = |n: i32| scale.px(n);
    let (main_w, main_h) = (px(windows.main.0), px(windows.main.1));
    let (right, bottom) = (area.x + area.w, area.y + area.h);
    let main = windows.main_at.unwrap_or(match corner {
        Corner::Left => (area.x, area.y),
        Corner::Right => (right - main_w, area.y),
    });
    let mut below = main.1 + main_h;
    let eq = windows.eq.map(|(_, h)| {
        let at = (main.0, below);
        below += px(h);
        at
    });
    let playlist = windows.playlist.map(|p| {
        let width = p.width.snap(windows.main.0);
        ((main.0, below), (width, fill(p.height, p.now.1, below, bottom, scale)))
    });
    let library = windows.library.map(|l| {
        let height = fill(l.height, l.now.1, main.1, bottom, scale);
        match corner {
            Corner::Left => {
                let x = main.0 + main_w;
                ((x, main.1), (fill(l.width, l.now.0, x, right, scale), height))
            }
            Corner::Right => {
                let width = fill(l.width, l.now.0, area.x, main.0, scale);
                ((main.0 - px(width), main.1), (width, height))
            }
        }
    });
    Placed { main, eq, playlist, library }
}

/// The longest length `dim` allows for a side from `start` that ends at
/// `limit` or before, else its shortest; `now` for a fixed side.
pub fn fill(dim: Dim, now: i32, start: i32, limit: i32, scale: Scale) -> i32 {
    match dim {
        Dim::Fixed(_) => now,
        Dim::Free { min, .. } => longest_within(min, scale.skin(limit - start) + 1, |n| start + scale.px(n), limit),
    }
}

/// The longest length from `min` to `upto` that ends at `bound` or before,
/// else `min`.
pub fn longest_within(min: i32, upto: i32, end: impl Fn(i32) -> i32, bound: i32) -> i32 {
    (min..=upto.max(min)).rev().find(|&n| end(n) <= bound).unwrap_or(min)
}

#[cfg(test)]
mod tests {
    use super::*;
    use popamp_core::config::Scaling;

    /// A 1920×1080 monitor with a 32-pixel panel along its top.
    const AREA: Screen = Screen { x: 0, y: 32, w: 1920, h: 1048 };
    const PLAYLIST: Sized = Sized { width: Dim::Free { min: 275, step: 25 }, height: Dim::Free { min: 116, step: 29 }, now: (300, 232) };
    const LIBRARY: Sized = Sized { width: Dim::Free { min: 400, step: 25 }, height: Dim::Free { min: 232, step: 29 }, now: (500, 348) };

    fn all() -> Windows {
        Windows { main: (275, 116), main_at: None, eq: Some((275, 116)), playlist: Some(PLAYLIST), library: Some(LIBRARY) }
    }

    #[test]
    fn to_the_top_left_the_windows_fill_the_screen() {
        let p = arrange(Corner::Left, AREA, &all(), Scale::ONE);
        assert_eq!(p.main, (0, 32), "under the panel");
        assert_eq!(p.eq, Some((0, 148)));
        assert_eq!(p.playlist, Some(((0, 264), (275, 816))), "as wide as the main window, down to the bottom");
        assert_eq!(p.library, Some(((275, 32), (1645, 1048))), "right of it, to the right and the bottom");
    }

    #[test]
    fn to_the_top_right_the_library_takes_the_left() {
        let p = arrange(Corner::Right, AREA, &all(), Scale::ONE);
        assert_eq!(p.main, (1645, 32));
        assert_eq!(p.eq, Some((1645, 148)));
        assert_eq!(p.playlist, Some(((1645, 264), (275, 816))));
        assert_eq!(p.library, Some(((0, 32), (1645, 1048))));
    }

    #[test]
    fn without_the_equalizer_the_playlist_goes_under_the_main_window() {
        let p = arrange(Corner::Left, AREA, &Windows { eq: None, library: None, ..all() }, Scale::ONE);
        assert_eq!((p.eq, p.library), (None, None));
        assert_eq!(p.playlist, Some(((0, 148), (275, 932))));
    }

    #[test]
    fn a_shaded_playlist_keeps_its_height() {
        let shaded = Sized { height: Dim::Fixed(14), ..PLAYLIST };
        let p = arrange(Corner::Left, AREA, &Windows { playlist: Some(shaded), ..all() }, Scale::ONE);
        assert_eq!(p.playlist, Some(((0, 264), (275, 232))));
    }

    #[test]
    fn a_main_window_popamp_cannot_move_stays_and_the_rest_comes_under_it() {
        let w = Windows { main_at: Some((300, 200)), library: None, ..all() };
        let p = arrange(Corner::Right, AREA, &w, Scale::ONE);
        assert_eq!(p.main, (300, 200));
        assert_eq!(p.eq, Some((300, 316)));
        assert_eq!(p.playlist, Some(((300, 432), (275, 648))));
    }

    #[test]
    fn bigger_windows_end_at_the_bottom_without_a_gap() {
        let x15 = Scale::new(150, Scaling::Sharp);
        let p = arrange(Corner::Left, AREA, &all(), x15);
        assert_eq!(p.main, (0, 32));
        assert_eq!(p.eq, Some((0, 32 + 174)));
        let ((_, y), (w, h)) = p.playlist.unwrap();
        assert_eq!((y, w), (32 + 348, 275));
        assert!((1079..=1080).contains(&(y + x15.px(h))), "ends at {}", y + x15.px(h));
        let ((x, _), (lw, lh)) = p.library.unwrap();
        assert_eq!(x, 413);
        assert!((1919..=1920).contains(&(x + x15.px(lw))) && (1079..=1080).contains(&(32 + x15.px(lh))));
    }

    #[test]
    fn a_library_with_no_room_keeps_its_shortest_size() {
        let small = Screen { x: 0, y: 0, w: 600, h: 300 };
        let p = arrange(Corner::Left, small, &all(), Scale::ONE);
        assert_eq!(p.library, Some(((275, 0), (400, 300))));
        assert_eq!(p.playlist.map(|p| p.1), Some((275, 116)), "past the bottom: its shortest");
    }

    #[test]
    fn a_main_window_off_the_playlist_steps_gives_it_the_nearest_width() {
        let p = arrange(Corner::Left, AREA, &Windows { main: (290, 116), ..all() }, Scale::ONE);
        assert_eq!(p.playlist.map(|p| p.1 .0), Some(300));
    }
}
