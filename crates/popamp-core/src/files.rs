//! Writing Popamp's own text files: the library, the stats, the history.

use std::io::Write;
use std::path::Path;

/// Writes `text` as the whole of `path`, through a file beside it, so a
/// crash midway leaves the old file.
pub fn replace(path: &Path, text: &str) -> std::io::Result<()> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let mut tmp = path.as_os_str().to_owned();
    tmp.push(".tmp");
    std::fs::write(&tmp, text)?;
    std::fs::rename(&tmp, path)
}

/// Adds `line` at the end of `path`, making the file when there is none.
pub fn append(path: &Path, line: &str) -> std::io::Result<()> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    std::fs::OpenOptions::new().create(true).append(true).open(path)?.write_all(line.as_bytes())
}
