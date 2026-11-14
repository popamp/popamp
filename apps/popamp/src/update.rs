//! The check for a newer Popamp: at start, at most once a day, Popamp
//! reads popamp.app/latest.json and, when it names a newer version, says
//! so. It never downloads or installs anything itself. The request
//! carries nothing but Popamp's version, in the User-Agent.

use crate::model::{AppEvent, Events, Model};
use std::path::Path;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

const LATEST: &str = "https://popamp.app/latest.json";
const SITE: &str = "https://popamp.app/";
const RUNNING: &str = env!("CARGO_PKG_VERSION");

/// A release named by latest.json.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Release {
    pub version: String,
    /// Its page on the site.
    pub url: String,
}

/// Reads latest.json: `{"version": "1.0.1", "url": "https://popamp.app/…"}`.
/// A version that is not plain numbers or a page off the site is refused.
pub fn parse(json: &[u8]) -> Result<Release, String> {
    let v: serde_json::Value = serde_json::from_slice(json).map_err(|e| e.to_string())?;
    let text = |key: &str| v[key].as_str().map(str::to_string);
    let version = text("version").filter(|s| numbers(s).is_some()).ok_or("no version")?;
    let url = text("url").filter(|u| u.starts_with(SITE)).ok_or("no page on the site")?;
    Ok(Release { version, url })
}

/// "1.0.2" as [1, 0, 2]; None for anything else, a pre-release too.
fn numbers(version: &str) -> Option<Vec<u32>> {
    version.split('.').map(|n| n.parse().ok().filter(|_| n.bytes().all(|b| b.is_ascii_digit()))).collect()
}

/// True when `latest` is a later version than `running`; 1.0 and 1.0.0
/// are the same.
pub fn newer(latest: &str, running: &str) -> bool {
    let (Some(mut a), Some(mut b)) = (numbers(latest), numbers(running)) else { return false };
    let len = a.len().max(b.len());
    a.resize(len, 0);
    b.resize(len, 0);
    a > b
}

/// Today, as days since 1970 in UTC.
fn today() -> u32 {
    let secs = SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_secs());
    (secs / 86_400) as u32
}

/// How this copy of Popamp is brought up to date.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Channel {
    /// From the site, by hand: Windows, an AppImage, a build of one's own.
    Site,
    /// Installed from a .deb or .rpm, under /usr: the package manager.
    Packages,
}

impl Channel {
    pub fn of_this_copy() -> Channel {
        let exe = std::env::current_exe().unwrap_or_default();
        Channel::of(&exe, std::env::var_os("APPIMAGE").is_some())
    }

    fn of(exe: &Path, appimage: bool) -> Channel {
        if cfg!(target_os = "linux") && !appimage && exe.starts_with("/usr") {
            Channel::Packages
        } else {
            Channel::Site
        }
    }
}

/// Asks the site on a thread of its own; the answer comes as
/// [`AppEvent::Update`], and an error, e.g. no network, is only logged.
fn start(proxy: String, events: Events) {
    let spawned = std::thread::Builder::new().name("popamp-update".into()).spawn(move || {
        let found = popamp_audio::stream::fetch_bytes(LATEST, &proxy, 64 << 10).and_then(|b| parse(&b));
        events.send(AppEvent::Update(found));
    });
    if let Err(e) = spawned {
        log::warn!("cannot start the update check: {e}");
    }
}

/// Opens `url` in the desktop's browser.
pub fn open_page(url: &str) -> std::io::Result<()> {
    use std::process::Command;
    let mut c = if cfg!(windows) {
        let mut c = Command::new("rundll32");
        c.arg("url.dll,FileProtocolHandler");
        c
    } else if cfg!(target_os = "macos") {
        Command::new("open")
    } else {
        Command::new("xdg-open")
    };
    c.arg(url).spawn().map(drop)
}

impl Model {
    /// Starts the check when it is on and was not answered today.
    pub fn check_for_update(&mut self) {
        let s = self.settings();
        if s.update_check && s.update_day != today() {
            start(s.proxy.clone(), self.events().clone());
        }
    }

    /// The site answered: a newer version, not skipped, is told of.
    pub fn on_update(&mut self, found: Result<Release, String>) {
        let release = match found {
            Ok(r) => r,
            Err(e) => return log::info!("update check: {e}"),
        };
        self.settings_mut().update_day = today();
        self.offer_update(release, RUNNING);
    }

    fn offer_update(&mut self, release: Release, running: &str) {
        if !newer(&release.version, running) || release.version == self.settings().update_skip {
            return;
        }
        let text = format!("POPAMP {} IS OUT, SEE THE MAIN MENU", release.version);
        self.transient = Some((text, Instant::now() + Duration::from_secs(8)));
        self.update = Some(release);
    }

    /// The main menu's item for a newer version, when there is one.
    pub fn update_menu(&self) -> Option<crate::menu::MenuItem> {
        use crate::menu::{MenuAction, MenuItem};
        let r = self.update.as_ref()?;
        let get = match Channel::of_this_copy() {
            Channel::Site => MenuItem::action("Download…", "", MenuAction::UpdateDownload),
            Channel::Packages => MenuItem::label("Update through your package manager"),
        };
        let skip = MenuItem::action("Skip this version", "", MenuAction::UpdateSkip);
        Some(MenuItem::sub(&format!("Popamp {} is out", r.version), vec![get, skip]))
    }

    pub fn update_download(&mut self) {
        let Some(r) = &self.update else { return };
        if let Err(e) = open_page(&r.url) {
            log::warn!("{}: {e}", r.url);
            self.flash("CANNOT OPEN THE BROWSER");
        }
    }

    pub fn update_skip(&mut self) {
        if let Some(r) = self.update.take() {
            self.settings_mut().update_skip = r.version;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_latest_json_and_refuses_what_is_not_a_release_on_the_site() {
        let ok = parse(br#"{"version":"1.0.1","url":"https://popamp.app/download"}"#);
        assert_eq!(ok, Ok(Release { version: "1.0.1".into(), url: "https://popamp.app/download".into() }));
        for bad in [
            r#"{"version":"1.1.0-rc1","url":"https://popamp.app/download"}"#,
            r#"{"version":"1.0.1","url":"https://example.org/popamp.exe"}"#,
            r#"{"url":"https://popamp.app/download"}"#,
            "<html>",
        ] {
            assert!(parse(bad.as_bytes()).is_err(), "{bad}");
        }
    }

    #[test]
    fn only_a_later_version_is_newer() {
        assert!(newer("1.0.1", "1.0.0") && newer("1.10.0", "1.9.9") && newer("2", "1.9"));
        assert!(!newer("1.0", "1.0.0") && !newer("0.9.1", "1.0.0") && !newer("x", "1.0.0"));
    }

    #[test]
    fn a_copy_under_usr_is_updated_by_the_package_manager() {
        let packages = if cfg!(target_os = "linux") { Channel::Packages } else { Channel::Site };
        assert_eq!(Channel::of(Path::new("/usr/bin/popamp"), false), packages);
        assert_eq!(Channel::of(Path::new("/usr/bin/popamp"), true), Channel::Site, "an AppImage mounts anywhere");
        assert_eq!(Channel::of(Path::new("/home/me/popamp/popamp"), false), Channel::Site);
    }

    #[test]
    fn a_newer_version_is_offered_until_skipped() {
        let mut m = Model::for_tests();
        let release = |v: &str| Release { version: v.into(), url: SITE.into() };
        m.offer_update(release("1.0.0"), "1.0.0");
        assert!(m.update_menu().is_none(), "the running one is not news");
        m.offer_update(release("1.0.1"), "1.0.0");
        assert_eq!(m.update_menu().map(|i| i.label), Some("Popamp 1.0.1 is out".into()));
        assert!(m.transient.as_ref().is_some_and(|t| t.0.starts_with("POPAMP 1.0.1 IS OUT")));
        m.update_skip();
        assert!(m.update_menu().is_none());
        m.offer_update(release("1.0.1"), "1.0.0");
        assert!(m.update_menu().is_none(), "a skipped version stays skipped");
        m.offer_update(release("1.0.2"), "1.0.0");
        assert!(m.update_menu().is_some(), "the next one is news again");
    }
}
