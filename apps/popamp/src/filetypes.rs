//! The files Popamp opens, and making it their default player.
//!
//! On Linux the choice is the user's `mimeapps.list`, as `xdg-mime
//! default` writes it. Windows lets only the user pick a default, in its
//! Settings, so there Popamp opens that page. On macOS the user picks one
//! in Finder, for now.

/// A kind of file, as Preferences offers them (on Windows, only the
/// tests of the installer use them).
#[cfg(any(linux_like, test))]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Group {
    Audio,
    Playlists,
    Skins,
}

#[cfg(any(linux_like, test))]
impl Group {
    pub const ALL: [Group; 3] = [Group::Audio, Group::Playlists, Group::Skins];

    #[cfg(linux_like)]
    pub fn label(self) -> &'static str {
        match self {
            Group::Audio => tr!("Audio files"),
            Group::Playlists => tr!("Playlists"),
            Group::Skins => tr!("Skins"),
        }
    }

    /// Its MIME types, as `popamp.desktop` lists them.
    #[cfg(linux_like)]
    pub fn mimes(self) -> &'static [&'static str] {
        match self {
            Group::Audio => &[
                "audio/mpeg",
                "audio/mp3",
                "audio/flac",
                "audio/x-flac",
                "audio/ogg",
                "audio/x-vorbis+ogg",
                "audio/vorbis",
                "audio/x-opus+ogg",
                "audio/opus",
                "audio/mp4",
                "audio/x-m4a",
                "audio/aac",
                "audio/wav",
                "audio/x-wav",
                "audio/x-aiff",
                "audio/x-matroska",
                "audio/midi",
                "audio/x-midi",
                "audio/x-mod",
                "audio/x-xm",
                "audio/x-s3m",
                "audio/x-it",
                "audio/x-stm",
            ],
            Group::Playlists => &[
                "audio/x-mpegurl",
                "audio/mpegurl",
                "application/vnd.apple.mpegurl",
                "audio/x-scpls",
                "application/xspf+xml",
                "application/x-cue",
            ],
            Group::Skins => &["application/x-popamp-skin", "application/x-popamp-vector-skin"],
        }
    }

    /// Its file name extensions, as the Windows installer registers them:
    /// every audio type but the video containers.
    #[cfg(test)]
    pub fn extensions(self) -> Vec<&'static str> {
        match self {
            Group::Audio => popamp_core::audio_extensions().filter(|e| !["mp4", "mkv", "webm"].contains(e)).collect(),
            Group::Playlists => vec!["m3u", "m3u8", "pls", "xspf", "cue"],
            Group::Skins => vec!["wsz", "psz"],
        }
    }
}

#[cfg(linux_like)]
pub use linux::Desktop;

#[cfg(linux_like)]
mod linux {
    use super::Group;
    use std::path::PathBuf;

    /// The desktop entry packages install, and the one written when none is.
    const ID: &str = "popamp.desktop";
    const ENTRY: &str = include_str!("../../../packaging/popamp.desktop");
    const SKIN_TYPES: &str = include_str!("../../../packaging/popamp-skin.xml");
    const SECTION: &str = "[Default Applications]";

    /// The XDG folders that say which program opens a file.
    pub struct Desktop {
        /// `XDG_CONFIG_HOME`, which holds `mimeapps.list`.
        pub config: PathBuf,
        /// `XDG_DATA_HOME`, where a desktop entry of our own goes.
        pub data: PathBuf,
        /// `XDG_DATA_DIRS`, where packages put theirs.
        pub dirs: Vec<PathBuf>,
        /// The program a desktop entry of our own starts.
        pub exe: Option<PathBuf>,
    }

    impl Desktop {
        pub fn from_env() -> Desktop {
            let home = std::env::home_dir().unwrap_or_else(|| PathBuf::from("."));
            let xdg = |var: &str, fallback: &str| {
                std::env::var_os(var).map(PathBuf::from).filter(|p| p.is_absolute()).unwrap_or_else(|| home.join(fallback))
            };
            let dirs = std::env::var_os("XDG_DATA_DIRS").filter(|v| !v.is_empty()).unwrap_or_else(|| "/usr/local/share:/usr/share".into());
            // An AppImage runs from a folder that is gone once it quits.
            let exe = std::env::var_os("APPIMAGE").map(PathBuf::from).or_else(|| std::env::current_exe().ok());
            Desktop {
                config: xdg("XDG_CONFIG_HOME", ".config"),
                data: xdg("XDG_DATA_HOME", ".local/share"),
                dirs: std::env::split_paths(&dirs).filter(|p| p.is_absolute()).collect(),
                exe,
            }
        }

        fn list_file(&self) -> PathBuf {
            self.config.join("mimeapps.list")
        }

        fn list(&self) -> String {
            std::fs::read_to_string(self.list_file()).unwrap_or_default()
        }

        /// Whether Popamp opens each group, in [`Group::ALL`] order.
        pub fn defaults(&self) -> [bool; 3] {
            let list = self.list();
            Group::ALL.map(|g| g.mimes().iter().all(|m| apps(&list, m).first() == Some(&ID)))
        }

        /// Makes Popamp the default for the groups chosen, and no longer
        /// for the others it was. Groups already as chosen are left alone.
        pub fn set(&self, chosen: [bool; 3]) -> Result<(), String> {
            let now = self.defaults();
            if chosen == now {
                return Ok(());
            }
            if chosen.iter().any(|&on| on) {
                self.ensure_entry().map_err(|e| tr!("File types: cannot add Popamp to the menu: {e}", e))?;
            }
            let mut list = self.list();
            for (n, g) in Group::ALL.into_iter().enumerate().filter(|&(n, _)| chosen[n] != now[n]) {
                list = with_default(&list, g.mimes(), chosen[n]);
            }
            let file = self.list_file();
            let tmp = file.with_extension("list.popamp-new");
            std::fs::create_dir_all(&self.config)
                .and_then(|()| std::fs::write(&tmp, list))
                .and_then(|()| std::fs::rename(&tmp, &file))
                .map_err(|e| tr!("File types: cannot write {file}: {e}", file = file.display(), e))
        }

        /// Writes a desktop entry for this program when no package did, so
        /// the list can name it.
        fn ensure_entry(&self) -> std::io::Result<()> {
            let installed = std::iter::once(&self.data).chain(&self.dirs).any(|d| d.join("applications").join(ID).is_file());
            let Some(exe) = self.exe.as_ref().filter(|_| !installed) else { return Ok(()) };
            let apps = self.data.join("applications");
            std::fs::create_dir_all(&apps)?;
            std::fs::write(apps.join(ID), entry_for(&exe.to_string_lossy()))?;
            let mime = self.data.join("mime");
            std::fs::create_dir_all(mime.join("packages"))?;
            std::fs::write(mime.join("packages").join("popamp-skin.xml"), SKIN_TYPES)?;
            // Skins are types of our own; without the tool they open as zips.
            let _ = std::process::Command::new("update-mime-database").arg(&mime).status();
            Ok(())
        }
    }

    /// The lines of the Default Applications section.
    fn section(list: &str) -> impl Iterator<Item = &str> {
        list.lines().map(str::trim).skip_while(|l| *l != SECTION).skip(1).take_while(|l| !l.starts_with('['))
    }

    /// The programs a list names for `mime`, the default first.
    fn apps<'a>(list: &'a str, mime: &str) -> Vec<&'a str> {
        section(list)
            .find_map(|l| l.split_once('=').filter(|(k, _)| k.trim() == mime))
            .map(|(_, v)| v.split(';').map(str::trim).filter(|a| !a.is_empty()).collect())
            .unwrap_or_default()
    }

    /// `list` with Popamp put first for `mimes`, or taken off them.
    fn with_default(list: &str, mimes: &[&str], on: bool) -> String {
        let line = |mime: &str| {
            let rest = apps(list, mime).into_iter().filter(|a| *a != ID);
            let apps: Vec<&str> = on.then_some(ID).into_iter().chain(rest).collect();
            (!apps.is_empty()).then(|| format!("{mime}={};", apps.join(";")))
        };
        let mut out: Vec<String> = Vec::new();
        let mut inside = false;
        let mut done = false;
        let add_rest = |out: &mut Vec<String>| {
            let at = out.iter().rposition(|l| !l.trim().is_empty()).map_or(0, |i| i + 1);
            let missing = mimes.iter().filter(|m| apps(list, m).is_empty()).filter_map(|m| line(m));
            out.splice(at..at, missing.collect::<Vec<_>>());
        };
        for raw in list.lines() {
            let t = raw.trim();
            if t.starts_with('[') {
                if inside {
                    add_rest(&mut out);
                    done = true;
                }
                inside = t == SECTION;
            } else if let Some(mime) = t.split_once('=').map(|(k, _)| k.trim()).filter(|k| inside && mimes.contains(k)) {
                out.extend(line(mime));
                continue;
            }
            out.push(raw.to_string());
        }
        if inside {
            add_rest(&mut out);
        } else if !done && on {
            if out.last().is_some_and(|l| !l.trim().is_empty()) {
                out.push(String::new());
            }
            out.push(SECTION.into());
            add_rest(&mut out);
        }
        let mut text = out.join("\n");
        text.push('\n');
        text
    }

    /// The packaged desktop entry, starting `exe` instead of `popamp`.
    fn entry_for(exe: &str) -> String {
        let quoted: String = exe.chars().flat_map(|c| ['"', '`', '$', '\\'].contains(&c).then_some('\\').into_iter().chain([c])).collect();
        ENTRY
            .lines()
            .map(|l| l.strip_prefix("Exec=popamp").map_or(l.to_string(), |rest| format!("Exec=\"{quoted}\"{rest}")))
            .collect::<Vec<_>>()
            .join("\n")
            + "\n"
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        const AUDIO: &[&str] = &["audio/mpeg", "audio/flac"];

        #[test]
        fn popamp_goes_first_and_others_stay_after_it() {
            let list = "[Added Associations]\naudio/mpeg=vlc.desktop;\n\n[Default Applications]\naudio/mpeg=vlc.desktop;rhythmbox.desktop;\ntext/plain=gedit.desktop;\n";
            let on = with_default(list, AUDIO, true);
            assert_eq!(
                on,
                "[Added Associations]\naudio/mpeg=vlc.desktop;\n\n[Default Applications]\naudio/mpeg=popamp.desktop;vlc.desktop;rhythmbox.desktop;\ntext/plain=gedit.desktop;\naudio/flac=popamp.desktop;\n"
            );
            let off = with_default(&on, AUDIO, false);
            assert_eq!(off, "[Added Associations]\naudio/mpeg=vlc.desktop;\n\n[Default Applications]\naudio/mpeg=vlc.desktop;rhythmbox.desktop;\ntext/plain=gedit.desktop;\n");
        }

        #[test]
        fn a_missing_section_is_added_and_a_later_one_kept() {
            assert_eq!(with_default("", AUDIO, true), "[Default Applications]\naudio/mpeg=popamp.desktop;\naudio/flac=popamp.desktop;\n");
            let list = "[Default Applications]\ntext/plain=gedit.desktop;\n\n[Removed Associations]\naudio/flac=popamp.desktop;\n";
            assert_eq!(
                with_default(list, AUDIO, true),
                "[Default Applications]\ntext/plain=gedit.desktop;\naudio/mpeg=popamp.desktop;\naudio/flac=popamp.desktop;\n\n[Removed Associations]\naudio/flac=popamp.desktop;\n"
            );
            assert_eq!(with_default("[Added Associations]\n", AUDIO, false), "[Added Associations]\n");
        }

        fn temp(name: &str) -> PathBuf {
            let dir = std::env::temp_dir().join(format!("popamp-filetypes-{}-{name}", std::process::id()));
            let _ = std::fs::remove_dir_all(&dir);
            dir
        }

        fn desktop(root: &std::path::Path) -> Desktop {
            Desktop {
                config: root.join("config"),
                data: root.join("data"),
                dirs: vec![root.join("usr")],
                exe: Some(PathBuf::from("/opt/My Apps/popamp")),
            }
        }

        #[test]
        fn chosen_groups_become_defaults_and_an_entry_is_written_when_none_is_installed() {
            let root = temp("entry");
            let d = desktop(&root);
            assert_eq!(d.defaults(), [false, false, false]);
            d.set([true, false, true]).unwrap();
            assert_eq!(d.defaults(), [true, false, true]);
            let entry = std::fs::read_to_string(root.join("data/applications/popamp.desktop")).unwrap();
            assert!(
                entry.contains("\nExec=\"/opt/My Apps/popamp\" %U\n") && entry.contains("Exec=\"/opt/My Apps/popamp\" /NEXT"),
                "{entry}"
            );
            assert!(root.join("data/mime/packages/popamp-skin.xml").is_file());
            d.set([false, false, true]).unwrap();
            assert_eq!(d.defaults(), [false, false, true]);
            let _ = std::fs::remove_dir_all(root);
        }

        #[test]
        fn a_packaged_entry_is_used_as_it_is() {
            let root = temp("packaged");
            std::fs::create_dir_all(root.join("usr/applications")).unwrap();
            std::fs::write(root.join("usr/applications/popamp.desktop"), ENTRY).unwrap();
            let d = desktop(&root);
            d.set([true, true, true]).unwrap();
            assert_eq!(d.defaults(), [true, true, true]);
            assert!(!root.join("data/applications").exists());
            let _ = std::fs::remove_dir_all(root);
        }

        #[test]
        fn the_desktop_entry_lists_every_type() {
            let line = ENTRY.lines().find_map(|l| l.strip_prefix("MimeType=")).unwrap();
            let listed: Vec<&str> = line.split(';').filter(|m| !m.is_empty()).collect();
            let ours: Vec<&str> = Group::ALL.iter().flat_map(|g| g.mimes().iter().copied()).collect();
            assert_eq!(listed, ours);
        }
    }
}

/// Opens the Settings page where the user picks the default apps, on
/// Popamp's own entry when Windows knows it.
#[cfg(windows)]
pub fn open_default_apps() {
    use windows_sys::Win32::System::Registry::{RegGetValueW, HKEY_CURRENT_USER, RRF_RT_REG_SZ};
    let wide = |s: &str| s.encode_utf16().chain([0]).collect::<Vec<u16>>();
    let (key, value) = (wide(r"Software\RegisteredApplications"), wide("Popamp"));
    // A per-user install registers Popamp for the user, one for all
    // users for the machine.
    // SAFETY: NUL-terminated names; no data is read back.
    let user = unsafe {
        RegGetValueW(
            HKEY_CURRENT_USER,
            key.as_ptr(),
            value.as_ptr(),
            RRF_RT_REG_SZ,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            std::ptr::null_mut(),
        )
    } == 0;
    let page =
        if user { "ms-settings:defaultapps?registeredAppUser=Popamp" } else { "ms-settings:defaultapps?registeredAppMachine=Popamp" };
    if let Err(e) = std::process::Command::new("explorer").arg(page).spawn() {
        log::warn!("cannot open the default apps: {e}");
    }
}

#[cfg(test)]
mod tests {
    use super::Group;

    #[test]
    fn the_installer_registers_every_extension() {
        let iss = include_str!("../../../packaging/windows/popamp.iss");
        for g in Group::ALL {
            for ext in g.extensions() {
                let supported = format!(r#"SupportedTypes"; ValueType: string; ValueName: ".{ext}""#);
                let progid = if g == Group::Skins { "Popamp.Skin" } else { "Popamp.Audio" };
                let capability =
                    format!(r#"Capabilities\FileAssociations"; ValueType: string; ValueName: ".{ext}"; ValueData: "{progid}""#);
                assert!(iss.contains(&supported), "no SupportedTypes for .{ext}");
                assert!(iss.contains(&capability), "no capability for .{ext}");
            }
        }
    }
}
