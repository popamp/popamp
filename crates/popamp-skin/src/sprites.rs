//! Classic skin sprite geometry. Source rectangles are within the
//! named bitmap; positions are within the window at 1x scale. Verified
//! against a reference skin sheet.

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Rect {
    pub x: i32,
    pub y: i32,
    pub w: i32,
    pub h: i32,
}

pub const fn r(x: i32, y: i32, w: i32, h: i32) -> Rect {
    Rect { x, y, w, h }
}

impl Rect {
    pub fn contains(&self, px: i32, py: i32) -> bool {
        px >= self.x && py >= self.y && px < self.x + self.w && py < self.y + self.h
    }
    pub const fn at(&self, x: i32, y: i32) -> Rect {
        Rect { x, y, w: self.w, h: self.h }
    }
}

/// A sprite sheet a rectangle refers to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Sheet {
    Main,
    CButtons,
    TitleBar,
    ShufRep,
    Text,
    Volume,
    Balance,
    MonoSter,
    PlayPaus,
    Numbers,
    NumsEx,
    PosBar,
    EqMain,
    EqEx,
    PlEdit,
}

impl Sheet {
    pub const ALL: [Sheet; 15] = [
        Sheet::Main,
        Sheet::CButtons,
        Sheet::TitleBar,
        Sheet::ShufRep,
        Sheet::Text,
        Sheet::Volume,
        Sheet::Balance,
        Sheet::MonoSter,
        Sheet::PlayPaus,
        Sheet::Numbers,
        Sheet::NumsEx,
        Sheet::PosBar,
        Sheet::EqMain,
        Sheet::EqEx,
        Sheet::PlEdit,
    ];

    pub fn file_name(self) -> &'static str {
        match self {
            Sheet::Main => "main.bmp",
            Sheet::CButtons => "cbuttons.bmp",
            Sheet::TitleBar => "titlebar.bmp",
            Sheet::ShufRep => "shufrep.bmp",
            Sheet::Text => "text.bmp",
            Sheet::Volume => "volume.bmp",
            Sheet::Balance => "balance.bmp",
            Sheet::MonoSter => "monoster.bmp",
            Sheet::PlayPaus => "playpaus.bmp",
            Sheet::Numbers => "numbers.bmp",
            Sheet::NumsEx => "nums_ex.bmp",
            Sheet::PosBar => "posbar.bmp",
            Sheet::EqMain => "eqmain.bmp",
            Sheet::EqEx => "eq_ex.bmp",
            Sheet::PlEdit => "pledit.bmp",
        }
    }

    /// Size of the sheet in the reference skin.
    pub fn size(self) -> (i32, i32) {
        match self {
            Sheet::Main => (275, 116),
            Sheet::CButtons => (136, 36),
            Sheet::TitleBar => (344, 87),
            Sheet::ShufRep => (92, 85),
            Sheet::Text => (155, 74),
            Sheet::Volume | Sheet::Balance => (68, 433),
            Sheet::MonoSter => (58, 24),
            Sheet::PlayPaus => (42, 9),
            Sheet::Numbers => (99, 13),
            Sheet::NumsEx => (108, 13),
            Sheet::PosBar => (307, 10),
            Sheet::EqMain => (275, 315),
            Sheet::EqEx => (275, 82),
            Sheet::PlEdit => (280, 186),
        }
    }
}

/// A button's sprites: normal and pressed.
#[derive(Debug, Clone, Copy)]
pub struct Button {
    pub normal: Rect,
    pub pressed: Rect,
}

/// A toggle button: off/on, each with a pressed variant.
#[derive(Debug, Clone, Copy)]
pub struct Toggle {
    pub off: Rect,
    pub off_pressed: Rect,
    pub on: Rect,
    pub on_pressed: Rect,
}

impl Toggle {
    pub fn pick(&self, on: bool, pressed: bool) -> Rect {
        match (on, pressed) {
            (false, false) => self.off,
            (false, true) => self.off_pressed,
            (true, false) => self.on,
            (true, true) => self.on_pressed,
        }
    }
}

pub mod main {
    use super::*;
    pub const SIZE: (i32, i32) = (275, 116);
    pub const SHADE_SIZE: (i32, i32) = (275, 14);

    // titlebar.bmp
    pub const TITLE_ACTIVE: Rect = r(27, 0, 275, 14);
    pub const TITLE_INACTIVE: Rect = r(27, 15, 275, 14);
    pub const SHADE_ACTIVE: Rect = r(27, 29, 275, 14);
    pub const SHADE_INACTIVE: Rect = r(27, 42, 275, 14);
    pub const OPTIONS: Button = Button { normal: r(0, 0, 9, 9), pressed: r(0, 9, 9, 9) };
    pub const MINIMIZE: Button = Button { normal: r(9, 0, 9, 9), pressed: r(9, 9, 9, 9) };
    pub const SHADE: Button = Button { normal: r(0, 18, 9, 9), pressed: r(9, 18, 9, 9) };
    pub const UNSHADE: Button = Button { normal: r(0, 27, 9, 9), pressed: r(9, 27, 9, 9) };
    pub const CLOSE: Button = Button { normal: r(18, 0, 9, 9), pressed: r(18, 9, 9, 9) };
    pub const CLUTTER_BG: Rect = r(304, 0, 8, 43);
    /// Pressed/active clutter buttons O, A, I, D, V; `.y` offsets are within the bar.
    pub const CLUTTER_SELECTED: [Rect; 5] = [r(304, 47, 8, 8), r(312, 55, 8, 7), r(320, 62, 8, 7), r(328, 69, 8, 8), r(336, 77, 8, 7)];
    pub const CLUTTER_OFFSETS: [i32; 5] = [3, 11, 18, 25, 33];
    pub const SHADE_POS_BG: Rect = r(0, 36, 17, 7);
    pub const SHADE_POS_THUMB: Rect = r(20, 36, 3, 7);

    // Positions
    pub const AT_OPTIONS: (i32, i32) = (6, 3);
    pub const AT_MINIMIZE: (i32, i32) = (244, 3);
    pub const AT_SHADE: (i32, i32) = (254, 3);
    pub const AT_CLOSE: (i32, i32) = (264, 3);
    pub const AT_CLUTTER: (i32, i32) = (10, 22);
    pub const AT_SHADE_POS: (i32, i32) = (226, 4);
    pub const AT_SHADE_TIME: (i32, i32) = (130, 4);
    /// Invisible transport hot spots in the shaded main window.
    pub const SHADE_TRANSPORT: [Rect; 6] =
        [r(169, 2, 8, 10), r(177, 2, 10, 10), r(187, 2, 10, 10), r(197, 2, 9, 10), r(206, 2, 8, 10), r(216, 2, 9, 10)];

    // cbuttons.bmp
    pub const PREV: Button = Button { normal: r(0, 0, 23, 18), pressed: r(0, 18, 23, 18) };
    pub const PLAY: Button = Button { normal: r(23, 0, 23, 18), pressed: r(23, 18, 23, 18) };
    pub const PAUSE: Button = Button { normal: r(46, 0, 23, 18), pressed: r(46, 18, 23, 18) };
    pub const STOP: Button = Button { normal: r(69, 0, 23, 18), pressed: r(69, 18, 23, 18) };
    pub const NEXT: Button = Button { normal: r(92, 0, 22, 18), pressed: r(92, 18, 22, 18) };
    pub const EJECT: Button = Button { normal: r(114, 0, 22, 16), pressed: r(114, 16, 22, 16) };
    pub const AT_PREV: (i32, i32) = (16, 88);
    pub const AT_PLAY: (i32, i32) = (39, 88);
    pub const AT_PAUSE: (i32, i32) = (62, 88);
    pub const AT_STOP: (i32, i32) = (85, 88);
    pub const AT_NEXT: (i32, i32) = (108, 88);
    pub const AT_EJECT: (i32, i32) = (136, 89);

    // shufrep.bmp
    pub const REPEAT: Toggle =
        Toggle { off: r(0, 0, 28, 15), off_pressed: r(0, 15, 28, 15), on: r(0, 30, 28, 15), on_pressed: r(0, 45, 28, 15) };
    pub const SHUFFLE: Toggle =
        Toggle { off: r(28, 0, 47, 15), off_pressed: r(28, 15, 47, 15), on: r(28, 30, 47, 15), on_pressed: r(28, 45, 47, 15) };
    pub const EQ: Toggle =
        Toggle { off: r(0, 61, 23, 12), off_pressed: r(46, 61, 23, 12), on: r(0, 73, 23, 12), on_pressed: r(46, 73, 23, 12) };
    pub const PL: Toggle =
        Toggle { off: r(23, 61, 23, 12), off_pressed: r(69, 61, 23, 12), on: r(23, 73, 23, 12), on_pressed: r(69, 73, 23, 12) };
    pub const AT_SHUFFLE: (i32, i32) = (164, 89);
    pub const AT_REPEAT: (i32, i32) = (210, 89);
    pub const AT_EQ: (i32, i32) = (219, 58);
    pub const AT_PL: (i32, i32) = (242, 58);

    // playpaus.bmp
    pub const PLAYING: Rect = r(0, 0, 9, 9);
    pub const PAUSED: Rect = r(9, 0, 9, 9);
    pub const STOPPED: Rect = r(18, 0, 9, 9);
    pub const NOT_WORKING: Rect = r(36, 0, 3, 9);
    pub const WORKING: Rect = r(39, 0, 3, 9);
    pub const AT_PLAYPAUS: (i32, i32) = (26, 28);
    pub const AT_WORK: (i32, i32) = (24, 28);

    // monoster.bmp
    pub const STEREO_ON: Rect = r(0, 0, 29, 12);
    pub const STEREO_OFF: Rect = r(0, 12, 29, 12);
    pub const MONO_ON: Rect = r(29, 0, 27, 12);
    pub const MONO_OFF: Rect = r(29, 12, 27, 12);
    pub const AT_MONO: (i32, i32) = (212, 41);
    pub const AT_STEREO: (i32, i32) = (239, 41);

    // numbers.bmp / nums_ex.bmp: digit n at (9n, 0), 9x13
    pub const DIGIT_W: i32 = 9;
    pub const DIGIT_H: i32 = 13;
    pub const NUMBERS_BLANK_X: i32 = 90;
    pub const NUMBERS_MINUS: Rect = r(20, 6, 5, 1);
    pub const NUMS_EX_MINUS: Rect = r(99, 0, 9, 13);
    pub const AT_MINUS: (i32, i32) = (36, 26);
    pub const AT_DIGITS: [(i32, i32); 4] = [(48, 26), (60, 26), (78, 26), (90, 26)];
    pub const TIME_AREA: Rect = r(36, 26, 63, 13);

    // posbar.bmp
    pub const POS_BG: Rect = r(0, 0, 248, 10);
    pub const POS_THUMB: Button = Button { normal: r(248, 0, 29, 10), pressed: r(278, 0, 29, 10) };
    pub const POS_AREA: Rect = r(16, 72, 248, 10);

    // volume.bmp / balance.bmp: 28 frames, 15 px apart
    pub const VOLUME_AREA: Rect = r(107, 57, 68, 13);
    pub const BALANCE_AREA: Rect = r(177, 57, 38, 13);
    pub const BALANCE_SRC_X: i32 = 9;
    pub const SLIDER_FRAME_STEP: i32 = 15;
    pub const SLIDER_FRAMES: i32 = 28;
    pub const SLIDER_THUMB: Button = Button { normal: r(15, 422, 14, 11), pressed: r(0, 422, 14, 11) };

    // text areas
    pub const MARQUEE: Rect = r(111, 27, 154, 6);
    pub const KBPS: Rect = r(111, 43, 15, 6);
    pub const KHZ: Rect = r(156, 43, 10, 6);
    pub const VIS: Rect = r(24, 43, 76, 16);
    pub const TITLE_DRAG: Rect = r(0, 0, 275, 14);
}

pub mod eq {
    use super::*;
    pub const SIZE: (i32, i32) = (275, 116);
    pub const SHADE_SIZE: (i32, i32) = (275, 14);
    pub const BG: Rect = r(0, 0, 275, 116);
    pub const TITLE_ACTIVE: Rect = r(0, 134, 275, 14);
    pub const TITLE_INACTIVE: Rect = r(0, 149, 275, 14);
    pub const ON: Toggle =
        Toggle { off: r(10, 119, 26, 12), off_pressed: r(128, 119, 26, 12), on: r(69, 119, 26, 12), on_pressed: r(187, 119, 26, 12) };
    pub const AUTO: Toggle =
        Toggle { off: r(36, 119, 32, 12), off_pressed: r(154, 119, 32, 12), on: r(95, 119, 32, 12), on_pressed: r(213, 119, 32, 12) };
    pub const PRESETS: Button = Button { normal: r(224, 164, 44, 12), pressed: r(224, 176, 44, 12) };
    pub const CLOSE: Button = Button { normal: r(0, 116, 9, 9), pressed: r(0, 125, 9, 9) };
    pub const THUMB: Button = Button { normal: r(0, 164, 11, 11), pressed: r(0, 176, 11, 11) };
    pub const GRAPH_BG: Rect = r(0, 294, 113, 19);
    pub const GRAPH_COLORS: Rect = r(115, 294, 1, 19);
    pub const PREAMP_LINE: Rect = r(0, 314, 113, 1);
    pub const SLIDER_W: i32 = 14;
    pub const SLIDER_H: i32 = 63;

    /// Background frame `k` (0..28) of a band slider.
    pub fn slider_frame(k: i32) -> Rect {
        let k = k.clamp(0, 27);
        r(13 + (k % 14) * 15, 164 + (k / 14) * 65, SLIDER_W, SLIDER_H)
    }

    // eq_ex.bmp
    pub const SHADE_ACTIVE: Rect = r(0, 0, 275, 14);
    pub const SHADE_INACTIVE: Rect = r(0, 15, 275, 14);
    pub const UNSHADE_PRESSED: Rect = r(1, 38, 9, 9);
    pub const SHADE_CLOSE_PRESSED: Rect = r(11, 38, 9, 9);

    /// Windowshade volume and balance sliders: a 3x7 thumb for the lower,
    /// middle and upper third of the range.
    pub const SHADE_VOLUME_THUMBS: [Rect; 3] = [r(1, 30, 3, 7), r(4, 30, 3, 7), r(7, 30, 3, 7)];
    pub const SHADE_BALANCE_THUMBS: [Rect; 3] = [r(11, 30, 3, 7), r(14, 30, 3, 7), r(17, 30, 3, 7)];
    pub const AT_SHADE_VOLUME: Rect = r(61, 4, 97, 7);
    pub const AT_SHADE_BALANCE: Rect = r(164, 4, 42, 7);

    pub const AT_ON: (i32, i32) = (14, 18);
    pub const AT_AUTO: (i32, i32) = (40, 18);
    pub const AT_PRESETS: (i32, i32) = (217, 18);
    pub const AT_CLOSE: (i32, i32) = (264, 3);
    pub const AT_SHADE: (i32, i32) = (254, 3);
    pub const AT_GRAPH: (i32, i32) = (86, 17);
    pub const AT_PREAMP: (i32, i32) = (21, 38);
    pub const BAND_X0: i32 = 78;
    pub const BAND_STEP: i32 = 18;
    pub const BAND_Y: i32 = 38;
}

pub mod pl {
    use super::*;
    pub const MIN_SIZE: (i32, i32) = (275, 116);
    pub const STEP: (i32, i32) = (25, 29);
    pub const TOP_LEFT: (Rect, Rect) = (r(0, 0, 25, 20), r(0, 21, 25, 20));
    pub const TITLE: (Rect, Rect) = (r(26, 0, 100, 20), r(26, 21, 100, 20));
    pub const TOP_TILE: (Rect, Rect) = (r(127, 0, 25, 20), r(127, 21, 25, 20));
    pub const TOP_RIGHT: (Rect, Rect) = (r(153, 0, 25, 20), r(153, 21, 25, 20));
    pub const LEFT_TILE: Rect = r(0, 42, 12, 29);
    pub const RIGHT_TILE: Rect = r(31, 42, 20, 29);
    pub const BOTTOM_LEFT: Rect = r(0, 72, 125, 38);
    pub const BOTTOM_RIGHT: Rect = r(126, 72, 150, 38);
    pub const BOTTOM_TILE: Rect = r(179, 0, 25, 38);
    pub const VIS: Rect = r(205, 0, 75, 38);
    pub const SCROLL_THUMB: Button = Button { normal: r(52, 53, 8, 18), pressed: r(61, 53, 8, 18) };
    pub const CLOSE_PRESSED: Rect = r(52, 42, 9, 9);
    pub const SHADE_PRESSED: Rect = r(62, 42, 9, 9);
    /// The shaded strip's left end, focused or not; 72, 57 under it is the tile.
    pub const SHADE_LEFT: Rect = r(72, 42, 25, 14);
    pub const SHADE_RIGHT: (Rect, Rect) = (r(99, 42, 50, 14), r(99, 57, 50, 14));
    pub const SHADE_TILE: Rect = r(72, 57, 25, 14);

    /// Pop-up menu buttons (normal, pressed) for ADD, REM, SEL, MISC, LIST,
    /// listed bottom-up as they stack above the bar button.
    pub fn menu_items(menu: usize) -> &'static [(Rect, Rect)] {
        const ADD: [(Rect, Rect); 3] =
            [(r(0, 149, 22, 18), r(23, 149, 22, 18)), (r(0, 130, 22, 18), r(23, 130, 22, 18)), (r(0, 111, 22, 18), r(23, 111, 22, 18))];
        const REM: [(Rect, Rect); 4] = [
            (r(54, 168, 22, 18), r(77, 168, 22, 18)),
            (r(54, 149, 22, 18), r(77, 149, 22, 18)),
            (r(54, 130, 22, 18), r(77, 130, 22, 18)),
            (r(54, 111, 22, 18), r(77, 111, 22, 18)),
        ];
        const SEL: [(Rect, Rect); 3] = [
            (r(104, 149, 22, 18), r(127, 149, 22, 18)),
            (r(104, 130, 22, 18), r(127, 130, 22, 18)),
            (r(104, 111, 22, 18), r(127, 111, 22, 18)),
        ];
        const MISC: [(Rect, Rect); 3] = [
            (r(154, 149, 22, 18), r(177, 149, 22, 18)),
            (r(154, 130, 22, 18), r(177, 130, 22, 18)),
            (r(154, 111, 22, 18), r(177, 111, 22, 18)),
        ];
        const LIST: [(Rect, Rect); 3] = [
            (r(204, 149, 22, 18), r(227, 149, 22, 18)),
            (r(204, 130, 22, 18), r(227, 130, 22, 18)),
            (r(204, 111, 22, 18), r(227, 111, 22, 18)),
        ];
        match menu {
            0 => &ADD,
            1 => &REM,
            2 => &SEL,
            3 => &MISC,
            _ => &LIST,
        }
    }
    /// Decorative bar left of each pop-up menu.
    pub const MENU_BARS: [Rect; 5] = [r(48, 111, 3, 54), r(100, 111, 3, 72), r(150, 111, 3, 54), r(200, 111, 3, 54), r(250, 111, 3, 54)];
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fits(sheet: Sheet, rect: Rect) {
        let (w, h) = sheet.size();
        assert!(rect.x >= 0 && rect.y >= 0 && rect.x + rect.w <= w && rect.y + rect.h <= h, "{rect:?} outside {sheet:?} {w}x{h}");
    }

    #[test]
    fn every_sprite_fits_its_sheet() {
        use main as m;
        for rect in
            [m::TITLE_ACTIVE, m::TITLE_INACTIVE, m::SHADE_ACTIVE, m::SHADE_INACTIVE, m::CLUTTER_BG, m::SHADE_POS_BG, m::SHADE_POS_THUMB]
        {
            fits(Sheet::TitleBar, rect);
        }
        for b in [m::OPTIONS, m::MINIMIZE, m::SHADE, m::UNSHADE, m::CLOSE] {
            fits(Sheet::TitleBar, b.normal);
            fits(Sheet::TitleBar, b.pressed);
        }
        m::CLUTTER_SELECTED.iter().for_each(|&r| fits(Sheet::TitleBar, r));
        for b in [m::PREV, m::PLAY, m::PAUSE, m::STOP, m::NEXT, m::EJECT] {
            fits(Sheet::CButtons, b.normal);
            fits(Sheet::CButtons, b.pressed);
        }
        for t in [m::REPEAT, m::SHUFFLE, m::EQ, m::PL] {
            for on in [false, true] {
                for p in [false, true] {
                    fits(Sheet::ShufRep, t.pick(on, p));
                }
            }
        }
        for rect in [m::PLAYING, m::PAUSED, m::STOPPED, m::NOT_WORKING, m::WORKING] {
            fits(Sheet::PlayPaus, rect);
        }
        for rect in [m::STEREO_ON, m::STEREO_OFF, m::MONO_ON, m::MONO_OFF] {
            fits(Sheet::MonoSter, rect);
        }
        fits(Sheet::Numbers, r(m::NUMBERS_BLANK_X, 0, 9, 13));
        fits(Sheet::Numbers, m::NUMBERS_MINUS);
        fits(Sheet::NumsEx, m::NUMS_EX_MINUS);
        fits(Sheet::PosBar, m::POS_BG);
        fits(Sheet::PosBar, m::POS_THUMB.pressed);
        fits(Sheet::Volume, r(0, 27 * m::SLIDER_FRAME_STEP, 68, 13));
        fits(Sheet::Volume, m::SLIDER_THUMB.normal);
        fits(Sheet::Balance, r(m::BALANCE_SRC_X, 27 * m::SLIDER_FRAME_STEP, 38, 13));
        for rect in [eq::BG, eq::TITLE_ACTIVE, eq::TITLE_INACTIVE, eq::GRAPH_BG, eq::GRAPH_COLORS, eq::PREAMP_LINE] {
            fits(Sheet::EqMain, rect);
        }
        for k in 0..28 {
            fits(Sheet::EqMain, eq::slider_frame(k));
        }
        for t in [eq::ON, eq::AUTO] {
            for on in [false, true] {
                for p in [false, true] {
                    fits(Sheet::EqMain, t.pick(on, p));
                }
            }
        }
        for b in [eq::PRESETS, eq::CLOSE, eq::THUMB] {
            fits(Sheet::EqMain, b.normal);
            fits(Sheet::EqMain, b.pressed);
        }
        for rect in [eq::SHADE_ACTIVE, eq::SHADE_INACTIVE, eq::UNSHADE_PRESSED, eq::SHADE_CLOSE_PRESSED]
            .into_iter()
            .chain(eq::SHADE_VOLUME_THUMBS)
            .chain(eq::SHADE_BALANCE_THUMBS)
        {
            fits(Sheet::EqEx, rect);
        }
        for (a, b) in [pl::TOP_LEFT, pl::TITLE, pl::TOP_TILE, pl::TOP_RIGHT, pl::SHADE_RIGHT] {
            fits(Sheet::PlEdit, a);
            fits(Sheet::PlEdit, b);
        }
        for rect in [
            pl::SHADE_LEFT,
            pl::LEFT_TILE,
            pl::RIGHT_TILE,
            pl::BOTTOM_LEFT,
            pl::BOTTOM_RIGHT,
            pl::BOTTOM_TILE,
            pl::VIS,
            pl::CLOSE_PRESSED,
            pl::SHADE_PRESSED,
        ] {
            fits(Sheet::PlEdit, rect);
        }
        for m in 0..5 {
            for &(a, b) in pl::menu_items(m) {
                fits(Sheet::PlEdit, a);
                fits(Sheet::PlEdit, b);
            }
            fits(Sheet::PlEdit, pl::MENU_BARS[m]);
        }
    }
}
