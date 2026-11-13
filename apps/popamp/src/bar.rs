//! A bar skin's main window: a strip across a monitor at its top or bottom
//! edge, inside the panels the desktop already has there. Its strip is
//! reserved like a taskbar's, so maximized windows keep out of it; the
//! playlist and the equalizer sit against it at the right end.

use crate::model::Model;
use popamp_core::config::BarEdge;
use popamp_skin::layout::{Placement, WindowKey};

/// A monitor's area in screen pixels.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Screen {
    pub x: i32,
    pub y: i32,
    pub w: i32,
    pub h: i32,
}

/// The edge the bar is at; `None` when the main window is no bar now.
pub fn edge(model: &Model) -> Option<BarEdge> {
    match crate::bind::placement(model, WindowKey::Main) {
        Placement::Bar(skin_edge) => Some(model.settings().bar_edge.unwrap_or(skin_edge)),
        _ => None,
    }
}

/// The part of `monitor` a bar spans: what the desktop's own panels leave
/// of it, from the work areas the window manager publishes. Those count
/// the bar's own strip too, so where `bar` (the strip reserved for it)
/// sits just outside the work area, it is given back.
pub fn usable(monitor: Screen, work_areas: &[Screen], bar: Option<Screen>) -> Screen {
    let overlap = |a: &Screen| {
        let (x0, y0) = (a.x.max(monitor.x), a.y.max(monitor.y));
        let (x1, y1) = ((a.x + a.w).min(monitor.x + monitor.w), (a.y + a.h).min(monitor.y + monitor.h));
        (x1 > x0 && y1 > y0).then_some(Screen { x: x0, y: y0, w: x1 - x0, h: y1 - y0 })
    };
    let Some(area) = work_areas.iter().filter_map(overlap).max_by_key(|a| a.w as i64 * a.h as i64) else { return monitor };
    let (mut top, mut bottom) = (area.y, area.y + area.h);
    if let Some(b) = bar {
        if b.y + b.h == top {
            top = b.y;
        }
        if b.y == bottom {
            bottom = b.y + b.h;
        }
    }
    Screen { y: top, h: bottom - top, ..area }
}

/// Top-left corners of the windows in [`WindowKey::ALL`] order, from their
/// sizes in screen pixels: the bar across `screen` at `edge`, the playlist
/// at the right end against the bar and the equalizer left of the playlist,
/// or at the right end while the playlist is closed.
pub fn places(edge: BarEdge, screen: Screen, sizes: [(i32, i32); 3], pl_open: bool) -> [(i32, i32); 3] {
    let [(_, bar_h), (eq_w, eq_h), (pl_w, pl_h)] = sizes;
    let against = |h: i32| match edge {
        BarEdge::Top => screen.y + bar_h,
        BarEdge::Bottom => screen.y + screen.h - bar_h - h,
    };
    let right = screen.x + screen.w;
    let pl_x = right - pl_w;
    let eq_x = if pl_open { pl_x - eq_w } else { right - eq_w };
    let bar_y = match edge {
        BarEdge::Top => screen.y,
        BarEdge::Bottom => screen.y + screen.h - bar_h,
    };
    [(screen.x, bar_y), (eq_x, against(eq_h)), (pl_x, against(pl_h))]
}

/// The strip a bar takes at `edge` of `screen`, `height` pixels high,
/// which other windows are kept out of.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Strip {
    pub edge: BarEdge,
    pub screen: Screen,
    pub height: i32,
}

impl Strip {
    /// The strip itself, in screen pixels.
    pub fn area(&self) -> Screen {
        let Strip { edge, screen, height } = *self;
        match edge {
            BarEdge::Top => Screen { h: height, ..screen },
            BarEdge::Bottom => Screen { y: screen.y + screen.h - height, h: height, ..screen },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const RIGHT_MONITOR: Screen = Screen { x: 1920, y: 0, w: 1280, h: 1024 };
    const SIZES: [(i32, i32); 3] = [(1280, 28), (275, 116), (275, 232)];

    #[test]
    fn a_bottom_bar_has_the_playlist_and_equalizer_above_it_at_the_right() {
        let [bar, eq, pl] = places(BarEdge::Bottom, RIGHT_MONITOR, SIZES, true);
        assert_eq!(bar, (1920, 1024 - 28));
        assert_eq!(pl, (3200 - 275, 1024 - 28 - 232));
        assert_eq!(eq, (3200 - 550, 1024 - 28 - 116));
    }

    #[test]
    fn a_top_bar_has_them_below_it() {
        let [bar, eq, pl] = places(BarEdge::Top, RIGHT_MONITOR, SIZES, true);
        assert_eq!(bar, (1920, 0));
        assert_eq!((eq.1, pl.1), (28, 28));
    }

    #[test]
    fn the_equalizer_takes_the_corner_while_the_playlist_is_closed() {
        let [_, eq, _] = places(BarEdge::Bottom, RIGHT_MONITOR, SIZES, false);
        assert_eq!(eq.0, 3200 - 275);
    }

    const MONITOR: Screen = Screen { x: 0, y: 0, w: 1920, h: 1080 };
    /// A desktop with a 52 pixel panel at the bottom.
    const ABOVE_PANEL: Screen = Screen { x: 0, y: 0, w: 1920, h: 1028 };

    #[test]
    fn a_bar_keeps_inside_the_desktops_panels() {
        assert_eq!(usable(MONITOR, &[ABOVE_PANEL], None), ABOVE_PANEL);
        assert_eq!(usable(MONITOR, &[], None), MONITOR, "no work area published");
    }

    #[test]
    fn the_bars_own_strip_is_not_taken_for_a_panel() {
        // Bottom bar 28 high above the panel: the work area ends at its top.
        let work = Screen { h: 1000, ..ABOVE_PANEL };
        let bar = Screen { x: 0, y: 1000, w: 1920, h: 28 };
        assert_eq!(usable(MONITOR, &[work], Some(bar)), ABOVE_PANEL);
        // Before the window manager counts the strip, the area is the same.
        assert_eq!(usable(MONITOR, &[ABOVE_PANEL], Some(bar)), ABOVE_PANEL);
        // A top bar.
        let work = Screen { y: 28, h: 1000, ..ABOVE_PANEL };
        let bar = Screen { y: 0, ..bar };
        assert_eq!(usable(MONITOR, &[work], Some(bar)), ABOVE_PANEL);
    }

    #[test]
    fn a_strip_is_the_bars_height_at_its_edge() {
        let bottom = Strip { edge: BarEdge::Bottom, screen: RIGHT_MONITOR, height: 28 };
        assert_eq!(bottom.area(), Screen { x: 1920, y: 1024 - 28, w: 1280, h: 28 });
        let top = Strip { edge: BarEdge::Top, ..bottom };
        assert_eq!(top.area(), Screen { x: 1920, y: 0, w: 1280, h: 28 });
    }

    #[test]
    fn the_work_area_is_the_one_on_the_bars_monitor() {
        let left = Screen { x: 0, y: 0, w: 1920, h: 1028 };
        assert_eq!(usable(RIGHT_MONITOR, &[left, Screen { h: 1000, ..RIGHT_MONITOR }], None), Screen { h: 1000, ..RIGHT_MONITOR });
        // One work area across both monitors is cut to the bar's.
        let both = Screen { x: 0, y: 0, w: 3200, h: 1000 };
        assert_eq!(usable(RIGHT_MONITOR, &[both], None), Screen { h: 1000, ..RIGHT_MONITOR });
    }

    #[test]
    fn the_edge_is_the_users_choice_or_the_skins() {
        let mut m = Model::for_tests();
        assert_eq!(edge(&m), None, "the default skin has no bar");
        m.skin.layout.windows[0].placement = Placement::Bar(BarEdge::Bottom);
        assert_eq!(edge(&m), Some(BarEdge::Bottom));
        m.settings_mut().bar_edge = Some(BarEdge::Top);
        assert_eq!(edge(&m), Some(BarEdge::Top));
    }
}
