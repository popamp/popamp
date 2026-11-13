//! The layout of a classic (`.wsz`) skin: Winamp 2's main window,
//! equalizer and playlist editor, built from the sprite geometry in
//! [`crate::sprites`]. Elements are listed in drawing order.

use crate::cursors::CursorKind as C;
use crate::layout::*;
use crate::sprites::{eq as e, main as m, pl as p, r, Button, Rect, Sheet, Toggle};
use popamp_core::Command;

/// `volume.bmp` taller than this has a slider thumb under its 28 frames.
const SLIDER_THUMB_BELOW: i32 = 422;

pub fn layout(volume_h: i32, balance_h: i32) -> Layout {
    Layout { windows: vec![main_window(volume_h, balance_h), eq_window(), playlist_window()] }
}

fn at(pos: (i32, i32), w: i32, h: i32) -> Place {
    Place::fixed(pos.0, pos.1, w, h)
}

fn rect(rc: Rect) -> Place {
    Place::fixed(rc.x, rc.y, rc.w, rc.h)
}

fn paint(sheet: Sheet, src: Rect) -> Paint {
    Paint::new(sheet, src)
}

fn image(place: Place, look: Look) -> Element {
    Element::new(place, Kind::Image { look, fill: Fill::Once, lit: None })
}

fn button_look(sheet: Sheet, b: Button) -> Look {
    Look::button(paint(sheet, b.normal), paint(sheet, b.pressed))
}

fn toggle_look(sheet: Sheet, t: Toggle) -> Look {
    Look::toggle(paint(sheet, t.off), paint(sheet, t.off_pressed), paint(sheet, t.on), paint(sheet, t.on_pressed))
}

fn button(place: Place, look: Look, action: Action) -> Element {
    Element::new(place, Kind::Button { look, action })
}

/// An invisible button.
fn hot_spot(place: Place, action: Action) -> Element {
    button(place, Look::default(), action)
}

fn click(plain: Command, shift: Command, ctrl: Command) -> Action {
    Action::Click { plain, shift: Some(shift), ctrl: Some(ctrl) }
}

fn transport() -> [(Action, MenuRef); 6] {
    use Command as K;
    [
        (click(K::Previous, K::Back5s, K::StartOfList), MenuRef::Previous),
        (click(K::Play, K::OpenFile, K::OpenUrl), MenuRef::Play),
        (Action::click(K::Pause), MenuRef::Pause),
        (click(K::Stop, K::StopFadeout, K::StopAfterCurrent), MenuRef::Stop),
        (click(K::Next, K::Fwd5s, K::EndOfList), MenuRef::Next),
        (click(K::OpenFile, K::OpenFolder, K::OpenUrl), MenuRef::Eject),
    ]
}

/// Options, minimize, shade and close on the main title bar.
fn title_buttons(shade_mode: bool) -> Vec<Element> {
    let t = Sheet::TitleBar;
    let shade = if shade_mode { m::UNSHADE } else { m::SHADE };
    vec![
        button(at(m::AT_OPTIONS, 9, 9), button_look(t, m::OPTIONS), Action::MenuOnPress(MenuRef::Main))
            .cursor(if shade_mode { C::MMenu } else { C::MainMenu })
            .tip("Main Menu"),
        button(at(m::AT_MINIMIZE, 9, 9), button_look(t, m::MINIMIZE), Action::click(Command::Minimize)).cursor(C::Min).tip("Minimize"),
        button(at(m::AT_SHADE, 9, 9), button_look(t, shade), Action::click(Command::ToggleWindowshade))
            .cursor(C::WinBut)
            .tip("Toggle Windowshade Mode"),
        button(at(m::AT_CLOSE, 9, 9), button_look(t, m::CLOSE), Action::click(Command::Exit)).cursor(C::Close).tip("Close"),
    ]
}

fn main_window(volume_h: i32, balance_h: i32) -> WindowDef {
    WindowDef {
        key: WindowKey::Main,
        placement: Placement::Free,
        normal: main_normal(volume_h, balance_h),
        shade: Some(main_shade()),
        bar: None,
        empty_press: EmptyPress::MoveIfEasy,
        wheel: Wheel::Volume(8),
    }
}

fn main_shade() -> Mode {
    let t = Sheet::TitleBar;
    let mut v = vec![
        image(Place::fixed(0, 0, 275, 14), Look::focus(paint(t, m::SHADE_ACTIVE), paint(t, m::SHADE_INACTIVE))),
        Element::new(Place::fixed(0, 0, 275, 14), Kind::Drag { double: Some(Command::ToggleWindowshade) }),
    ];
    v.extend(title_buttons(true));
    v.push(Element::new(at(m::AT_SHADE_TIME, 25, 6), Kind::Text(Text::ShortTime)).show(Cond::Unless(Flag::Stopped)));
    v.push(
        Element::new(
            at(m::AT_SHADE_POS, 17, 7),
            Kind::Slider(Slider {
                value: Value::Position,
                vertical: false,
                track: Some(single_frame(t, m::SHADE_POS_BG)),
                thumb: Look::plain(paint(t, m::SHADE_POS_THUMB)),
                thumb_len: m::SHADE_POS_THUMB.w,
                live: false,
                reset: None,
                wheel: 0,
                detent: 0,
            }),
        )
        .show(Cond::If(Flag::Seekable))
        .cursor(C::WSPosBar),
    );
    for (rc, (action, _)) in m::SHADE_TRANSPORT.iter().zip(transport()) {
        v.push(hot_spot(rect(*rc), action));
    }
    Mode { width: Dim::Fixed(m::SHADE_SIZE.0), height: Dim::Fixed(m::SHADE_SIZE.1), cursor: C::WSNormal, elements: v, shape: None }
}

fn single_frame(sheet: Sheet, src: Rect) -> Frames {
    Frames { picture: Picture::Sheet(sheet), first: src, step: (0, 0), count: 1, per_row: 1, map: FrameMap::Rising }
}

fn main_normal(volume_h: i32, balance_h: i32) -> Mode {
    let t = Sheet::TitleBar;
    let mut v = vec![
        image(Place::fixed(0, 0, 275, 116), Look::plain(paint(Sheet::Main, r(0, 0, 275, 116)))),
        image(Place::fixed(0, 0, 275, 14), Look::focus(paint(t, m::TITLE_ACTIVE), paint(t, m::TITLE_INACTIVE))),
        Element::new(Place::fixed(0, 0, 275, 14), Kind::Drag { double: Some(Command::ToggleWindowshade) }).cursor(C::TitleBar),
    ];
    v.extend(title_buttons(false));
    v.extend(clutter_bar());
    v.extend(status());
    v.push(
        Element::new(
            Place::fixed(m::TIME_AREA.x, m::TIME_AREA.y, m::TIME_AREA.w, m::TIME_AREA.h),
            Kind::Clock { align: Align::Left, font_size: None },
        )
        .show(Cond::Unless(Flag::Stopped)),
    );
    v.push(hot_spot(rect(m::TIME_AREA), Action::Press(Command::ToggleTimeMode)).menu(MenuRef::Time));
    let mq = m::MARQUEE;
    v.push(
        Element::new(
            Place::fixed(mq.x - 2, 24, mq.w + 4, 12),
            Kind::Marquee { area: Some(r(2, mq.y - 24, mq.w, mq.h)), double: Some(Command::FileInfo) },
        )
        .cursor(C::SongName)
        .tip("Song Title (right-click for options)")
        .menu(MenuRef::SongTitle),
    );
    v.push(Element::new(rect(m::KBPS), Kind::Text(Text::Kbps)));
    v.push(Element::new(rect(m::KHZ), Kind::Text(Text::Khz)));
    let ms = Sheet::MonoSter;
    for (pos, off, on, flag) in
        [(m::AT_STEREO, m::STEREO_OFF, m::STEREO_ON, Flag::Stereo), (m::AT_MONO, m::MONO_OFF, m::MONO_ON, Flag::Mono)]
    {
        let look = Look { normal: Some(paint(ms, off)), on: Some(paint(ms, on)), ..Look::default() };
        v.push(Element::new(at(pos, off.w, off.h), Kind::Image { look, fill: Fill::Once, lit: Some(flag) }));
    }
    v.push(Element::new(rect(m::VIS), Kind::Vis));
    v.push(hot_spot(rect(m::VIS), Action::Press(Command::CycleVisMode)).menu(MenuRef::Vis));
    v.push(seek_bar());
    v.push(volume_slider(Value::Volume, m::VOLUME_AREA, 0, volume_h, 0));
    v.push(volume_slider(Value::Balance, m::BALANCE_AREA, m::BALANCE_SRC_X, balance_h, 16));
    let cb = Sheet::CButtons;
    let buttons = [
        (m::PREV, m::AT_PREV),
        (m::PLAY, m::AT_PLAY),
        (m::PAUSE, m::AT_PAUSE),
        (m::STOP, m::AT_STOP),
        (m::NEXT, m::AT_NEXT),
        (m::EJECT, m::AT_EJECT),
    ];
    for ((b, pos), (action, menu)) in buttons.into_iter().zip(transport()) {
        v.push(button(at(pos, b.normal.w, b.normal.h), button_look(cb, b), action).menu(menu));
    }
    let sr = Sheet::ShufRep;
    for (tg, pos, flag, cmd, menu) in [
        (m::SHUFFLE, m::AT_SHUFFLE, Flag::Shuffle, Command::ToggleShuffle, Some(MenuRef::Shuffle)),
        (m::REPEAT, m::AT_REPEAT, Flag::Repeat, Command::ToggleRepeat, Some(MenuRef::Repeat)),
        (m::EQ, m::AT_EQ, Flag::EqWindow, Command::ToggleEqWindow, None),
        (m::PL, m::AT_PL, Flag::PlaylistWindow, Command::TogglePlaylistWindow, None),
    ] {
        let mut el =
            Element::new(at(pos, tg.off.w, tg.off.h), Kind::Toggle { look: toggle_look(sr, tg), flag, action: Action::click(cmd) });
        el.menu = menu;
        v.push(el);
    }
    Mode { width: Dim::Fixed(m::SIZE.0), height: Dim::Fixed(m::SIZE.1), cursor: C::Normal, elements: v, shape: None }
}

/// The O A I D V buttons; A and D light up while their option is on.
fn clutter_bar() -> Vec<Element> {
    let t = Sheet::TitleBar;
    let (cx, cy) = m::AT_CLUTTER;
    let mut v = vec![image(at(m::AT_CLUTTER, m::CLUTTER_BG.w, m::CLUTTER_BG.h), Look::plain(paint(t, m::CLUTTER_BG)))];
    let ends = [m::CLUTTER_OFFSETS[1], m::CLUTTER_OFFSETS[2], m::CLUTTER_OFFSETS[3], m::CLUTTER_OFFSETS[4], m::CLUTTER_BG.h];
    let tips = ["Options Menu", "Toggle Always-On-Top", "File Info Box", "Toggle Doublesize Mode", "Visualization Menu"];
    for i in 0..5 {
        // The first button also takes the bar's top edge.
        let top = if i == 0 { 0 } else { m::CLUTTER_OFFSETS[i] };
        let lit = paint(t, m::CLUTTER_SELECTED[i]).offset(0, m::CLUTTER_OFFSETS[i] - top);
        let place = Place::fixed(cx, cy + top, m::CLUTTER_BG.w, ends[i] - top);
        let look = Look { pressed: Some(lit), on: Some(lit), on_pressed: Some(lit), ..Look::default() };
        let kind = match i {
            0 => Kind::Button { look: Look::default(), action: Action::MenuOnPress(MenuRef::Options) },
            1 => Kind::Toggle { look, flag: Flag::AlwaysOnTop, action: Action::click(Command::ToggleAlwaysOnTop) },
            2 => Kind::Button { look, action: Action::click(Command::FileInfo) },
            3 => Kind::Toggle { look, flag: Flag::DoubleSize, action: Action::click(Command::ToggleDoubleSize) },
            _ => Kind::Button { look, action: Action::MenuOnRelease(MenuRef::Vis) },
        };
        let mut el = Element::new(place, kind).tip(tips[i]);
        if i == 4 {
            el.menu = Some(MenuRef::Vis);
        }
        v.push(el);
    }
    v
}

/// Play/pause/stop indicator and the "working" lamp beside it.
fn status() -> Vec<Element> {
    let pp = Sheet::PlayPaus;
    let ind = |src: Rect, flag| image(at(m::AT_PLAYPAUS, src.w, src.h), Look::plain(paint(pp, src))).show(Cond::If(flag));
    vec![
        ind(m::PLAYING, Flag::Playing),
        image(at(m::AT_WORK, m::WORKING.w, m::WORKING.h), Look::plain(paint(pp, m::WORKING))).show(Cond::If(Flag::Playing)),
        ind(m::PAUSED, Flag::Paused),
        ind(m::STOPPED, Flag::Stopped),
    ]
}

fn seek_bar() -> Element {
    let pb = Sheet::PosBar;
    Element::new(
        rect(m::POS_AREA),
        Kind::Slider(Slider {
            value: Value::Position,
            vertical: false,
            track: Some(single_frame(pb, m::POS_BG)),
            thumb: button_look(pb, m::POS_THUMB),
            thumb_len: m::POS_THUMB.normal.w,
            live: false,
            reset: None,
            wheel: 0,
            detent: 0,
        }),
    )
    .show(Cond::If(Flag::Seekable))
    .cursor(C::PosBar)
    .menu(MenuRef::Seek)
}

/// Volume or balance: 28 background frames 15 px apart, a thumb under
/// them if the bitmap has one.
fn volume_slider(value: Value, area: Rect, src_x: i32, sheet_h: i32, detent: i64) -> Element {
    let sheet = if value == Value::Volume { Sheet::Volume } else { Sheet::Balance };
    let thumb = if sheet_h > SLIDER_THUMB_BELOW {
        Look::button(paint(sheet, m::SLIDER_THUMB.normal).offset(0, 1), paint(sheet, m::SLIDER_THUMB.pressed).offset(0, 1))
    } else {
        Look::default()
    };
    let map = if value == Value::Volume { FrameMap::Rising } else { FrameMap::FromMiddle };
    Element::new(
        rect(area),
        Kind::Slider(Slider {
            value,
            vertical: false,
            track: Some(Frames {
                picture: Picture::Sheet(sheet),
                first: r(src_x, 0, area.w, area.h),
                step: (0, m::SLIDER_FRAME_STEP),
                count: m::SLIDER_FRAMES,
                per_row: 1,
                map,
            }),
            thumb,
            thumb_len: m::SLIDER_THUMB.normal.w,
            live: true,
            reset: None,
            wheel: 0,
            detent,
        }),
    )
    .cursor(C::VolBal)
}

fn eq_window() -> WindowDef {
    WindowDef {
        key: WindowKey::Eq,
        placement: Placement::Docked,
        normal: eq_normal(),
        shade: Some(eq_shade()),
        bar: None,
        empty_press: EmptyPress::Move,
        wheel: Wheel::Volume(8),
    }
}

/// Close and shade on the equalizer's title bar; only pressed paints.
fn eq_title_buttons(sheet: Sheet, close_pressed: Option<Rect>, shade_pressed: Option<Rect>) -> [Element; 2] {
    let overlay = |src: Option<Rect>| Look { pressed: src.map(|s| paint(sheet, s)), ..Look::default() };
    [
        button(at(e::AT_CLOSE, 9, 9), overlay(close_pressed), Action::click(Command::ToggleEqWindow)).cursor(C::EqClose),
        button(at(e::AT_SHADE, 9, 9), overlay(shade_pressed), Action::click(Command::ToggleEqShade)),
    ]
}

fn eq_shade() -> Mode {
    let x = Sheet::EqEx;
    let mut v = vec![
        image(Place::fixed(0, 0, 275, 14), Look::focus(paint(x, e::SHADE_ACTIVE), paint(x, e::SHADE_INACTIVE))),
        Element::new(Place::fixed(0, 0, 275, 14), Kind::Drag { double: Some(Command::ToggleEqShade) }).cursor(C::EqTitle),
    ];
    v.extend(eq_title_buttons(x, Some(e::SHADE_CLOSE_PRESSED), Some(e::UNSHADE_PRESSED)));
    Mode { width: Dim::Fixed(e::SHADE_SIZE.0), height: Dim::Fixed(e::SHADE_SIZE.1), cursor: C::EqNormal, elements: v, shape: None }
}

fn eq_normal() -> Mode {
    let s = Sheet::EqMain;
    let mut v = vec![
        image(rect(e::BG), Look::plain(paint(s, e::BG))),
        image(Place::fixed(0, 0, 275, 14), Look::focus(paint(s, e::TITLE_ACTIVE), paint(s, e::TITLE_INACTIVE))),
        Element::new(Place::fixed(0, 0, 275, 14), Kind::Drag { double: Some(Command::ToggleEqShade) }).cursor(C::EqTitle),
    ];
    v.extend(eq_title_buttons(s, Some(e::CLOSE.pressed), None));
    v.push(Element::new(
        at(e::AT_ON, 26, 12),
        Kind::Toggle { look: toggle_look(s, e::ON), flag: Flag::EqOn, action: Action::click(Command::ToggleEq) },
    ));
    v.push(Element::new(
        at(e::AT_AUTO, 32, 12),
        Kind::Toggle { look: toggle_look(s, e::AUTO), flag: Flag::EqAutoload, action: Action::click(Command::ToggleEqAutoload) },
    ));
    v.push(button(at(e::AT_PRESETS, 44, 12), button_look(s, e::PRESETS), Action::MenuOnPress(MenuRef::EqPresets)).menu(MenuRef::EqPresets));
    v.push(eq_slider(Value::Preamp, e::AT_PREAMP.0));
    for i in 0..10u8 {
        v.push(eq_slider(Value::EqBand(i), e::BAND_X0 + i as i32 * e::BAND_STEP));
    }
    v.push(Element::new(at(e::AT_GRAPH, e::GRAPH_BG.w, e::GRAPH_BG.h), Kind::EqGraph));
    Mode { width: Dim::Fixed(e::SIZE.0), height: Dim::Fixed(e::SIZE.1), cursor: C::EqNormal, elements: v, shape: None }
}

fn eq_slider(value: Value, x: i32) -> Element {
    let s = Sheet::EqMain;
    let first = e::slider_frame(0);
    let second_row = e::slider_frame(14);
    Element::new(
        Place::fixed(x, e::BAND_Y, e::SLIDER_W, e::SLIDER_H),
        Kind::Slider(Slider {
            value,
            vertical: true,
            track: Some(Frames {
                picture: Picture::Sheet(s),
                first,
                step: (e::slider_frame(1).x - first.x, second_row.y - first.y),
                count: 28,
                per_row: 14,
                map: FrameMap::Falling,
            }),
            thumb: Look::button(paint(s, e::THUMB.normal).offset(1, 0), paint(s, e::THUMB.pressed).offset(1, 0)),
            thumb_len: e::THUMB.normal.h,
            live: true,
            reset: Some(31),
            wheel: -2,
            detent: 0,
        }),
    )
    .cursor(C::EqSlid)
    .menu(MenuRef::EqPresets)
}

fn playlist_window() -> WindowDef {
    WindowDef {
        key: WindowKey::Playlist,
        placement: Placement::Docked,
        normal: playlist_normal(),
        shade: Some(playlist_shade()),
        bar: None,
        empty_press: EmptyPress::Nothing,
        wheel: Wheel::ScrollList,
    }
}

fn span(left: Pos, top: Pos, right: Pos, bottom: Pos) -> Place {
    Place { left, top, right, bottom }
}

use Pos::{Center, End, Start};

/// A paint at a spot measured from the right and/or bottom edge.
fn piece(left: Pos, top: Pos, look: Look, w: i32, h: i32) -> Element {
    image(span(left, top, shift(left, w), shift(top, h)), look)
}

fn shift(p: Pos, by: i32) -> Pos {
    match p {
        Start(o) => Start(o + by),
        Center(o) => Center(o + by),
        End(o) => End(o + by),
    }
}

fn tiles(place: Place, src: Rect) -> Element {
    Element::new(place, Kind::Image { look: Look::plain(paint(Sheet::PlEdit, src)), fill: Fill::Tile, lit: None })
}

fn pair_look((active, inactive): (Rect, Rect)) -> Look {
    Look::focus(paint(Sheet::PlEdit, active), paint(Sheet::PlEdit, inactive))
}

fn pl_overlay(src: Rect) -> Look {
    Look { pressed: Some(paint(Sheet::PlEdit, src)), ..Look::default() }
}

fn playlist_shade() -> Mode {
    let v = vec![
        piece(Start(0), Start(0), pair_look(p::SHADE_LEFT), 25, 14),
        tiles(span(Start(25), Start(0), End(-50), Start(14)), p::SHADE_TILE),
        piece(End(-50), Start(0), pair_look(p::SHADE_RIGHT), 50, 14),
        Element::new(span(Start(0), Start(0), End(0), Start(14)), Kind::Drag { double: Some(Command::TogglePlShade) }),
        button(span(End(-21), Start(3), End(-12), Start(12)), pl_overlay(p::SHADE_PRESSED), Action::click(Command::TogglePlShade))
            .cursor(C::PWinBut),
        button(span(End(-11), Start(3), End(-2), Start(12)), pl_overlay(p::CLOSE_PRESSED), Action::click(Command::TogglePlaylistWindow))
            .cursor(C::PClose),
        Element::new(span(Start(5), Start(4), End(-55), Start(10)), Kind::Text(Text::Title)),
    ];
    Mode { width: Dim::Free { min: p::MIN_SIZE.0, step: p::STEP.0 }, height: Dim::Fixed(14), cursor: C::PWSNorm, elements: v, shape: None }
}

fn playlist_normal() -> Mode {
    let mut v = vec![
        piece(Start(0), Start(0), pair_look(p::TOP_LEFT), 25, 20),
        Element::new(
            span(Start(25), Start(0), End(-25), Start(20)),
            Kind::Image { look: pair_look(p::TOP_TILE), fill: Fill::Tile, lit: None },
        ),
        piece(Center(-50), Start(0), pair_look(p::TITLE), 100, 20),
        piece(End(-25), Start(0), pair_look(p::TOP_RIGHT), 25, 20),
        tiles(span(Start(0), Start(20), Start(12), End(-38)), p::LEFT_TILE),
        tiles(span(End(-20), Start(20), End(0), End(-38)), p::RIGHT_TILE),
        tiles(span(Start(125), End(-38), End(-150), End(0)), p::BOTTOM_TILE),
        piece(Start(0), End(-38), Look::plain(paint(Sheet::PlEdit, p::BOTTOM_LEFT)), 125, 38),
        piece(End(-225), End(-38), Look::plain(paint(Sheet::PlEdit, p::VIS)), 75, 38).show(Cond::MinWidth(350)),
        piece(End(-150), End(-38), Look::plain(paint(Sheet::PlEdit, p::BOTTOM_RIGHT)), 150, 38),
        Element::new(span(Start(0), Start(0), End(0), Start(20)), Kind::Drag { double: Some(Command::TogglePlShade) }).cursor(C::PTBar),
        button(span(End(-11), Start(3), End(-2), Start(12)), pl_overlay(p::CLOSE_PRESSED), Action::click(Command::TogglePlaylistWindow))
            .cursor(C::PClose),
        button(span(End(-20), Start(3), End(-11), Start(12)), pl_overlay(p::SHADE_PRESSED), Action::click(Command::TogglePlShade))
            .cursor(C::PWinBut),
        Element::new(span(Start(12), Start(20), End(-20), End(-38)), Kind::List).menu(MenuRef::PlaylistRow),
        Element::new(
            span(End(-15), Start(20), End(-7), End(-38)),
            Kind::ListScroll {
                thumb: Look::button(paint(Sheet::PlEdit, p::SCROLL_THUMB.normal), paint(Sheet::PlEdit, p::SCROLL_THUMB.pressed)),
            },
        )
        .cursor(C::PVScroll),
        Element::new(span(End(-143), End(-28), End(-58), End(-22)), Kind::Text(Text::PlaylistTime)),
        Element::new(span(End(-82), End(-14), End(-52), End(-8)), Kind::Text(Text::PlaylistClock)).show(Cond::Unless(Flag::Stopped)),
        Element::new(span(End(-222), End(-28), End(-150), End(-10)), Kind::Vis).show(Cond::MinWidth(350)),
        Element::new(span(End(-20), End(-20), End(0), End(0)), Kind::Resize).cursor(C::PSize),
    ];
    use Command as K;
    let mini = [(6, 8, K::Previous), (14, 8, K::Play), (22, 9, K::Pause), (31, 8, K::Stop), (39, 8, K::Next), (48, 9, K::OpenFile)];
    for (x, w, cmd) in mini {
        v.push(hot_spot(span(End(-150 + x), End(-16), End(-150 + x + w), End(-7)), Action::click(cmd)));
    }
    v.extend(playlist_menus());
    Mode {
        width: Dim::Free { min: p::MIN_SIZE.0, step: p::STEP.0 },
        height: Dim::Free { min: p::MIN_SIZE.1, step: p::STEP.1 },
        cursor: C::PNormal,
        elements: v,
        shape: None,
    }
}

/// The ADD, REM, SEL, MISC and LIST buttons and their picture menus.
fn playlist_menus() -> Vec<Element> {
    use Command as K;
    let act = Action::click;
    let actions: [&[Action]; 5] = [
        &[act(K::PlAddFiles), act(K::PlAddFolder), act(K::PlAddUrl)],
        &[Action::MenuOnRelease(MenuRef::PlaylistRemoveMisc), act(K::PlRemoveSelected), act(K::PlCropSelected), act(K::ClearPlaylist)],
        &[act(K::SelectAll), act(K::SelectNone), act(K::InvertSelection)],
        &[Action::MenuOnRelease(MenuRef::PlaylistMisc), act(K::FileInfo), Action::MenuOnRelease(MenuRef::PlaylistSort)],
        &[act(K::PlaylistOpen), act(K::PlaylistSave), act(K::ClearPlaylist)],
    ];
    let lefts = [Start(14), Start(43), Start(72), Start(101), End(-44)];
    (0..5)
        .map(|i| {
            let items = p::menu_items(i)
                .iter()
                .zip(actions[i])
                .map(|(&(normal, pressed), &action)| PopupItem {
                    look: Look::button(paint(Sheet::PlEdit, normal), paint(Sheet::PlEdit, pressed)),
                    action,
                })
                .collect::<Vec<_>>();
            let bar = paint(Sheet::PlEdit, p::MENU_BARS[i]).offset(-3, -(items.len() as i32 - 1) * 18);
            let w = if i == 4 { 22 } else { 25 };
            Element::new(span(lefts[i], End(-30), shift(lefts[i], w), End(-12)), Kind::Popup { items, bar: Some(bar), above: false })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn paints(k: &Kind) -> Vec<Paint> {
        let look = |l: &Look| [l.normal, l.pressed, l.on, l.on_pressed, l.inactive].into_iter().flatten().collect::<Vec<_>>();
        match k {
            Kind::Image { look: l, .. } | Kind::Button { look: l, .. } | Kind::Toggle { look: l, .. } | Kind::ListScroll { thumb: l } => {
                look(l)
            }
            Kind::Slider(s) => look(&s.thumb),
            Kind::Popup { items, bar, .. } => items.iter().flat_map(|i| look(&i.look)).chain(*bar).collect(),
            _ => Vec::new(),
        }
    }

    #[test]
    fn every_paint_fits_its_sheet() {
        for w in layout(433, 433).windows {
            for mode in [Some(&w.normal), w.shade.as_ref()].into_iter().flatten() {
                for el in &mode.elements {
                    for pt in paints(&el.kind) {
                        let Picture::Sheet(sheet) = pt.picture else { panic!("{pt:?}") };
                        let (sw, sh) = sheet.size();
                        let s = pt.src;
                        assert!(s.x >= 0 && s.y >= 0 && s.x + s.w <= sw && s.y + s.h <= sh, "{:?} {:?}", w.key, pt);
                    }
                }
            }
        }
    }

    /// The last interactive element under a point, as the player picks it.
    fn hit(mode: &Mode, w: i32, h: i32, x: i32, y: i32) -> Option<&Element> {
        mode.elements.iter().rev().find(|el| el.interactive() && el.at.resolve(w, h).contains(x, y))
    }

    #[test]
    fn main_window_controls_are_where_winamp_has_them() {
        let l = layout(433, 433);
        let main = &l.window(WindowKey::Main).unwrap().normal;
        let tip = |x, y| hit(main, 275, 116, x, y).and_then(|e| e.tip.clone());
        assert_eq!(tip(9, 5).as_deref(), Some("Main Menu"));
        assert_eq!(tip(13, 22).as_deref(), Some("Options Menu"), "the first clutter button takes the bar's top");
        assert_eq!(tip(13, 36).as_deref(), Some("Toggle Always-On-Top"));
        assert_eq!(tip(13, 64).as_deref(), Some("Visualization Menu"));
        assert!(matches!(hit(main, 275, 116, 100, 5).unwrap().kind, Kind::Drag { .. }));
        assert!(matches!(hit(main, 275, 116, 120, 60).unwrap().kind, Kind::Slider(Slider { value: Value::Volume, .. })));
        assert!(matches!(
            hit(main, 275, 116, 40, 95).unwrap().kind,
            Kind::Button { action: Action::Click { plain: Command::Play, .. }, .. }
        ));
        assert!(hit(main, 275, 116, 50, 110).is_none());
    }

    #[test]
    fn short_volume_bitmaps_have_no_thumb() {
        let l = layout(420, 433);
        let main = &l.window(WindowKey::Main).unwrap().normal;
        let thumbs: Vec<bool> = main
            .elements
            .iter()
            .filter_map(|e| match &e.kind {
                Kind::Slider(s) if matches!(s.value, Value::Volume | Value::Balance) => Some(s.thumb.normal.is_some()),
                _ => None,
            })
            .collect();
        assert_eq!(thumbs, [false, true]);
    }

    #[test]
    fn playlist_controls_follow_the_window_size() {
        let l = layout(433, 433);
        let pl = &l.window(WindowKey::Playlist).unwrap().normal;
        for (w, h) in [(275, 116), (450, 290)] {
            assert!(matches!(
                hit(pl, w, h, w - 7, 6).unwrap().kind,
                Kind::Button { action: Action::Click { plain: Command::TogglePlaylistWindow, .. }, .. }
            ));
            assert!(matches!(hit(pl, w, h, w - 5, h - 5).unwrap().kind, Kind::Resize));
            assert!(matches!(hit(pl, w, h, 30, 30).unwrap().kind, Kind::List));
            assert!(matches!(hit(pl, w, h, w - 11, 40).unwrap().kind, Kind::ListScroll { .. }));
            assert!(matches!(hit(pl, w, h, w - 40, h - 20).unwrap().kind, Kind::Popup { .. }), "LIST button");
        }
    }
}
