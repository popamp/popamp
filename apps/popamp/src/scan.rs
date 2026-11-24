//! Scanning the selected tracks for ReplayGain. The values go to
//! `replaygain.txt`, never into the files, and play from then on. A
//! window counts the tracks, can cancel, and says how the scan ended.

use crate::model::{AppEvent, Model, UiReq};
use crate::panels::{Act, Flag, PanelKind};
use crate::widgets::{Event, Form, Rect, Widget, BUTTON};
use popamp_audio::replaygain::{self, Outcome};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

/// How a scan running on its own thread reports.
#[derive(Debug)]
pub enum ScanEvent {
    /// The number of tracks done.
    Progress(usize),
    /// Each location with what its scan gave; `None` when cancelled.
    Done(Option<Vec<(String, Outcome)>>),
}

/// Where the ReplayGain scan is.
#[derive(Debug, Default, Clone)]
pub enum ScanState {
    #[default]
    Idle,
    Running {
        done: usize,
        total: usize,
        /// Set to stop the scan.
        cancel: Arc<AtomicBool>,
    },
    Finished(Ending),
}

/// How the last scan ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Ending {
    Cancelled,
    Scanned { total: usize, failed: usize },
}

impl ScanState {
    pub fn is_running(&self) -> bool {
        matches!(self, ScanState::Running { .. })
    }

    #[cfg(test)]
    pub fn is_cancelled(&self) -> bool {
        matches!(self, ScanState::Running { cancel, .. } if cancel.load(Ordering::Relaxed))
    }
}

impl Model {
    /// Scans the selected local tracks, or shows the scan under way.
    pub fn scan_replaygain(&mut self) {
        if self.scan.is_running() {
            return self.ui.push(UiReq::Panel(PanelKind::ReplayGainScan));
        }
        let locations: Vec<String> =
            self.playlist.entries().iter().filter(|e| e.selected && !e.is_url()).map(|e| e.location.clone()).collect();
        if locations.is_empty() {
            return self.flash(tr!("ReplayGain: select the tracks to scan"));
        }
        let cancel = Arc::new(AtomicBool::new(false));
        let (events, sources, cx, total) = (self.events.clone(), self.sources.clone(), self.source_context(), locations.len());
        let stop = cancel.clone();
        let spawned = std::thread::Builder::new().name("popamp-rgscan".into()).spawn(move || {
            let outcomes = replaygain::scan(
                &locations,
                &sources,
                &cx,
                &|done| {
                    events.send(AppEvent::Scan(ScanEvent::Progress(done)));
                },
                &stop,
            );
            events.send(AppEvent::Scan(ScanEvent::Done(outcomes.map(|o| locations.into_iter().zip(o).collect()))));
        });
        match spawned {
            Ok(_) => {
                self.scan = ScanState::Running { done: 0, total, cancel };
                self.ui.push(UiReq::Panel(PanelKind::ReplayGainScan));
            }
            Err(e) => log::warn!("cannot start a ReplayGain scan: {e}"),
        }
    }

    pub fn on_scan(&mut self, ev: ScanEvent) {
        match ev {
            ScanEvent::Progress(now) => {
                if let ScanState::Running { done, .. } = &mut self.scan {
                    *done = now;
                }
            }
            ScanEvent::Done(None) => {
                self.scan = ScanState::Finished(Ending::Cancelled);
                self.show_scan(tr!("ReplayGain: scan cancelled").into());
            }
            ScanEvent::Done(Some(results)) => self.scan_done(results),
        }
    }

    /// Stops the scan under way, if any; nothing it measured is kept.
    fn cancel_scan(&self) {
        if let ScanState::Running { cancel, .. } = &self.scan {
            cancel.store(true, Ordering::Relaxed);
        }
    }

    /// Keeps what was measured and plays it from the next track on.
    fn scan_done(&mut self, results: Vec<(String, Outcome)>) {
        let (total, mut failed) = (results.len(), 0);
        let store = Arc::make_mut(&mut self.scanned);
        for (location, outcome) in results {
            match outcome {
                Ok((stamp, gains)) => store.insert(location, stamp, gains),
                Err(e) => {
                    log::warn!("ReplayGain scan of {location}: {e}");
                    failed += 1;
                }
            }
        }
        if let Err(e) = self.scanned.save(&self.paths.replaygain_file()) {
            log::warn!("cannot save ReplayGain values: {e}");
        }
        self.push_engine_settings();
        self.scan = ScanState::Finished(Ending::Scanned { total, failed });
        self.show_scan(scan_summary(total, failed, self.settings().replaygain));
    }

    fn show_scan(&mut self, text: String) {
        self.transient = Some((text, Instant::now() + Duration::from_secs(4)));
    }
}

/// What the marquee says when a scan ends.
fn scan_summary(total: usize, failed: usize, on: bool) -> String {
    let mut s = tr!("ReplayGain: {done} of {total} scanned", done = total - failed, total);
    if !on {
        s.push_str(tr!(", turn it on in Options > ReplayGain"));
    }
    s
}

/// The lines the scan window shows for `scan`, and its button.
fn window_text(scan: &ScanState, on: bool, file: &str) -> ([String; 3], &'static str) {
    match scan {
        ScanState::Idle => (Default::default(), tr!("OK")),
        ScanState::Running { cancel, .. } if cancel.load(Ordering::Relaxed) => {
            ([tr!("Cancelling: finishing the tracks under way").into(), String::new(), String::new()], tr!("Cancel"))
        }
        ScanState::Running { done, total, .. } => (
            [trn!(*total, "Scanning: {done} of {n} track", "Scanning: {done} of {n} tracks", done), String::new(), String::new()],
            tr!("Cancel"),
        ),
        ScanState::Finished(Ending::Cancelled) => ([tr!("Cancelled: nothing was saved").into(), String::new(), String::new()], tr!("OK")),
        ScanState::Finished(Ending::Scanned { total, failed }) => {
            let mut done = trn!(*total, "Done: {done} of {n} track scanned", "Done: {done} of {n} tracks scanned", done = total - failed);
            if *failed > 0 {
                done += &tr!(", {failed} failed", failed);
            }
            let hint = if on { String::new() } else { tr!("ReplayGain is off: turn it on in Options > ReplayGain").into() };
            ([done, tr!("Saved to {file}", file), hint], tr!("OK"))
        }
    }
}

/// The ReplayGain scan window. It shows the model's scan; Cancel stops
/// the scan, and when it ends the window waits for OK unless told not to.
pub struct ScanPanel {
    pub(crate) form: Form<Act>,
    lines: [usize; 3],
    button: usize,
    /// What the window shows, to redraw only on a change.
    shown: ([String; 3], &'static str),
    /// The window saw the scan run, so its end may close it.
    saw_running: bool,
    close: bool,
}

impl ScanPanel {
    pub fn new(model: &Model) -> ScanPanel {
        let w = 460;
        let mut form = Form::new(w, 10 + 3 * 24 + 6 + 22 + 10 + BUTTON.1 + 10);
        let lines = [0, 1, 2].map(|n| form.add(Rect::new(14, 10 + n * 24, w - 28, 22), Widget::Label { text: String::new(), dim: n == 1 }));
        let y = 10 + 3 * 24 + 6;
        let keep = Flag::KeepScanOpen;
        form.add(
            Rect::new(14, y, w - 28, 20),
            Widget::Check {
                label: tr!("Keep this window open when the scan ends").into(),
                checked: keep.get(model.settings()),
                action: Act::Flag(keep),
            },
        );
        // One button: Cancel while the scan runs, then OK.
        let button = form.add(
            Rect::new(w - BUTTON.0 - 10, y + 32, BUTTON.0, BUTTON.1),
            Widget::Button { label: tr!("Cancel").into(), action: Act::Close, default: true },
        );
        let mut p = ScanPanel { form, lines, button, shown: Default::default(), saw_running: false, close: false };
        p.refresh(model);
        p
    }

    pub fn handle(&mut self, ev: Event<Act>, model: &mut Model) -> bool {
        match ev {
            Event::Toggled(Act::Flag(f), on) => {
                f.set(model, on);
                true
            }
            Event::Clicked(_) | Event::Cancel => {
                model.cancel_scan();
                false
            }
            _ => true,
        }
    }

    /// Follows the scan; returns whether the window needs drawing again.
    pub fn refresh(&mut self, model: &Model) -> bool {
        let running = model.scan.is_running();
        if self.saw_running && !running && !model.settings().rg_scan_keep_open {
            self.close = true;
        }
        self.saw_running |= running;
        let file = model.paths.replaygain_file().display().to_string();
        let now = window_text(&model.scan, model.settings().replaygain, &file);
        if now == self.shown {
            return false;
        }
        for (&i, text) in self.lines.iter().zip(&now.0) {
            self.form.set_label(i, text.clone());
        }
        self.form.set_label(self.button, now.1.into());
        self.shown = now;
        true
    }

    pub fn wants_close(&self) -> bool {
        self.close
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::UiReq;
    use crate::panels::tests::{click, click_button, songs};
    use crate::panels::{Act, Flag, Panel, PanelKind};
    use crate::widgets::Widget;
    use popamp_core::replaygain::{Gains, Level, Scanned, Stamp};

    fn message(m: &Model) -> String {
        m.transient.as_ref().map(|t| t.0.clone()).unwrap_or_default()
    }

    /// What the window's labels and buttons say, top to bottom.
    fn says(p: &Panel) -> Vec<String> {
        let f = p.form();
        (0..f.count_all())
            .filter_map(|i| match f.widget(i) {
                Widget::Label { text, .. } if !text.is_empty() => Some(text.clone()),
                Widget::Button { label, .. } => Some(format!("[{label}]")),
                _ => None,
            })
            .collect()
    }

    fn keep_open_box(p: &Panel) -> crate::widgets::Rect {
        let f = p.form();
        let i = (0..f.count_all())
            .find(|&i| matches!(f.widget(i), Widget::Check { action: Act::Flag(Flag::KeepScanOpen), .. }))
            .expect("the check box");
        f.rect(i)
    }

    const STAMP: Stamp = Stamp { size: 10, modified: 20 };
    const GAINS: Gains = Gains { track: Level { gain_db: -3.0, peak: 0.9 }, album: None };

    /// A scan of two tracks, under way, with its window open.
    fn scanning() -> (Model, Panel) {
        let mut m = songs(&["Alpha", "Beta"]);
        m.playlist.select_all(true);
        m.scan_replaygain();
        assert!(m.ui.iter().any(|r| matches!(r, UiReq::Panel(PanelKind::ReplayGainScan))), "the window opens");
        let p = Panel::open(PanelKind::ReplayGainScan, &m);
        (m, p)
    }

    fn one_of_two() -> Vec<(String, Outcome)> {
        vec![("/music/0.mp3".into(), Ok((STAMP, GAINS))), ("/music/1.mp3".into(), Err("broken".into()))]
    }

    #[test]
    fn nothing_selected_is_nothing_to_scan() {
        let mut m = songs(&["Alpha"]);
        m.scan_replaygain();
        assert!(!m.scan.is_running());
        assert_eq!(message(&m), "ReplayGain: select the tracks to scan");
        assert!(m.ui.is_empty(), "no window");
    }

    #[test]
    fn the_window_counts_the_tracks_scanned() {
        let (mut m, mut p) = scanning();
        assert_eq!(says(&p), ["Scanning: 0 of 2 tracks", "[Cancel]"]);
        m.on_scan(ScanEvent::Progress(1));
        assert!(p.refresh(&m), "redrawn");
        assert_eq!(says(&p), ["Scanning: 1 of 2 tracks", "[Cancel]"]);
        assert!(!p.refresh(&m), "nothing new");
    }

    #[test]
    fn a_second_scan_raises_the_window_of_the_first() {
        let (mut m, _p) = scanning();
        m.ui.clear();
        m.scan_replaygain();
        assert!(m.ui.iter().any(|r| matches!(r, UiReq::Panel(PanelKind::ReplayGainScan))));
        assert_eq!(message(&m), "", "no second scan");
    }

    #[test]
    fn cancel_stops_the_scan_and_saves_nothing() {
        let (mut m, mut p) = scanning();
        assert!(!click_button(&mut p, &mut m, Act::Close), "the window closes");
        assert!(m.scan.is_cancelled());
        m.scan_replaygain();
        let p = Panel::open(PanelKind::ReplayGainScan, &m);
        assert_eq!(says(&p), ["Cancelling: finishing the tracks under way", "[Cancel]"], "opened again meanwhile");
        m.on_scan(ScanEvent::Done(None));
        assert!(!m.scan.is_running());
        assert_eq!(message(&m), "ReplayGain: scan cancelled");
        assert!(Scanned::load(&m.paths.replaygain_file()).is_empty());
    }

    #[test]
    fn the_window_waits_for_ok_when_the_scan_ends() {
        let (mut m, mut p) = scanning();
        m.on_scan(ScanEvent::Done(Some(one_of_two())));
        assert!(p.refresh(&m));
        assert!(!p.wants_close(), "kept open by default");
        let file = m.paths.replaygain_file().display().to_string();
        assert_eq!(
            says(&p),
            [
                "Done: 1 of 2 tracks scanned, 1 failed".to_string(),
                format!("Saved to {file}"),
                "ReplayGain is off: turn it on in Options > ReplayGain".to_string(),
                "[OK]".to_string(),
            ]
        );
        assert!(!click_button(&mut p, &mut m, Act::Close), "OK closes it");
        assert!(matches!(m.scan, ScanState::Finished(_)), "and cancels nothing");
    }

    #[test]
    fn unticked_the_window_closes_itself_when_the_scan_ends() {
        let (mut m, mut p) = scanning();
        let r = keep_open_box(&p);
        assert!(click(&mut p, &mut m, r));
        assert!(!m.settings().rg_scan_keep_open, "the same setting as in Preferences");
        m.on_scan(ScanEvent::Done(Some(one_of_two())));
        p.refresh(&m);
        assert!(p.wants_close());
        assert_eq!(message(&m), "ReplayGain: 1 of 2 scanned, turn it on in Options > ReplayGain");
    }

    #[test]
    fn measured_values_are_kept_and_reach_playback() {
        let (mut m, _p) = scanning();
        m.on_scan(ScanEvent::Done(Some(one_of_two())));
        assert!(!m.scan.is_running());
        let saved = Scanned::load(&m.paths.replaygain_file());
        assert_eq!(saved.get("/music/0.mp3", STAMP), Some(&GAINS));
        assert_eq!(m.source_context().scanned.get("/music/0.mp3", STAMP), Some(&GAINS), "the engine is told");
        m.config.settings.replaygain = true;
        m.scan_replaygain();
        m.on_scan(ScanEvent::Done(Some(vec![])));
        assert_eq!(message(&m), "ReplayGain: 0 of 0 scanned");
        assert_eq!(Scanned::load(&m.paths.replaygain_file()).len(), 1, "earlier scans stay");
    }
}
