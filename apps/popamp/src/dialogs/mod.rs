//! Native dialogs, run off the UI thread: `zenity` (or `kdialog`) on
//! Linux, the system's own on Windows. Results come back to the event loop
//! as [`DialogResult`]s.

use std::path::PathBuf;

#[cfg(linux_like)]
mod zenity;
#[cfg(linux_like)]
pub use zenity::execute;
#[cfg(target_os = "macos")]
mod osascript;
#[cfg(target_os = "macos")]
pub use osascript::execute;
#[cfg(any(windows, test))]
mod windows;
#[cfg(windows)]
pub use windows::execute;

#[derive(Debug, Clone)]
pub enum DialogKind {
    /// Open files; `play` replaces the playlist, otherwise appends.
    OpenFiles {
        play: bool,
    },
    OpenFolder {
        play: bool,
    },
    OpenUrl {
        play: bool,
    },
    LoadPlaylist,
    /// A folder to take into the media library.
    LibraryFolder,
    SavePlaylist,
    SaveHtml,
    JumpToTime,
    EditEntry {
        id: u64,
    },
    EqLoadPreset {
        auto: bool,
    },
    EqSavePreset {
        auto: bool,
    },
    EqDeletePreset {
        auto: bool,
    },
    RemoveBookmark,
    EqImport,
    EqExport,
    InstallSkin,
    Info,
    /// A picture for the cover in File info, from `dir` on.
    PickCover {
        dir: Option<std::path::PathBuf>,
    },
    ConfirmPhysicalRemove {
        paths: Vec<String>,
    },
}

#[derive(Debug, Clone)]
pub struct DialogResult {
    pub kind: DialogKind,
    /// Selected paths/strings; empty when cancelled.
    pub values: Vec<String>,
}

/// The request for a dialog; built on the UI thread, executed elsewhere.
// macOS reads none of it until its dialogs are there.
#[cfg_attr(target_os = "macos", allow(dead_code))]
#[derive(Debug, Clone)]
pub enum Request {
    Files { title: String, multiple: bool, filter: Vec<String>, start: Option<PathBuf> },
    Folder { title: String, start: Option<PathBuf> },
    Save { title: String, name: String, start: Option<PathBuf> },
    Entry { title: String, text: String, initial: String },
    List { title: String, column: String, rows: Vec<String> },
    Info { title: String, text: String },
    Question { title: String, text: String },
}

/// Parses `m:ss`, `h:mm:ss` or plain seconds.
pub fn parse_time(s: &str) -> Option<u64> {
    let parts: Vec<&str> = s.trim().split(':').collect();
    if parts.is_empty() || parts.len() > 3 {
        return None;
    }
    let mut secs = 0u64;
    for p in &parts {
        secs = secs.checked_mul(60)?.checked_add(p.trim().parse::<u64>().ok()?)?;
    }
    Some(secs * 1000)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn times() {
        assert_eq!(parse_time("1:30"), Some(90_000));
        assert_eq!(parse_time("45"), Some(45_000));
        assert_eq!(parse_time("1:00:01"), Some(3_601_000));
        assert_eq!(parse_time("x"), None);
        assert_eq!(parse_time("1:2:3:4"), None);
    }
}
