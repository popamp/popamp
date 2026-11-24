//! Native dialogs, run off the UI thread: `zenity` (or `kdialog`) on
//! Linux, the system's own on Windows. Results come back to the event loop
//! as [`DialogResult`]s.

use std::path::PathBuf;

#[cfg(target_os = "macos")]
mod osascript;
#[cfg(linux_like)]
mod zenity;
#[cfg(target_os = "macos")]
use osascript::execute;
#[cfg(any(windows, test))]
mod windows;
#[cfg(windows)]
use windows::execute;

/// Why no dialog came up; only the Linux programs ever say.
#[derive(Debug, PartialEq)]
#[cfg_attr(not(linux_like), allow(dead_code))]
pub enum DialogError {
    /// Neither zenity nor kdialog is installed.
    Missing,
    /// The dialog program could not run, or failed: its message.
    Failed(String),
}

impl DialogError {
    /// Short enough for the main window's display.
    pub fn short(&self) -> &'static str {
        match self {
            DialogError::Missing => "DIALOGS NEED ZENITY",
            DialogError::Failed(_) => "DIALOG FAILED, SEE THE LOG",
        }
    }
}

impl std::fmt::Display for DialogError {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self {
            DialogError::Missing => f.write_str("neither zenity nor kdialog is installed"),
            DialogError::Failed(why) => f.write_str(why),
        }
    }
}

/// Runs `req` and waits for it: the chosen values, none when cancelled, or
/// why no dialog came up. On Linux that is up to programs that may be
/// missing; the system's own dialogs log their rare failures themselves.
pub fn show(req: &Request) -> Result<Vec<String>, DialogError> {
    #[cfg(linux_like)]
    return zenity::execute(req);
    #[cfg(not(linux_like))]
    Ok(execute(req))
}

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
    /// Folders to take into the media library.
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
    /// Asks before writing the picture at `image` into `paths` as their
    /// cover.
    WriteCover {
        image: String,
        paths: Vec<String>,
    },
    /// Asks before deleting the smart playlist at `index`.
    DeleteSmart {
        index: usize,
    },
    /// The name of a new plain playlist.
    NewPlaylist,
    /// The new name of the plain playlist at `index`.
    RenamePlaylist {
        index: usize,
    },
    /// Asks before deleting the plain playlist at `index`.
    DeletePlaylist {
        index: usize,
    },
    /// Asks before deleting a plug-in's folder.
    #[cfg(feature = "plugins")]
    RemovePlugin {
        dir: PathBuf,
        id: Option<String>,
        name: String,
    },
}

#[derive(Debug, Clone)]
pub struct DialogResult {
    pub kind: DialogKind,
    /// Selected paths/strings; empty when cancelled.
    pub values: Vec<String>,
}

/// The request for a dialog; built on the UI thread, executed elsewhere.
#[derive(Debug, Clone)]
pub enum Request {
    Files { title: String, multiple: bool, filter: Vec<String>, start: Option<PathBuf> },
    Folder { title: String, multiple: bool, start: Option<PathBuf> },
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
