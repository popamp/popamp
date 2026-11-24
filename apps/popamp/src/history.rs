//! The History window (Ctrl+H): what was played, newest first.

use crate::model::Model;
use crate::panels::{button, search, Act};
use crate::widgets::{Event, Form, Rect, Widget, BUTTON, ROW_H};
use jiff::tz::TimeZone;
use jiff::Timestamp;
use popamp_core::history::Played;

/// Searches the history as you type; Enter plays the selected track.
pub struct HistoryPanel {
    pub(crate) form: Form<Act>,
    search: usize,
    list: usize,
    /// The plays the rows show, in row order.
    shown: Vec<Played>,
    /// The newest play when the rows were made.
    newest: Option<Played>,
}

impl HistoryPanel {
    pub fn new(model: &Model) -> HistoryPanel {
        let (w, list_h) = (520, 4 + 18 * ROW_H);
        let mut form = Form::new(w, 10 + 24 + 8 + list_h + 10 + BUTTON.1 + 10);
        form.add(Rect::new(10, 10, 70, 24), Widget::Label { text: tr!("Search for:").into(), dim: false });
        let search = form.add(Rect::new(80, 10, w - 90, 24), Widget::Input { text: String::new(), cursor: 0 });
        let list = form.add(Rect::new(10, 42, w - 20, list_h), Widget::List { rows: vec![], selected: None, scroll: 0 });
        let by = 42 + list_h + 10;
        button(&mut form, w - 2 * BUTTON.0 - 16, by, tr!("Play"), Act::Ok, true);
        button(&mut form, w - BUTTON.0 - 10, by, tr!("Close"), Act::Close, false);
        let mut p = HistoryPanel { form, search, list, shown: vec![], newest: None };
        p.refilter(model);
        p
    }

    /// Lists the plays matching the search, newest first.
    fn refilter(&mut self, model: &Model) {
        let all: Vec<&Played> = model.history.newest_first().collect();
        let matching = search(all.iter().map(|p| p.title.as_str()), self.form.text(self.search));
        self.shown = matching.into_iter().map(|i| all[i].clone()).collect();
        self.newest = all.first().map(|p| (*p).clone());
        let (now, tz) = (Timestamp::now().as_second(), TimeZone::system());
        let rows = self.shown.iter().map(|p| format!("{}   {}", when(p.at, now, &tz), p.title)).collect();
        self.form.set_rows(self.list, rows, Some(0));
    }

    pub fn handle(&mut self, ev: Event<Act>, model: &mut Model) -> bool {
        match ev {
            Event::Edited => {
                self.refilter(model);
                true
            }
            Event::Clicked(Act::Ok) | Event::Activated => match self.form.selected(self.list).and_then(|r| self.shown.get(r)) {
                Some(p) => {
                    model.play_again(p);
                    false
                }
                None => true,
            },
            Event::Clicked(_) | Event::Cancel => false,
            _ => true,
        }
    }

    /// Shows a track that started while the window is open.
    pub fn refresh(&mut self, model: &Model) -> bool {
        if model.history.newest_first().next() == self.newest.as_ref() {
            return false;
        }
        self.refilter(model);
        true
    }
}

/// When a play was, as the list shows it: `Today 14:03`, `Yesterday
/// 09:10` or `2026-11-02 21:45`, in the local time zone `tz`.
fn when(at: u64, now: i64, tz: &TimeZone) -> String {
    let zoned = |secs: i64| Timestamp::from_second(secs).unwrap_or(Timestamp::UNIX_EPOCH).to_zoned(tz.clone());
    let (then, today) = (zoned(at as i64), zoned(now).date());
    let time = then.strftime("%H:%M");
    match then.date() {
        d if d == today => tr!("Today {time}", time),
        d if today.yesterday().is_ok_and(|y| y == d) => tr!("Yesterday {time}", time),
        _ => then.strftime("%Y-%m-%d %H:%M").to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::panels::tests::{press, type_text};
    use crate::panels::{Panel, PanelKind};
    use crate::widgets::Key;

    #[test]
    fn plays_read_as_today_yesterday_or_a_date() {
        let utc = TimeZone::UTC;
        let now = 1_794_144_449; // 2026-11-08 13:27:29Z
        assert_eq!(when(1_794_144_449 - 3600, now, &utc), "Today 12:27");
        assert_eq!(when(1_794_144_449 - 86_400, now, &utc), "Yesterday 13:27");
        assert_eq!(when(1_794_144_449 - 3 * 86_400, now, &utc), "2026-11-05 13:27");
        let east = TimeZone::fixed(jiff::tz::offset(11));
        assert_eq!(when(1_794_144_449 - 3600, now, &east), "Yesterday 23:27", "00:27 there is past midnight");
    }

    fn played(model: &mut Model, location: &str, title: &str) {
        let path = model.paths.history_file();
        model.history.record(&path, Played { at: 1_000, location: location.into(), title: title.into() });
    }

    #[test]
    fn the_newest_play_is_listed_first_and_found_by_search() {
        let mut m = Model::for_tests();
        played(&mut m, "/music/a.mp3", "Alpha");
        played(&mut m, "/music/b.mp3", "Beta");
        let mut p = Panel::open(PanelKind::History, &m);
        assert_eq!(titles(&p), ["Beta", "Alpha"]);
        type_text(&mut p, &mut m, "alp");
        assert_eq!(titles(&p), ["Alpha"]);
        played(&mut m, "/music/c.mp3", "Alpine");
        assert!(p.refresh(&m), "a new play shows while open");
        assert_eq!(titles(&p), ["Alpine", "Alpha"]);
    }

    /// The titles the list shows, without their times.
    fn titles(p: &Panel) -> Vec<String> {
        let Panel::History(h) = p else { unreachable!() };
        match h.form.widget(h.list) {
            Widget::List { rows, .. } => rows.iter().map(|r| r.split_once("   ").unwrap().1.to_string()).collect(),
            _ => unreachable!(),
        }
    }

    #[test]
    fn a_track_that_starts_is_recorded_with_its_title() {
        let mut m = crate::panels::tests::songs(&["Alpha", "Beta"]);
        let id = m.playlist.entries()[1].id;
        let info = popamp_audio::decoder::StreamInfo {
            sample_rate: 44_100,
            channels: 2,
            length_ms: Some(1000),
            bitrate_kbps: None,
            codec: "mp3".into(),
            seekable: true,
        };
        let meta = popamp_core::meta::TrackMeta { title: Some("Beta".into()), artist: Some("Band".into()), ..Default::default() };
        m.on_engine(popamp_audio::engine::EngineEvent::Started { id, info, meta, stream: None });
        let shown = m.playlist.entries()[1].display.clone();
        let newest = m.history.newest_first().next().map(|p| (p.location.clone(), p.title.clone()));
        assert!(shown.contains("Beta"), "{shown}");
        assert_eq!(newest, Some(("/music/1.mp3".into(), shown)), "the title as the playlist shows it");
    }

    fn locations(m: &Model) -> Vec<&str> {
        m.playlist.entries().iter().map(|e| e.location.as_str()).collect()
    }

    #[test]
    fn a_track_in_the_playlist_plays_from_there_another_is_added() {
        let mut m = crate::panels::tests::songs(&["Alpha", "Beta"]);
        played(&mut m, "/music/0.mp3", "Alpha");
        played(&mut m, "/elsewhere/c.mp3", "Gamma");
        let mut p = Panel::open(PanelKind::History, &m);
        assert!(!press(&mut p, &mut m, Key::Enter), "playing closes the window");
        assert_eq!(locations(&m), ["/music/0.mp3", "/music/1.mp3", "/elsewhere/c.mp3"]);
        assert_eq!(m.playlist.current(), Some(2));
        let mut p = Panel::open(PanelKind::History, &m);
        type_text(&mut p, &mut m, "alpha");
        press(&mut p, &mut m, Key::Enter);
        assert_eq!((locations(&m).len(), m.playlist.current()), (3, Some(0)));
    }
}
