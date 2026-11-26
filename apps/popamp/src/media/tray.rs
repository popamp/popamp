//! The tray icon on Unix: a StatusNotifierItem, which KDE, Xfce, Cinnamon,
//! MATE and GNOME's AppIndicator extension show. A click brings Popamp up,
//! a middle click plays or pauses, the wheel sets the volume, and the menu
//! has the transport buttons.

use super::{now_playing, tray_menu, Sender, Snapshot};
use crate::cli::Remote;
use ksni::blocking::{Handle, TrayMethods};
use ksni::menu::{CheckmarkItem, MenuItem, StandardItem};
use popamp_core::Command;
use std::sync::Arc;

/// How far one notch of the wheel moves the volume, in percent.
const WHEEL_STEP: i32 = 2;

struct Item {
    send: Sender,
    snap: Snapshot,
    icons: Arc<Vec<ksni::Icon>>,
    /// KDE sends the wheel turned up as a positive delta, GNOME as negative.
    up_is_positive: bool,
}

impl ksni::Tray for Item {
    fn id(&self) -> String {
        "popamp".into()
    }

    fn title(&self) -> String {
        crate::edition::NAME.into()
    }

    fn icon_pixmap(&self) -> Vec<ksni::Icon> {
        self.icons.to_vec()
    }

    fn tool_tip(&self) -> ksni::ToolTip {
        ksni::ToolTip { title: crate::edition::NAME.into(), description: now_playing(&self.snap), ..Default::default() }
    }

    fn activate(&mut self, _x: i32, _y: i32) {
        (self.send)(vec![Remote::Show]);
    }

    fn secondary_activate(&mut self, _x: i32, _y: i32) {
        (self.send)(vec![Remote::Cmd(Command::PlayPause)]);
    }

    fn scroll(&mut self, delta: i32, orientation: ksni::Orientation) {
        if orientation == ksni::Orientation::Vertical && delta != 0 {
            let up = (delta > 0) == self.up_is_positive;
            (self.send)(vec![Remote::VolumeStep(if up { WHEEL_STEP } else { -WHEEL_STEP })]);
        }
    }

    fn menu(&self) -> Vec<MenuItem<Item>> {
        let item = |entry: super::Entry| -> MenuItem<Item> {
            let remote = entry.remote;
            let activate = Box::new(move |item: &mut Item| (item.send)(vec![remote.clone()]));
            match entry.checked {
                Some(checked) => CheckmarkItem { label: entry.label.into(), checked, activate, ..Default::default() }.into(),
                None => StandardItem { label: entry.label.into(), activate, ..Default::default() }.into(),
            }
        };
        tray_menu(&self.snap).into_iter().map(|e| e.map_or(MenuItem::Separator, item)).collect()
    }
}

/// The logo, drawn for each size the tray may want: a host picks the
/// nearest, so small trays get the small logo, not the big one shrunk.
fn icons() -> Vec<ksni::Icon> {
    let pngs: [&[u8]; 5] = [
        include_bytes!("../../../../assets/logo/png/popamp-16.png"),
        include_bytes!("../../../../assets/logo/png/popamp-24.png"),
        include_bytes!("../../../../assets/logo/png/popamp-32.png"),
        include_bytes!("../../../../assets/logo/png/popamp-48.png"),
        include_bytes!("../../../../assets/logo/png/popamp-64.png"),
    ];
    pngs.iter().filter_map(|png| image::load_from_memory(png).ok()).map(|img| argb(&img.to_rgba8())).collect()
}

/// RGBA pixels as the ARGB32 in network byte order the spec asks for.
fn argb(img: &image::RgbaImage) -> ksni::Icon {
    let data = img.pixels().flat_map(|p| [p[3], p[0], p[1], p[2]]).collect();
    ksni::Icon { width: img.width() as i32, height: img.height() as i32, data }
}

/// An icon on show; it goes when dropped.
struct Shown(Handle<Item>);

impl Drop for Shown {
    fn drop(&mut self) {
        self.0.shutdown().wait();
    }
}

/// The tray icon while the user wants one.
pub struct Tray {
    send: Sender,
    shown: Option<Option<Shown>>,
    icons: Arc<Vec<ksni::Icon>>,
}

impl Tray {
    pub fn new(send: Sender) -> Tray {
        Tray { send, shown: None, icons: Arc::new(icons()) }
    }

    /// Shows, updates or takes down the icon to match `snap`. A desktop
    /// with no tray is asked again only after the setting is turned off
    /// and on.
    pub fn update(&mut self, snap: &Snapshot) {
        let (send, icons) = (&self.send, &self.icons);
        crate::hold::update(&mut self.shown, snap.tray, || show(send.clone(), icons.clone()));
        if let Some(Some(Shown(handle))) = &self.shown {
            handle.update(|item| item.snap = snap.clone());
        }
    }
}

fn show(send: Sender, icons: Arc<Vec<ksni::Icon>>) -> Option<Shown> {
    let kde = std::env::var("XDG_CURRENT_DESKTOP").is_ok_and(|d| d.split(':').any(|d| d.eq_ignore_ascii_case("KDE")));
    let item = Item { send, snap: Snapshot::default(), icons, up_is_positive: kde };
    // Flatpak lets an app own no StatusNotifierItem-PID-ID name; the
    // watcher takes the item by its connection's own name instead.
    item.disable_dbus_name(crate::flatpak::sandboxed()).spawn().map(Shown).map_err(|e| log::info!("no tray icon: {e}")).ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use ksni::Tray as _;
    use parking_lot::Mutex;

    fn item(snap: Snapshot) -> (Item, Arc<Mutex<Vec<Remote>>>) {
        let sent = Arc::new(Mutex::new(Vec::new()));
        let log = sent.clone();
        let send: Sender = Arc::new(move |r| log.lock().extend(r));
        (Item { send, snap, icons: Arc::new(Vec::new()), up_is_positive: false }, sent)
    }

    /// Clicks the menu entry labelled `label`.
    fn click(item: &mut Item, label: &str) {
        let entry = item.menu().into_iter().find_map(|m| match m {
            MenuItem::Standard(s) if s.label == label => Some(s.activate),
            MenuItem::Checkmark(c) if c.label == label => Some(c.activate),
            _ => None,
        });
        entry.expect(label)(item);
    }

    #[test]
    fn the_menu_and_the_icon_drive_the_player() {
        let (mut item, sent) = item(Snapshot { shuffle: true, ..Snapshot::default() });
        click(&mut item, "Next");
        click(&mut item, "Shuffle");
        click(&mut item, "Exit");
        item.activate(0, 0);
        item.secondary_activate(0, 0);
        item.scroll(-1, ksni::Orientation::Vertical);
        item.scroll(3, ksni::Orientation::Vertical);
        item.scroll(1, ksni::Orientation::Horizontal);
        let want = [
            Remote::Cmd(Command::Next),
            Remote::Shuffle(Some(false)),
            Remote::Quit,
            Remote::Show,
            Remote::Cmd(Command::PlayPause),
            Remote::VolumeStep(2),
            Remote::VolumeStep(-2),
        ];
        assert_eq!(*sent.lock(), want);
        sent.lock().clear();
        item.up_is_positive = true;
        item.scroll(5, ksni::Orientation::Vertical);
        assert_eq!(*sent.lock(), [Remote::VolumeStep(2)], "KDE's way up");
    }

    #[test]
    fn the_menu_has_separators_and_check_marks() {
        let (item, _) = item(Snapshot::default());
        let kinds: String = item
            .menu()
            .iter()
            .map(|m| match m {
                MenuItem::Separator => '-',
                MenuItem::Checkmark(_) => 'v',
                _ => 'o',
            })
            .collect();
        assert_eq!(kinds, "o-ooooo-vv-o");
    }

    #[test]
    fn every_logo_size_is_an_icon() {
        let icons = icons();
        assert_eq!(icons.iter().map(|i| i.width).collect::<Vec<_>>(), [16, 24, 32, 48, 64]);
        let one = argb(&image::RgbaImage::from_pixel(1, 1, image::Rgba([1, 2, 3, 4])));
        assert_eq!(one.data, [4, 1, 2, 3], "alpha first");
    }
}
