//! The words a vector skin file uses for commands, player states, menus
//! and the like. Most are the code name in kebab case (`ToggleShuffle` is
//! `toggle-shuffle`); cursors use their classic file name without `.cur`.

use crate::cursors::CursorKind;
use crate::layout::{Align, EmptyPress, Fill, Flag, FrameMap, MenuRef, Placement, Text, Value, WindowKey};
use crate::sprites::Sheet;
use popamp_core::config::BarEdge;
use popamp_core::Command;
use serde::de::{Deserialize, Deserializer, Error};

pub trait Named: Sized + Copy + 'static {
    /// What the thing is called in error messages.
    const WHAT: &'static str;
    fn all() -> Vec<Self>;
    fn name(self) -> String;
}

/// A `T` read from its name.
#[derive(Debug, Clone, Copy)]
pub struct N<T>(pub T);

impl<'de, T: Named> Deserialize<'de> for N<T> {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let word = String::deserialize(d)?;
        T::all().into_iter().find(|v| v.name() == word).map(N).ok_or_else(|| {
            let names: Vec<String> = T::all().into_iter().map(Named::name).collect();
            D::Error::custom(format!("unknown {} `{word}`; it can be one of: {}", T::WHAT, names.join(", ")))
        })
    }
}

/// `ToggleShuffle` -> `toggle-shuffle`.
fn kebab(code: &str) -> String {
    let mut out = String::new();
    for (i, c) in code.chars().enumerate() {
        if c.is_ascii_uppercase() && i > 0 {
            out.push('-');
        }
        out.push(c.to_ascii_lowercase());
    }
    out
}

/// Names `$t` by its variants' code names.
macro_rules! named {
    ($t:ty, $what:literal, $all:expr) => {
        impl Named for $t {
            const WHAT: &'static str = $what;
            fn all() -> Vec<Self> {
                $all.to_vec()
            }
            fn name(self) -> String {
                kebab(&format!("{self:?}"))
            }
        }
    };
}

named!(Command, "command", Command::ALL);
named!(WindowKey, "window", WindowKey::ALL);
named!(BarEdge, "edge", [BarEdge::Top, BarEdge::Bottom]);
named!(Fill, "fill", [Fill::Once, Fill::Tile]);
named!(Align, "alignment", [Align::Left, Align::Center, Align::Right]);
named!(FrameMap, "frame order", [FrameMap::Rising, FrameMap::Falling, FrameMap::FromMiddle]);
named!(EmptyPress, "empty-spot action", [EmptyPress::Nothing, EmptyPress::Move, EmptyPress::MoveIfEasy]);
impl Named for Placement {
    const WHAT: &'static str = "placement";
    fn all() -> Vec<Self> {
        vec![Placement::Free, Placement::Docked, Placement::Bar(BarEdge::Top), Placement::Bar(BarEdge::Bottom)]
    }
    fn name(self) -> String {
        match self {
            Placement::Free => "free",
            Placement::Docked => "docked",
            Placement::Bar(BarEdge::Top) => "bar-top",
            Placement::Bar(BarEdge::Bottom) => "bar-bottom",
        }
        .into()
    }
}

named!(Text, "text", [Text::Kbps, Text::Khz, Text::ShortTime, Text::Title, Text::PlaylistTime, Text::PlaylistClock]);
named!(
    Flag,
    "player state",
    [
        Flag::Playing,
        Flag::Paused,
        Flag::Stopped,
        Flag::Seekable,
        Flag::Stereo,
        Flag::Mono,
        Flag::Shuffle,
        Flag::Repeat,
        Flag::EqWindow,
        Flag::PlaylistWindow,
        Flag::AlwaysOnTop,
        Flag::DoubleSize,
        Flag::EqOn,
        Flag::EqAutoload,
    ]
);
named!(
    MenuRef,
    "menu",
    [
        MenuRef::Main,
        MenuRef::Options,
        MenuRef::Vis,
        MenuRef::EqPresets,
        MenuRef::PlaylistRow,
        MenuRef::PlaylistMisc,
        MenuRef::PlaylistSort,
        MenuRef::PlaylistRemoveMisc,
        MenuRef::Previous,
        MenuRef::Play,
        MenuRef::Pause,
        MenuRef::Stop,
        MenuRef::Next,
        MenuRef::Eject,
        MenuRef::Seek,
        MenuRef::Repeat,
        MenuRef::Shuffle,
        MenuRef::Time,
        MenuRef::SongTitle,
    ]
);

impl Named for CursorKind {
    const WHAT: &'static str = "cursor";
    fn all() -> Vec<Self> {
        CursorKind::ALL.to_vec()
    }
    fn name(self) -> String {
        self.file_name().trim_end_matches(".cur").to_string()
    }
}

/// Sheets by their classic file name without `.bmp`.
impl Named for Sheet {
    const WHAT: &'static str = "sheet";
    fn all() -> Vec<Self> {
        Sheet::ALL.to_vec()
    }
    fn name(self) -> String {
        self.file_name().trim_end_matches(".bmp").to_string()
    }
}

/// EQ bands are `eq-band-1` (60 Hz) to `eq-band-10` (16 kHz).
impl Named for Value {
    const WHAT: &'static str = "value";
    fn all() -> Vec<Self> {
        let plain = [Value::Volume, Value::Balance, Value::Position, Value::Preamp];
        plain.into_iter().chain((0..10).map(Value::EqBand)).collect()
    }
    fn name(self) -> String {
        match self {
            Value::EqBand(i) => format!("eq-band-{}", i + 1),
            v => kebab(&format!("{v:?}")),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn read<T: Named>(word: &str) -> Result<T, String> {
        N::<T>::deserialize(serde::de::value::StrDeserializer::<serde::de::value::Error>::new(word)).map(|n| n.0).map_err(|e| e.to_string())
    }

    #[test]
    fn words_name_the_things() {
        assert_eq!(read::<Command>("toggle-shuffle"), Ok(Command::ToggleShuffle));
        assert_eq!(read::<Value>("eq-band-10"), Ok(Value::EqBand(9)));
        assert_eq!(read::<CursorKind>("pwsnorm"), Ok(CursorKind::PWSNorm));
        assert_eq!(read::<FrameMap>("from-middle"), Ok(FrameMap::FromMiddle));
    }

    #[test]
    fn unknown_words_list_the_choices() {
        let e = read::<Fill>("stretch").unwrap_err();
        assert_eq!(e, "unknown fill `stretch`; it can be one of: once, tile");
    }
}
