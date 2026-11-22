//! Dialogs on macOS, through `osascript`: each kind of request is a fixed
//! AppleScript, and every value (titles, names, paths, addresses) goes in
//! as an argument, never as script text. Like `zenity`, it runs off the UI
//! thread; Cancel ends the script with an error, which reads as nothing
//! chosen.

use super::Request;
use std::path::Path;
use std::process::Command;

const FILES: &str = r#"on run argv
    activate
    set {t, multi, startDir} to {item 1 of argv, (item 2 of argv is "1"), item 3 of argv}
    set exts to {}
    if (count of argv) > 3 then set exts to items 4 thru -1 of argv
    if startDir is "" then
        if exts is {} then
            set picked to choose file with prompt t multiple selections allowed multi
        else
            set picked to choose file with prompt t of type exts multiple selections allowed multi
        end if
    else
        set loc to POSIX file startDir
        if exts is {} then
            set picked to choose file with prompt t default location loc multiple selections allowed multi
        else
            set picked to choose file with prompt t of type exts default location loc multiple selections allowed multi
        end if
    end if
    if class of picked is not list then set picked to {picked}
    set out to ""
    repeat with f in picked
        set out to out & POSIX path of f & linefeed
    end repeat
    return out
end run"#;

const FOLDER: &str = r#"on run argv
    activate
    set {t, multi, startDir} to {item 1 of argv, (item 2 of argv is "1"), item 3 of argv}
    if startDir is "" then
        set picked to choose folder with prompt t multiple selections allowed multi
    else
        set picked to choose folder with prompt t default location (POSIX file startDir) multiple selections allowed multi
    end if
    if class of picked is not list then set picked to {picked}
    set out to ""
    repeat with f in picked
        set out to out & POSIX path of f & linefeed
    end repeat
    return out
end run"#;

const SAVE: &str = r#"on run argv
    activate
    if item 3 of argv is "" then
        set f to choose file name with prompt (item 1 of argv) default name (item 2 of argv)
    else
        set f to choose file name with prompt (item 1 of argv) default name (item 2 of argv) default location (POSIX file (item 3 of argv))
    end if
    return POSIX path of f
end run"#;

const ENTRY: &str = r#"on run argv
    activate
    return text returned of (display dialog (item 2 of argv) with title (item 1 of argv) default answer (item 3 of argv))
end run"#;

const LIST: &str = r#"on run argv
    activate
    set picked to choose from list (items 3 thru -1 of argv) with title (item 1 of argv) with prompt (item 2 of argv)
    if picked is false then error number -128
    return item 1 of picked
end run"#;

const INFO: &str = r#"on run argv
    activate
    display dialog (item 2 of argv) with title (item 1 of argv) buttons {"OK"} default button 1
    return ""
end run"#;

const QUESTION: &str = r#"on run argv
    activate
    display dialog (item 2 of argv) with title (item 1 of argv) buttons {"No", "Yes"} default button "Yes" cancel button "No"
    return "yes"
end run"#;

/// Runs `req`; the chosen values, or none when cancelled.
pub fn execute(req: &Request) -> Vec<String> {
    let (source, args) = script(req);
    match Command::new("osascript").arg("-e").arg(source).args(&args).output() {
        Ok(out) if out.status.success() => answer(req, &String::from_utf8_lossy(&out.stdout)),
        Ok(out) => {
            // Cancel is error -128; anything else is worth knowing about.
            let err = String::from_utf8_lossy(&out.stderr);
            if !err.contains("-128") {
                log::warn!("dialog failed: {}", err.trim());
            }
            Vec::new()
        }
        Err(e) => {
            log::warn!("cannot run osascript: {e}");
            Vec::new()
        }
    }
}

/// The script for `req` and the arguments it reads.
fn script(req: &Request) -> (&'static str, Vec<String>) {
    // A start folder that went away (an unplugged drive) would keep the
    // dialog from opening at all: then there is none.
    let dir =
        |p: &Option<std::path::PathBuf>| p.as_deref().filter(|p| p.is_dir()).map(Path::to_string_lossy).unwrap_or_default().into_owned();
    match req {
        Request::Files { title, multiple, filter, start } => {
            let mut args = vec![title.clone(), if *multiple { "1" } else { "0" }.into(), dir(start)];
            args.extend(filter.iter().filter_map(|f| f.strip_prefix("*.")).filter(|e| !e.contains('*')).map(str::to_lowercase));
            (FILES, args)
        }
        Request::Folder { title, multiple, start } => (FOLDER, vec![title.clone(), if *multiple { "1" } else { "0" }.into(), dir(start)]),
        Request::Save { title, name, start } => (SAVE, vec![title.clone(), name.clone(), dir(start)]),
        Request::Entry { title, text, initial } => (ENTRY, vec![title.clone(), text.clone(), initial.clone()]),
        Request::List { title, column, rows } => (LIST, [title.clone(), column.clone()].into_iter().chain(rows.iter().cloned()).collect()),
        Request::Info { title, text } => (INFO, vec![title.clone(), text.clone()]),
        Request::Question { title, text } => (QUESTION, vec![title.clone(), text.clone()]),
    }
}

/// What a script printed, as `zenity` would give it: paths, one path, the
/// typed text, a row's number, or `yes`.
fn answer(req: &Request, stdout: &str) -> Vec<String> {
    let text = stdout.strip_suffix('\n').unwrap_or(stdout);
    let one = |v: &str| if v.is_empty() { Vec::new() } else { vec![v.to_string()] };
    match req {
        Request::Files { .. } => text.lines().filter(|l| !l.is_empty()).map(str::to_string).collect(),
        // A folder's POSIX path ends with a slash; zenity's does not.
        Request::Folder { .. } => text
            .lines()
            .map(|l| l.strip_suffix('/').filter(|l| !l.is_empty()).unwrap_or(l))
            .filter(|l| !l.is_empty())
            .map(str::to_string)
            .collect(),
        Request::Save { .. } | Request::Entry { .. } => one(text),
        Request::List { rows, .. } => rows.iter().position(|r| r == text).map(|i| vec![i.to_string()]).unwrap_or_default(),
        Request::Info { .. } => Vec::new(),
        Request::Question { .. } => {
            if text == "yes" {
                vec!["yes".into()]
            } else {
                Vec::new()
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn values_go_in_as_arguments_never_as_script() {
        let req = Request::Entry { title: "Open URL".into(), text: "Address:".into(), initial: "http://a/?x=\"1\"&y=\\".into() };
        let (source, args) = script(&req);
        assert!(!source.contains("http://a"), "{source}");
        assert!(args.iter().any(|a| a == "http://a/?x=\"1\"&y=\\"), "{args:?}");
    }

    #[test]
    fn file_filters_become_types() {
        let req = Request::Files { title: "Open".into(), multiple: true, filter: vec!["*.mp3".into(), "*.FLAC".into()], start: None };
        let (_, args) = script(&req);
        assert!(args.contains(&"mp3".to_string()) && args.contains(&"flac".to_string()), "{args:?}");
    }

    #[test]
    fn a_list_answers_with_the_row_number() {
        let req = Request::List { title: "Presets".into(), column: "Name".into(), rows: vec!["Rock".into(), "Pop".into()] };
        assert_eq!(answer(&req, "Pop\n"), ["1"]);
    }

    #[test]
    fn several_folders_come_back_without_their_slashes() {
        let req = Request::Folder { title: "Add".into(), multiple: true, start: None };
        assert_eq!(script(&req).1, ["Add", "1", ""]);
        assert_eq!(answer(&req, "/Music/A/\n/Music/B/\n/\n"), ["/Music/A", "/Music/B", "/"]);
    }

    #[test]
    fn a_cancelled_dialog_gives_nothing() {
        for req in [
            Request::Question { title: "Remove".into(), text: "Delete from disk?".into() },
            Request::Files { title: "Open".into(), multiple: false, filter: vec![], start: None },
        ] {
            assert!(answer(&req, "").is_empty());
        }
    }

    // AppleScript is only read when it runs; compiling each one catches a
    // mistake without showing a dialog.
    #[test]
    fn every_script_compiles() {
        let out = std::env::temp_dir().join(format!("popamp-osascript-{}.scpt", std::process::id()));
        for (name, source) in
            [("files", FILES), ("folder", FOLDER), ("save", SAVE), ("entry", ENTRY), ("list", LIST), ("info", INFO), ("question", QUESTION)]
        {
            let run = Command::new("osacompile").arg("-e").arg(source).arg("-o").arg(&out).output().unwrap();
            assert!(run.status.success(), "{name}: {}", String::from_utf8_lossy(&run.stderr));
        }
        let _ = std::fs::remove_file(out);
    }

    // A folder that went away (an unplugged drive) would stop the dialog
    // from opening at all.
    #[test]
    fn a_start_folder_that_is_gone_is_left_out() {
        let req = Request::Folder { title: "Add folder".into(), multiple: false, start: Some("/Volumes/popamp-gone/Music".into()) };
        assert_eq!(script(&req).1, ["Add folder", "0", ""]);
    }
}
