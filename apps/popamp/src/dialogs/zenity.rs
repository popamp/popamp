//! Dialogs through `zenity`, or `kdialog` where only that is installed.

use super::{DialogError, Request};
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

#[derive(Clone, Copy, PartialEq, Eq)]
enum Backend {
    Zenity,
    KDialog,
    None,
}

fn backend() -> Backend {
    static B: std::sync::OnceLock<Backend> = std::sync::OnceLock::new();
    *B.get_or_init(|| {
        let has = |bin: &str| std::env::var_os("PATH").is_some_and(|p| std::env::split_paths(&p).any(|d| d.join(bin).is_file()));
        if has("zenity") {
            Backend::Zenity
        } else if has("kdialog") {
            Backend::KDialog
        } else {
            Backend::None
        }
    })
}

/// What the dialog program printed, none when cancelled, or why it failed.
fn run(cmd: &mut Command) -> Result<Option<String>, DialogError> {
    let name = cmd.get_program().to_string_lossy().into_owned();
    let out = cmd.output().map_err(|e| DialogError::Failed(format!("cannot run {name}: {e}")))?;
    outcome(&name, &out)
}

/// How a run of the dialog program `name` ended. Both programs leave with
/// 1 when cancelled or answered no; they also print warnings that mean
/// nothing, so only another end is a failure, told by its last message.
fn outcome(name: &str, out: &Output) -> Result<Option<String>, DialogError> {
    match out.status.code() {
        Some(0) => Ok(Some(String::from_utf8_lossy(&out.stdout).trim_end_matches('\n').to_string())),
        Some(1) => Ok(None),
        _ => {
            let err = String::from_utf8_lossy(&out.stderr);
            let why = err.lines().rev().map(str::trim).find(|l| !l.is_empty()).unwrap_or("no message");
            Err(DialogError::Failed(format!("{name} failed ({}): {why}", out.status)))
        }
    }
}

/// Runs `req`: the chosen values, none when cancelled, or why no dialog
/// came up.
pub fn execute(req: &Request) -> Result<Vec<String>, DialogError> {
    let b = backend();
    let lines = |s: Result<Option<String>, DialogError>| -> Result<Vec<String>, DialogError> {
        Ok(s?.map(|s| s.lines().filter(|l| !l.is_empty()).map(str::to_string).collect()).unwrap_or_default())
    };
    let yes = |s: Result<Option<String>, DialogError>| -> Result<Vec<String>, DialogError> {
        Ok(s?.map(|_| vec!["yes".into()]).unwrap_or_default())
    };
    match (b, req) {
        (Backend::None, _) => Err(DialogError::Missing),
        (Backend::Zenity, Request::Files { title, multiple, filter, start }) => {
            let mut c = Command::new("zenity");
            c.args(["--file-selection", "--title", title, "--separator", "\n"]);
            if *multiple {
                c.arg("--multiple");
            }
            if !filter.is_empty() {
                c.arg(format!("--file-filter={} | {}", tr!("Media files"), filter.join(" ")));
                c.arg(format!("--file-filter={} | *", tr!("All files")));
            }
            if let Some(s) = start {
                c.arg(format!("--filename={}/", s.display()));
            }
            lines(run(&mut c))
        }
        (Backend::Zenity, Request::Folder { title, multiple, start }) => lines(run(&mut zenity_folder(title, *multiple, start.as_deref()))),
        (Backend::Zenity, Request::Save { title, name, start }) => {
            let mut c = Command::new("zenity");
            c.args(["--file-selection", "--save", "--confirm-overwrite", "--title", title]);
            let initial = start.as_ref().map(|s| s.join(name)).unwrap_or_else(|| PathBuf::from(name));
            c.arg(format!("--filename={}", initial.display()));
            lines(run(&mut c))
        }
        (Backend::Zenity, Request::Entry { title, text, initial }) => {
            lines(run(Command::new("zenity").args(["--entry", "--title", title, "--text", text, "--entry-text", initial])))
        }
        (Backend::Zenity, Request::List { title, column, rows }) => {
            let mut c = Command::new("zenity");
            c.args(["--list", "--title", title, "--width", "520", "--height", "480", "--print-column", "1", "--hide-column", "1"]);
            c.args(["--column", "#", "--column", column]);
            for (i, r) in rows.iter().enumerate() {
                c.arg(i.to_string());
                c.arg(r);
            }
            lines(run(&mut c))
        }
        (Backend::Zenity, Request::Info { title, text }) => {
            run(Command::new("zenity").args(["--info", "--no-markup", "--title", title, "--text", text]))?;
            Ok(vec![])
        }
        (Backend::Zenity, Request::Question { title, text }) => {
            yes(run(Command::new("zenity").args(["--question", "--no-markup", "--title", title, "--text", text])))
        }
        (Backend::KDialog, Request::Files { title, multiple, filter, start }) => {
            let mut c = Command::new("kdialog");
            c.args(["--title", title, "--getopenfilename"]);
            c.arg(start.as_ref().map(|p| p.display().to_string()).unwrap_or_else(|| ".".into()));
            c.arg(format!("{}|{}", filter.join(" "), tr!("Media files")));
            if *multiple {
                c.args(["--multiple", "--separate-output"]);
            }
            lines(run(&mut c))
        }
        // kdialog picks one folder at a time.
        (Backend::KDialog, Request::Folder { title, start, .. }) => {
            let start = start.as_ref().map(|p| p.display().to_string()).unwrap_or_else(|| ".".into());
            lines(run(Command::new("kdialog").args(["--title", title, "--getexistingdirectory", &start])))
        }
        (Backend::KDialog, Request::Save { title, name, start }) => {
            let initial = start.as_ref().map(|s| s.join(name)).unwrap_or_else(|| PathBuf::from(name));
            lines(run(Command::new("kdialog").args(["--title", title, "--getsavefilename", &initial.display().to_string()])))
        }
        (Backend::KDialog, Request::Entry { title, text, initial }) => {
            lines(run(Command::new("kdialog").args(["--title", title, "--inputbox", text, initial])))
        }
        (Backend::KDialog, Request::List { title, column, rows }) => {
            let mut c = Command::new("kdialog");
            c.args(["--title", title, "--menu", column]);
            for (i, r) in rows.iter().enumerate() {
                c.arg(i.to_string());
                c.arg(r);
            }
            lines(run(&mut c))
        }
        (Backend::KDialog, Request::Info { title, text }) => {
            run(Command::new("kdialog").args(["--title", title, "--msgbox", text]))?;
            Ok(vec![])
        }
        (Backend::KDialog, Request::Question { title, text }) => {
            yes(run(Command::new("kdialog").args(["--title", title, "--yesno", text])))
        }
    }
}

/// zenity's folder dialog, open inside `start`. Given a folder as
/// `--filename`, GTK opens the folder above it with it selected; started
/// in the folder instead, it opens there, and OK with nothing selected
/// gives that folder.
fn zenity_folder(title: &str, multiple: bool, start: Option<&Path>) -> Command {
    let mut c = Command::new("zenity");
    c.args(["--file-selection", "--directory", "--title", title, "--separator", "\n"]);
    if multiple {
        c.arg("--multiple");
    }
    // A folder that is gone would keep zenity from starting at all.
    if let Some(s) = start.filter(|s| s.is_dir()) {
        c.current_dir(s);
    }
    c
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::process::ExitStatusExt;
    use std::process::{ExitStatus, Output};

    fn exited(code: i32, stdout: &str, stderr: &str) -> Output {
        Output { status: ExitStatus::from_raw(code << 8), stdout: stdout.into(), stderr: stderr.into() }
    }

    #[test]
    fn a_choice_is_what_was_printed() {
        assert_eq!(outcome("zenity", &exited(0, "/a.mp3\n/b.mp3\n", "")), Ok(Some("/a.mp3\n/b.mp3".into())));
    }

    #[test]
    fn exit_one_is_cancel_or_no() {
        assert_eq!(outcome("zenity", &exited(1, "", "Gtk-WARNING: noise")), Ok(None));
    }

    #[test]
    fn another_exit_is_a_failure_with_its_last_message() {
        let Err(DialogError::Failed(why)) = outcome("zenity", &exited(255, "", "Gtk-WARNING: noise\ncannot open display\n\n")) else {
            panic!("not a failure")
        };
        assert!(why.starts_with("zenity") && why.ends_with("cannot open display"), "{why}");
    }

    #[test]
    fn a_kill_is_a_failure() {
        assert!(outcome("kdialog", &Output { status: ExitStatus::from_raw(9), stdout: vec![], stderr: vec![] }).is_err());
    }

    #[test]
    fn the_folder_dialog_opens_inside_its_start_folder() {
        let here = std::env::temp_dir();
        let c = zenity_folder("Add", true, Some(&here));
        assert_eq!(c.get_current_dir(), Some(here.as_path()));
        assert!(c.get_args().all(|a| !a.to_string_lossy().starts_with("--filename")), "a --filename opens the folder above");
        assert!(c.get_args().any(|a| a == "--multiple"));
        assert_eq!(zenity_folder("Add", false, Some(Path::new("/popamp-gone"))).get_current_dir(), None);
    }
}
