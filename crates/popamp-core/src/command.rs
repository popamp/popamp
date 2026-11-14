//! User-level commands. Discriminants are the original Winamp command ids
//! so `/COMMAND=<id>` scripts keep working.
//!
//! `LibraryOpen` is Enter or a double click in the library: Play, or
//! Enqueue when `enqueuedef` is on; `LibraryOpenOther` (Shift+Enter) is the
//! other of the two.

macro_rules! commands {
    ($( $name:ident = $id:expr ),* $(,)?) => {
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
        #[repr(u32)]
        pub enum Command { $( $name = $id ),* }

        impl Command {
            pub fn from_id(id: u32) -> Option<Command> {
                match id { $( $id => Some(Command::$name), )* _ => None }
            }
            pub fn id(self) -> u32 { self as u32 }
            pub const ALL: &'static [Command] = &[ $( Command::$name ),* ];
        }
    };
}

commands! {
    Exit = 40001,
    Preferences = 40012,
    ToggleAlwaysOnTop = 40019,
    ToggleRepeat = 40022,
    ToggleShuffle = 40023,
    OpenFile = 40029,
    ToggleEqWindow = 40036,
    TimeElapsed = 40037,
    TimeRemaining = 40038,
    TogglePlaylistWindow = 40040,
    About = 40041,
    Previous = 40044,
    Play = 40045,
    Pause = 40046,
    Stop = 40047,
    Next = 40048,
    Forward5s = 40060,
    Rewind5s = 40061,
    Back5s = 40144,
    StopFadeout = 40147,
    Fwd5s = 40148,
    StartOfList = 40154,
    StopAfterCurrent = 40157,
    EndOfList = 40158,
    ToggleDoubleSize = 40165,
    InvertSelection = 40171,
    OpenUrl = 40185,
    ToggleEasyMove = 40186,
    OpenFolder = 40187,
    FileInfo = 40188,
    ToggleAutoscroll = 40189,
    SelectVisPlugin = 40191,
    ToggleVis = 40192,
    JumpToTime = 40193,
    JumpToFile = 40194,
    TenTracksForward = 40195,
    TenTracksBack = 40197,
    PlaylistOpen = 40202,
    PlaylistSave = 40204,
    SelectAll = 40205,
    SelectNone = 40207,
    SortByTitle = 40209,
    SortByFileName = 40210,
    SortByPath = 40211,
    RandomizeList = 40212,
    ReverseList = 40213,
    ClearPlaylist = 40214,
    SkinBrowser = 40219,
    RemoveMissing = 40222,
    PhysicallyRemove = 40223,
    ToggleEq = 40244,
    ToggleEqAutoload = 40245,
    EditPlaylistEntry = 40255,
    ToggleMainWindow = 40258,
    ToggleManualAdvance = 40395,
    // Playlist-editor button ids from the PL context menu.
    PlAddFiles = 1032,
    PlRemoveSelected = 1034,
    PlCropSelected = 1035,
    PlAddFolder = 1036,
    PlAddUrl = 1039,
    // Popamp-only commands (ids outside Winamp's range).
    PlayPause = 50001,
    VolumeUp = 50002,
    VolumeDown = 50003,
    Minimize = 50004,
    ToggleWindowshade = 50005,
    ToggleTimeMode = 50006,
    PlaySelected = 50007,
    MoveSelectionUp = 50008,
    MoveSelectionDown = 50009,
    EqReset = 50010,
    EqPresetLoad = 50011,
    EqPresetSave = 50012,
    EqPresetDelete = 50013,
    EqImportEqf = 50014,
    EqExportEqf = 50015,
    CycleVisMode = 50016,
    ToggleEqShade = 50017,
    TogglePlShade = 50018,
    BalanceLeft = 50019,
    BalanceRight = 50020,
    AddBookmark = 50021,
    BookmarkSelected = 50022,
    RemoveBookmark = 50023,
    EqLoadDefault = 50024,
    EqSaveDefault = 50025,
    PlGenerateHtml = 50026,
    SkinMuseum = 50027,
    Plugins = 50028,
    Gallery = 50029,
    AlbumArt = 50030,
    History = 50031,
    ScanReplayGain = 50032,
    ToggleLibraryWindow = 50033,
    LibraryPlay = 50034,
    LibraryEnqueue = 50035,
    LibraryAddFolder = 50036,
    LibraryRescan = 50037,
    LibraryRemoveFolder = 50038,
    LibraryOpen = 50039,
    LibraryOpenOther = 50040,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ids_round_trip() {
        for &c in Command::ALL {
            assert_eq!(Command::from_id(c.id()), Some(c));
        }
        assert_eq!(Command::from_id(40045), Some(Command::Play));
        assert_eq!(Command::from_id(1), None);
    }
}
