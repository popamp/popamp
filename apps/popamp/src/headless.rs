//! `/RENDER=<dir>`: draws the windows into PNG files without a display or
//! audio device, for skin previews and pixel comparisons between versions.
//!
//! Each scenario starts from the settings in `<dir>/popamp.ini` (defaults
//! if there is none) and changes only what it shows. The set of states
//! follows Ferramp's `--render` (see THIRD_PARTY.md). With no files the
//! title display reads POPAMP, without the version.

use crate::canvas::Canvas;
use crate::font::Fonts;
use crate::model::{AddMode, Model};
use crate::scale::Scale;
use crate::skinwin::SkinWin;
use popamp_audio::{PlayState, StreamInfo};
use popamp_core::config::Scaling;
use popamp_core::config::{Config, Paths, Settings};
use popamp_skin::layout::WindowKey;
use popamp_skin::Bitmap;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

/// What the "playing" scenarios pretend to play.
const PLAY_POS_MS: u64 = 83_000;
const FALLBACK_LENGTH_MS: u64 = 225_000;

/// A state that needs only settings: window, file name, what to change.
type Shot = (WindowKey, &'static str, fn(&mut Settings));

const STOPPED: &[Shot] = &[
    (WindowKey::Main, "main-stopped", |_| {}),
    (WindowKey::Eq, "eq-default", |_| {}),
    (WindowKey::Playlist, "playlist-default", |_| {}),
    (WindowKey::Main, "main-toggles", |s| {
        s.shuffle = true;
        s.repeat = true;
        s.volume = 77; // 30 %
        s.balance = -51; // 40 % left
    }),
    // Equalizer values: 0 = +12 dB, 63 = -12 dB, 31 = centre.
    (WindowKey::Eq, "eq-values", |s| {
        s.use_eq = true;
        s.autoload_eq = true;
        s.preamp = 0;
        s.eq_data = "63,0,31,63,0,31,63,0,31,63".into();
    }),
    (WindowKey::Main, "main-shade", |s| s.windowshade = true),
    (WindowKey::Eq, "eq-shade", |s| s.eq_ws = true),
    (WindowKey::Playlist, "playlist-shade", |s| s.pe_ws = true),
    (WindowKey::Playlist, "playlist-small", |s| s.pe_height = 116),
    (WindowKey::Playlist, "playlist-wide", |s| s.pe_width = 450),
];

struct Renderer {
    dir: PathBuf,
    skin: Option<PathBuf>,
    files: Vec<String>,
}

impl Renderer {
    /// A fresh model with the skin and playlist loaded, nothing playing.
    fn model(&self) -> Result<Model, String> {
        let paths = Paths::resolve(Some(self.dir.clone()));
        let config = Config::load(paths.ini_file());
        let mut m = Model::headless(paths, config);
        if let Some(p) = &self.skin {
            m.skin = m.load_skin(p)?;
        }
        m.add_now(self.files.clone(), AddMode::Append);
        if m.playlist.is_empty() {
            // Not the version or edition, so images compare between builds.
            m.transient = Some(("POPAMP".into(), Instant::now() + Duration::from_secs(3600)));
        }
        Ok(m)
    }

    fn save(&self, m: &mut Model, k: WindowKey, name: &str) -> Result<(), String> {
        let (w, h) = crate::bind::window_size(m, k);
        let mut b = Bitmap::new(w, h, 0);
        SkinWin::new(k).draw(m, &mut Fonts::for_skin(&m.skin), &mut Canvas::new(&mut b, Scale::ONE), true);
        let path = self.dir.join(format!("{name}.png"));
        b.save_png(&path).map_err(|e| format!("{}: {e}", path.display()))
    }

    /// Renders after changing the settings with `set`.
    fn shot(&self, k: WindowKey, name: &str, set: &fn(&mut Settings)) -> Result<(), String> {
        let mut m = self.model()?;
        set(m.settings_mut());
        self.save(&mut m, k, name)
    }

    fn all(&self) -> Result<(), String> {
        for (k, name, set) in STOPPED {
            self.shot(*k, name, set)?;
        }
        self.playing_states()?;
        self.scaled()?;
        self.eq_preset()
    }

    /// The playing main window at x1.5, sharp and nearest, to compare.
    fn scaled(&self) -> Result<(), String> {
        let m = self.playing()?;
        let (w, h) = crate::bind::window_size(&m, WindowKey::Main);
        let mut font = Fonts::for_skin(&m.skin);
        let mut b = Bitmap::new(w, h, 0);
        SkinWin::new(WindowKey::Main).draw(&m, &mut font, &mut Canvas::new(&mut b, Scale::ONE), true);
        for (mode, name) in [(Scaling::Sharp, "main-x1.5-sharp"), (Scaling::Nearest, "main-x1.5-nearest")] {
            let s = Scale::new(150, mode);
            let mut out = Bitmap::new(s.px(w), s.px(h), 0);
            if m.skin.vector {
                SkinWin::new(WindowKey::Main).draw(&m, &mut font, &mut Canvas::new(&mut out, s), true);
            } else {
                s.present(&b, &mut out.px, out.w as usize, out.h as usize);
            }
            let path = self.dir.join(format!("{name}.png"));
            out.save_png(&path).map_err(|e| format!("{}: {e}", path.display()))?;
        }
        Ok(())
    }

    fn playing_states(&self) -> Result<(), String> {
        let mut m = self.playing()?;
        self.save(&mut m, WindowKey::Main, "main-playing")?;
        self.save(&mut m, WindowKey::Playlist, "playlist-playing")?;
        m.settings_mut().sa = 2;
        analyze(&mut m);
        self.save(&mut m, WindowKey::Main, "main-scope")?;
        m.settings_mut().sa = 1;
        m.settings_mut().windowshade = true;
        self.save(&mut m, WindowKey::Main, "main-shade-playing")
    }

    fn eq_preset(&self) -> Result<(), String> {
        let mut m = self.model()?;
        let rock = m.eq_presets.iter().find(|p| p.name.eq_ignore_ascii_case("rock")).cloned();
        if let Some(p) = rock {
            let s = m.settings_mut();
            s.use_eq = true;
            s.preamp = p.preamp as i32;
            s.eq_data = p.bands.map(|b| b.to_string()).join(",");
        }
        self.save(&mut m, WindowKey::Eq, "eq-preset")
    }

    /// The first track "playing" at a fixed position, with a synthetic
    /// signal in the visualizer.
    fn playing(&self) -> Result<Model, String> {
        let mut m = self.model()?;
        let length_ms = m.playlist.entries().first().and_then(|e| e.length_ms).unwrap_or(FALLBACK_LENGTH_MS);
        let info = StreamInfo {
            sample_rate: 44_100,
            channels: 2,
            length_ms: Some(length_ms),
            bitrate_kbps: Some(128),
            codec: "mp3".into(),
            seekable: true,
        };
        m.engine.simulate(PlayState::Playing, PLAY_POS_MS.min(length_ms), &info);
        if !m.playlist.is_empty() {
            m.playlist.set_current(Some(0));
            m.status.playing_id = m.playlist.current_entry().map(|e| e.id);
        }
        m.status.info = Some(info);
        analyze(&mut m);
        Ok(m)
    }
}

/// Feeds 110 Hz + 880 Hz + 5 kHz into the visualizer. Several passes let
/// the bars and peaks settle the way they do after a moment of playback.
fn analyze(m: &mut Model) {
    for (i, s) in m.vis.samples_mut().iter_mut().enumerate() {
        let t = i as f32 / 44_100.0;
        let tone = |hz: f32| (t * hz * std::f32::consts::TAU).sin();
        *s = 0.35 * tone(110.0) + 0.2 * tone(880.0) + 0.1 * tone(5000.0);
    }
    let settings = m.settings().clone();
    for _ in 0..4 {
        m.vis.analyze(&settings, 44_100);
    }
}

pub fn render_all(dir: &Path, skin: Option<PathBuf>, files: Vec<String>) -> Result<(), String> {
    std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    Renderer { dir: dir.to_path_buf(), skin, files }.all()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn size(path: &Path) -> (u32, u32) {
        image::image_dimensions(path).unwrap()
    }

    #[test]
    fn renders_every_window_and_state() {
        let dir = std::env::temp_dir().join(format!("popamp-render-test-{}", std::process::id()));
        render_all(&dir, None, Vec::new()).unwrap();
        assert_eq!(size(&dir.join("main-stopped.png")), (275, 116));
        assert_eq!(size(&dir.join("main-shade.png")), (275, 14));
        assert_eq!(size(&dir.join("eq-default.png")), (275, 116));
        assert_eq!(size(&dir.join("eq-shade.png")), (275, 14));
        assert_eq!(size(&dir.join("playlist-default.png")), (275, 232));
        assert_eq!(size(&dir.join("playlist-wide.png")), (450, 232));
        assert_eq!(size(&dir.join("playlist-shade.png")), (275, 14));
        assert_eq!(size(&dir.join("main-x1.5-sharp.png")), (413, 174));
        let read = |n: &str| std::fs::read(dir.join(n)).unwrap();
        assert_ne!(read("main-playing.png"), read("main-stopped.png"), "playing shows time and vis");
        assert_ne!(read("main-scope.png"), read("main-playing.png"), "oscilloscope differs from spectrum");
        assert_ne!(read("eq-values.png"), read("eq-default.png"));
        assert_ne!(read("main-toggles.png"), read("main-stopped.png"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Where the committed images live; `POPAMP_BLESS=1 cargo test golden`
    /// rewrites them after a change that is meant to show.
    fn golden_dir() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/golden")
    }

    fn names(dir: &Path) -> Vec<String> {
        let mut v: Vec<String> = std::fs::read_dir(dir)
            .map(|rd| rd.filter_map(|e| e.ok()?.file_name().into_string().ok()).filter(|n| n.ends_with(".png")).collect())
            .unwrap_or_default();
        v.sort();
        v
    }

    /// What differs between two images, if anything.
    fn difference(want: &Path, got: &Path) -> Option<String> {
        let (want, got) = (image::open(want).unwrap().to_rgba8(), image::open(got).unwrap().to_rgba8());
        if want.dimensions() != got.dimensions() {
            return Some(format!("size {:?}, want {:?}", got.dimensions(), want.dimensions()));
        }
        let mut off = want.enumerate_pixels().zip(got.pixels()).filter(|((_, _, a), b)| a != b);
        let ((x, y, _), _) = off.next()?;
        Some(format!("{} pixels differ, the first at ({x}, {y})", off.count() + 1))
    }

    #[test]
    fn golden_images_match() {
        let dir = std::env::temp_dir().join(format!("popamp-golden-{}", std::process::id()));
        render_all(&dir, None, Vec::new()).unwrap();
        let golden = golden_dir();
        if std::env::var_os("POPAMP_BLESS").is_some() {
            let _ = std::fs::remove_dir_all(&golden);
            std::fs::create_dir_all(&golden).unwrap();
            for n in names(&dir) {
                std::fs::copy(dir.join(&n), golden.join(&n)).unwrap();
            }
        }
        assert_eq!(names(&dir), names(&golden), "rendered vs committed in {}", golden.display());
        let wrong: Vec<String> =
            names(&dir).into_iter().filter_map(|n| Some(format!("{n}: {}", difference(&golden.join(&n), &dir.join(&n))?))).collect();
        assert!(wrong.is_empty(), "the windows changed (see {}):\n{}", dir.display(), wrong.join("\n"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_broken_skin_is_named_once() {
        let dir = std::env::temp_dir().join(format!("popamp-render-broken-{}", std::process::id()));
        let skin = dir.join("broken.wsz");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(&skin, b"not a zip").unwrap();
        let e = render_all(&dir.join("out"), Some(skin.clone()), Vec::new()).unwrap_err();
        let _ = std::fs::remove_dir_all(&dir);
        assert_eq!(e.matches(&*skin.to_string_lossy()).count(), 1, "{e}");
    }
}
