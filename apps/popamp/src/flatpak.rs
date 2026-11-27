//! Running inside a Flatpak sandbox: Flatpak updates it, and some desktop
//! services must be reached differently there.

/// True inside a Flatpak, which always provides /.flatpak-info.
pub fn sandboxed() -> bool {
    std::path::Path::new("/.flatpak-info").exists()
}

/// The name the desktop knows Popamp by: its .desktop file and icon, the
/// windows' Wayland app_id and X11 WM_CLASS. In a Flatpak, the app ID they
/// are exported under.
#[cfg(linux_like)]
pub fn desktop_id() -> &'static str {
    static NAME: std::sync::OnceLock<String> = std::sync::OnceLock::new();
    NAME.get_or_init(|| std::env::var("FLATPAK_ID").unwrap_or_else(|_| "popamp".into()))
}
