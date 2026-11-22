//! MIDI files, played by a SoundFont synthesizer (rustysynth).
//!
//! The file becomes one list of events in sample frames, so a seek is a
//! jump: the events before it set the instruments and controllers without
//! a sound, and the notes held there start again.
//!
//! The SoundFont is the one in the settings, else the first General MIDI
//! one the system has. It stays loaded while tracks use it.

use crate::decoder::{AudioSource, Status, StreamInfo};
use midly::{Format, MetaMessage, Smf, Timing, TrackEventKind};
use parking_lot::Mutex;
use popamp_core::meta::TrackMeta;
use rustysynth::{SoundFont, Synthesizer, SynthesizerSettings};
use std::path::{Path, PathBuf};
use std::sync::Arc;

const RATE: u32 = 44_100;
/// Frames rendered per call: about 23 ms.
const BLOCK: usize = 1_024;
/// After the last event, notes ring out until silent or this long.
const MAX_TAIL_MS: u64 = 3_000;
const SILENT: f32 = 1e-4;

/// Where the systems keep their General MIDI SoundFonts.
const SYSTEM_SOUNDFONTS: &[&str] = &[
    "/usr/share/sounds/sf2/default-GM.sf2",
    "/usr/share/sounds/sf2/FluidR3_GM.sf2",
    "/usr/share/soundfonts/default.sf2",
    "/usr/share/soundfonts/FluidR3_GM.sf2",
    "/usr/share/sounds/sf2/TimGM6mb.sf2",
];

/// The SoundFont the downloads carry: beside popamp.exe on Windows, in
/// `Contents/Resources` of Popamp.app on macOS.
const BUNDLED_SOUNDFONT: &str = "GeneralUser-GS.sf2";

/// The SoundFont to use: `setting` when given, else the one the download
/// carries, else the system's.
pub fn soundfont_path(setting: &str) -> Result<PathBuf, String> {
    let exe = std::env::current_exe().ok();
    find_soundfont(setting, exe.as_deref().and_then(Path::parent))
}

fn find_soundfont(setting: &str, program_dir: Option<&Path>) -> Result<PathBuf, String> {
    if !setting.is_empty() {
        return Ok(PathBuf::from(setting));
    }
    let beside = program_dir.map(|d| d.join(BUNDLED_SOUNDFONT));
    // In Popamp.app the program is in Contents/MacOS, its files in Resources.
    let named = |p: &Path, n: &str| p.file_name().is_some_and(|f| f == n);
    let in_app = program_dir
        .filter(|d| named(d, "MacOS"))
        .and_then(Path::parent)
        .filter(|d| named(d, "Contents"))
        .map(|contents| contents.join("Resources").join(BUNDLED_SOUNDFONT));
    beside
        .into_iter()
        .chain(in_app)
        .chain(SYSTEM_SOUNDFONTS.iter().map(PathBuf::from))
        .find(|p| p.is_file())
        .ok_or_else(|| "MIDI needs a SoundFont: set one in Preferences, or install a General MIDI one (fluid-soundfont-gm)".to_string())
}

fn load_soundfont(path: &Path) -> Result<Arc<SoundFont>, String> {
    static LOADED: Mutex<Option<(PathBuf, Arc<SoundFont>)>> = Mutex::new(None);
    let mut loaded = LOADED.lock();
    if let Some((p, sf)) = loaded.as_ref() {
        if p == path {
            return Ok(sf.clone());
        }
    }
    let mut file = std::fs::File::open(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let sf = Arc::new(SoundFont::new(&mut file).map_err(|e| format!("{}: not a SoundFont ({e:?})", path.display()))?);
    *loaded = Some((path.to_path_buf(), sf.clone()));
    Ok(sf)
}

/// A channel message, as the synthesizer takes it.
#[derive(Debug, Clone, Copy, PartialEq)]
struct Event {
    frame: u64,
    status: u8,
    data1: u8,
    data2: u8,
}

impl Event {
    /// The key and velocity of a note-on; a note-on at velocity 0 is an off.
    fn note_on(&self) -> Option<(u8, u8)> {
        (self.status & 0xF0 == 0x90 && self.data2 > 0).then_some((self.data1, self.data2))
    }

    fn note_off(&self) -> Option<u8> {
        let command = self.status & 0xF0;
        (command == 0x80 || (command == 0x90 && self.data2 == 0)).then_some(self.data1)
    }

    fn channel(&self) -> u8 {
        self.status & 0x0F
    }
}

/// A MIDI file as one list of events in time order.
#[derive(Debug, Default, PartialEq)]
struct Song {
    events: Vec<Event>,
    /// Up to the last event or track end.
    frames: u64,
}

impl Song {
    fn length_ms(&self) -> u64 {
        self.frames * 1000 / RATE as u64
    }
}

/// The Standard MIDI File inside `bytes`, which may be wrapped in RIFF (`.rmi`).
fn smf_bytes(bytes: &[u8]) -> &[u8] {
    if bytes.len() < 12 || &bytes[..4] != b"RIFF" || &bytes[8..12] != b"RMID" {
        return bytes;
    }
    let mut at = 12;
    while at + 8 <= bytes.len() {
        let size = u32::from_le_bytes([bytes[at + 4], bytes[at + 5], bytes[at + 6], bytes[at + 7]]) as usize;
        let body = &bytes[at + 8..(at + 8 + size).min(bytes.len())];
        if &bytes[at..at + 4] == b"data" {
            return body;
        }
        at += 8 + size + size % 2;
    }
    bytes
}

enum Raw {
    Tempo(u32),
    Message([u8; 3]),
    End,
}

fn parse(bytes: &[u8]) -> Result<Song, String> {
    let smf = Smf::parse(smf_bytes(bytes)).map_err(|e| format!("not a MIDI file ({e})"))?;
    // Every event at its tick; sequential tracks follow one another.
    let mut raw: Vec<(u64, Raw)> = Vec::new();
    let mut offset = 0;
    for track in &smf.tracks {
        let mut tick = offset;
        for ev in track {
            tick += ev.delta.as_int() as u64;
            let item = match ev.kind {
                TrackEventKind::Midi { channel, message } => {
                    let mut buf = Vec::with_capacity(3);
                    let live = midly::live::LiveEvent::Midi { channel, message };
                    live.write_std(&mut buf).map_err(|e| e.to_string())?;
                    buf.resize(3, 0);
                    Raw::Message([buf[0], buf[1], buf[2]])
                }
                TrackEventKind::Meta(MetaMessage::Tempo(t)) => Raw::Tempo(t.as_int()),
                TrackEventKind::Meta(MetaMessage::EndOfTrack) => Raw::End,
                _ => continue,
            };
            raw.push((tick, item));
        }
        if smf.header.format == Format::Sequential {
            offset = tick;
        }
    }
    raw.sort_by_key(|(tick, _)| *tick);
    // Ticks to frames, through the tempo changes on the way.
    let seconds_per_tick = |tempo_us: u32| match smf.header.timing {
        Timing::Metrical(tpb) => tempo_us as f64 / 1e6 / tpb.as_int().max(1) as f64,
        Timing::Timecode(fps, sub) => 1.0 / (fps.as_f32() as f64 * sub.max(1) as f64),
    };
    let mut song = Song::default();
    let (mut last_tick, mut seconds, mut tempo) = (0, 0.0, 500_000);
    for (tick, item) in raw {
        seconds += (tick - last_tick) as f64 * seconds_per_tick(tempo);
        last_tick = tick;
        let frame = (seconds * RATE as f64).round() as u64;
        song.frames = song.frames.max(frame);
        match item {
            Raw::Tempo(t) => tempo = t,
            Raw::Message([status, data1, data2]) => song.events.push(Event { frame, status, data1, data2 }),
            Raw::End => {}
        }
    }
    Ok(song)
}

fn load_song(path: &Path) -> Result<Song, String> {
    let bytes = std::fs::read(path).map_err(|e| format!("{}: {e}", path.display()))?;
    parse(&bytes).map_err(|e| format!("{}: {e}", path.display()))
}

/// What a MIDI file says about itself; it needs no SoundFont.
pub fn read_meta(path: &Path) -> Result<TrackMeta, String> {
    Ok(meta(load_song(path)?.length_ms()))
}

fn meta(length_ms: u64) -> TrackMeta {
    TrackMeta { length_ms: Some(length_ms), sample_rate: Some(RATE), channels: Some(2), codec: Some("MIDI".into()), ..TrackMeta::default() }
}

pub struct MidiSource {
    info: StreamInfo,
    synth: Synthesizer,
    song: Song,
    /// The first event not yet sent.
    next: usize,
    /// Frames played since the start of the file.
    frame: u64,
    left: Vec<f32>,
    right: Vec<f32>,
}

impl MidiSource {
    pub fn open(path: &Path, soundfont: &str) -> Result<(MidiSource, TrackMeta), String> {
        let song = load_song(path)?;
        let soundfont = load_soundfont(&soundfont_path(soundfont)?)?;
        let synth = Synthesizer::new(&soundfont, &SynthesizerSettings::new(RATE as i32)).map_err(|e| format!("synthesizer: {e:?}"))?;
        let length = song.length_ms();
        let info = StreamInfo {
            sample_rate: RATE,
            channels: 2,
            length_ms: Some(length),
            bitrate_kbps: None,
            codec: "MIDI".into(),
            seekable: true,
        };
        let source = MidiSource { info, synth, song, next: 0, frame: 0, left: vec![0.0; BLOCK], right: vec![0.0; BLOCK] };
        Ok((source, meta(length)))
    }

    fn send(&mut self, ev: Event) {
        self.synth.process_midi_message(ev.channel() as i32, (ev.status & 0xF0) as i32, ev.data1 as i32, ev.data2 as i32);
    }

    /// Fills the buffers, sending each event at its frame.
    fn render(&mut self) {
        let mut wrote = 0;
        while wrote < BLOCK {
            while let Some(&ev) = self.song.events.get(self.next).filter(|e| e.frame <= self.frame) {
                self.send(ev);
                self.next += 1;
            }
            let until = self.song.events.get(self.next).map_or(u64::MAX, |e| e.frame);
            let n = (BLOCK - wrote).min((until - self.frame) as usize);
            self.synth.render(&mut self.left[wrote..wrote + n], &mut self.right[wrote..wrote + n]);
            wrote += n;
            self.frame += n as u64;
        }
    }

    fn ms(&self) -> u64 {
        self.frame * 1000 / RATE as u64
    }
}

impl AudioSource for MidiSource {
    fn info(&self) -> &StreamInfo {
        &self.info
    }

    fn next(&mut self, out: &mut Vec<f32>) -> Result<Status, String> {
        if self.next == self.song.events.len() && self.frame >= self.song.frames {
            let ringing = self.left.iter().chain(&self.right).any(|s| s.abs() > SILENT);
            if !ringing || self.ms() > self.song.length_ms() + MAX_TAIL_MS {
                return Ok(Status::Eof);
            }
        }
        self.render();
        out.extend(self.left.iter().zip(&self.right).flat_map(|(&l, &r)| [l, r]));
        Ok(Status::Data)
    }

    fn seek(&mut self, ms: u64) -> Result<u64, String> {
        let target = (ms * RATE as u64 / 1000).min(self.song.frames);
        self.synth.reset();
        let mut held: Vec<(u8, u8, u8)> = Vec::new();
        let mut i = 0;
        while let Some(&ev) = self.song.events.get(i).filter(|e| e.frame <= target) {
            if let Some((key, vel)) = ev.note_on() {
                held.retain(|&(c, k, _)| (c, k) != (ev.channel(), key));
                held.push((ev.channel(), key, vel));
            } else if let Some(key) = ev.note_off() {
                held.retain(|&(c, k, _)| (c, k) != (ev.channel(), key));
            } else {
                self.send(ev);
            }
            i += 1;
        }
        for (channel, key, vel) in held {
            self.synth.note_on(channel as i32, key as i32, vel as i32);
        }
        self.next = i;
        self.frame = target;
        self.left.fill(0.0);
        self.right.fill(0.0);
        Ok(self.ms())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture(name: &str) -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures").join(name)
    }

    fn open(name: &str) -> MidiSource {
        MidiSource::open(&fixture(name), &fixture("sine.sf2").to_string_lossy()).unwrap().0
    }

    fn play_all(source: &mut MidiSource) -> Vec<f32> {
        let mut out = Vec::new();
        while let Status::Data = source.next(&mut out).unwrap() {}
        out
    }

    /// Times the left channel crosses zero going up, per second.
    fn pitch(stereo: &[f32]) -> f32 {
        let left: Vec<f32> = stereo.iter().step_by(2).copied().collect();
        let ups = left.windows(2).filter(|w| w[0] <= 0.0 && w[1] > 0.0).count();
        ups as f32 * RATE as f32 / left.len() as f32
    }

    fn loud(samples: &[f32]) -> bool {
        samples.iter().any(|s| s.abs() > 0.05)
    }

    #[test]
    fn a_midi_note_plays_at_its_pitch_for_its_length() {
        let path = fixture("a4.mid");
        assert_eq!(read_meta(&path).unwrap().length_ms, Some(1_000));
        let mut source = open("a4.mid");
        let out = play_all(&mut source);
        let secs = out.len() as f32 / 2.0 / RATE as f32;
        assert!((1.0..1.0 + MAX_TAIL_MS as f32 / 1000.0 + 0.1).contains(&secs), "{secs} s");
        let held = &out[2 * 4_410..2 * 39_690];
        let a4 = pitch(held);
        assert!((435.0..445.0).contains(&a4), "{a4} Hz");
        assert!(loud(held), "audible");
    }

    #[test]
    fn a_seek_into_a_note_sounds_it_and_one_past_the_end_ends() {
        let mut source = open("a4.mid");
        assert_eq!(source.seek(500).unwrap(), 500);
        let mut out = Vec::new();
        source.next(&mut out).unwrap();
        assert!(loud(&out), "the held note sounds after the seek");
        source.seek(60_000).unwrap();
        let rest = play_all(&mut source);
        assert!(rest.len() < 2 * RATE as usize * MAX_TAIL_MS as usize / 1000, "{} samples", rest.len());
    }

    #[test]
    fn an_rmi_file_is_the_midi_inside() {
        let smf = std::fs::read(fixture("a4.mid")).unwrap();
        let mut rmi = b"RIFF".to_vec();
        rmi.extend((4 + 8 + smf.len() as u32).to_le_bytes());
        rmi.extend(b"RMIDdata");
        rmi.extend((smf.len() as u32).to_le_bytes());
        rmi.extend(&smf);
        assert_eq!(parse(&rmi), parse(&smf));
        assert_eq!(parse(&smf).unwrap().length_ms(), 1_000);
    }

    #[test]
    fn tempo_changes_stretch_the_time_after_them() {
        // One beat at 120 bpm (0.5 s), then a tempo of 60 bpm and one more beat (1 s).
        let track = [0x00, 0x90, 60, 100, 0x83, 0x60, 0xFF, 0x51, 0x03, 0x0F, 0x42, 0x40, 0x83, 0x60, 0x80, 60, 0, 0x00, 0xFF, 0x2F, 0x00];
        let mut smf = b"MThd\0\0\0\x06\0\0\0\x01\x01\xe0MTrk".to_vec();
        smf.extend((track.len() as u32).to_be_bytes());
        smf.extend(track);
        let song = parse(&smf).unwrap();
        assert_eq!(song.events.iter().map(|e| e.frame).collect::<Vec<_>>(), [0, 66_150]);
        assert_eq!(song.length_ms(), 1_500);
    }

    #[test]
    fn the_soundfont_in_an_app_bundle_is_found() {
        let app = std::env::temp_dir().join(format!("popamp-sf-app-{}", std::process::id())).join("Popamp.app/Contents");
        std::fs::create_dir_all(app.join("MacOS")).unwrap();
        std::fs::create_dir_all(app.join("Resources")).unwrap();
        std::fs::write(app.join("Resources").join(BUNDLED_SOUNDFONT), b"x").unwrap();
        assert_eq!(find_soundfont("", Some(&app.join("MacOS"))), Ok(app.join("Resources").join(BUNDLED_SOUNDFONT)));
        let _ = std::fs::remove_dir_all(app.parent().unwrap().parent().unwrap());
    }

    #[test]
    fn the_soundfont_beside_the_program_comes_before_the_systems() {
        let dir = std::env::temp_dir().join(format!("popamp-sf-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join(BUNDLED_SOUNDFONT), b"").unwrap();
        assert_eq!(find_soundfont("", Some(&dir)), Ok(dir.join(BUNDLED_SOUNDFONT)));
        assert_eq!(find_soundfont("/my.sf2", Some(&dir)), Ok(PathBuf::from("/my.sf2")), "the setting wins");
        std::fs::remove_dir_all(&dir).unwrap();
        let fallback = find_soundfont("", Some(&dir));
        assert!(fallback.as_ref().map_or(true, |p| SYSTEM_SOUNDFONTS.iter().any(|s| p == Path::new(s))), "{fallback:?}");
    }

    #[test]
    fn a_missing_soundfont_says_which() {
        let err = MidiSource::open(&fixture("a4.mid"), "/nonexistent/x.sf2").err().unwrap();
        assert!(err.contains("/nonexistent/x.sf2"), "{err}");
    }
}
