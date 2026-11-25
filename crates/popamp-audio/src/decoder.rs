//! Symphonia-based decoding to interleaved stereo `f32`, plus tag reading.

use crate::stream::TitleSlot;
use popamp_core::meta::{parse_gain, TrackMeta};
use std::path::Path;
use std::sync::LazyLock;
use symphonia::core::audio::SampleBuffer;
use symphonia::core::codecs::{CodecRegistry, DecoderOptions, CODEC_TYPE_NULL};
use symphonia::core::errors::Error as SymError;
use symphonia::core::formats::{FormatOptions, FormatReader, SeekMode, SeekTo, Track};
use symphonia::core::io::{BufReader, MediaSource, MediaSourceStream};
use symphonia::core::meta::{MetadataBuilder, MetadataOptions, MetadataRevision, StandardTagKey};
use symphonia::core::probe::Hint;
use symphonia::core::units::Time;

#[derive(Debug, Clone, Default, PartialEq)]
pub struct StreamInfo {
    pub sample_rate: u32,
    /// Channels in the source (output is always stereo).
    pub channels: u16,
    pub length_ms: Option<u64>,
    pub bitrate_kbps: Option<u32>,
    pub codec: String,
    pub seekable: bool,
}

pub enum Status {
    Data,
    Eof,
}

/// Anything the engine can play: produces interleaved stereo `f32`.
pub trait AudioSource: Send {
    fn info(&self) -> &StreamInfo;
    /// Appends the next chunk of samples to `out`.
    fn next(&mut self, out: &mut Vec<f32>) -> Result<Status, String>;
    /// Seeks to `ms`, returns the position actually reached.
    fn seek(&mut self, ms: u64) -> Result<u64, String>;
}

impl AudioSource for Decoder {
    fn info(&self) -> &StreamInfo {
        &self.info
    }
    fn next(&mut self, out: &mut Vec<f32>) -> Result<Status, String> {
        Decoder::next(self, out)
    }
    fn seek(&mut self, ms: u64) -> Result<u64, String> {
        Decoder::seek(self, ms)
    }
}

pub struct Decoder {
    reader: Box<dyn FormatReader>,
    codec: Box<dyn symphonia::core::codecs::Decoder>,
    track_id: u32,
    pub info: StreamInfo,
    sample_buf: Option<SampleBuffer<f32>>,
    skip_frames: u64,
    byte_len: Option<u64>,
    // Running bitrate estimate.
    bytes_acc: u64,
    frames_acc: u64,
    packets_seen: u64,
    length_known: bool,
    /// Where to name each next song of a chain, for a radio.
    songs: Option<TitleSlot>,
}

/// Symphonia's codecs plus Opus.
fn codecs() -> &'static CodecRegistry {
    static CODECS: LazyLock<CodecRegistry> = LazyLock::new(|| {
        let mut registry = CodecRegistry::new();
        symphonia::default::register_enabled_codecs(&mut registry);
        registry.register_all::<crate::opus::OpusDecoder>();
        registry
    });
    &CODECS
}

pub(crate) fn hint_for(ext: Option<&str>) -> Hint {
    let mut hint = Hint::new();
    if let Some(e) = ext {
        hint.with_extension(e);
    }
    hint
}

/// The ID3v1 tag in the last 128 bytes of the file, which symphonia does
/// not look for. Old MP3s often have no other; after an ID3v2 tag it only
/// fills what that one lacks. Track 0 stands for none.
fn id3v1(path: &Path) -> Option<MetadataRevision> {
    use std::io::{Read, Seek, SeekFrom};
    let mut file = std::fs::File::open(path).ok()?;
    file.seek(SeekFrom::End(-128)).ok()?;
    let mut tail = [0; 128];
    file.read_exact(&mut tail).ok()?;
    let mut read = MetadataBuilder::new();
    symphonia_metadata::id3v1::read_id3v1(&mut BufReader::new(&tail), &mut read).ok()?;
    let mut kept = MetadataBuilder::new();
    for tag in read.metadata().tags() {
        if !(tag.std_key == Some(StandardTagKey::TrackNumber) && tag.value.to_string() == "0") {
            kept.add_tag(tag.clone());
        }
    }
    Some(kept.metadata())
}

fn apply_tags(meta: &mut TrackMeta, rev: &MetadataRevision) {
    // The values of one tag were written in one code page: judge them together.
    let mut values: Vec<String> = rev.tags().iter().map(|t| t.value.to_string()).collect();
    popamp_core::text::redecode_latin1(values.iter_mut());
    for (tag, value) in rev.tags().iter().zip(&values) {
        let value = value.trim();
        if value.is_empty() {
            continue;
        }
        let set = |slot: &mut Option<String>| {
            if slot.is_none() {
                *slot = Some(value.to_string());
            }
        };
        match tag.std_key {
            Some(StandardTagKey::TrackTitle) => set(&mut meta.title),
            Some(StandardTagKey::Artist) => set(&mut meta.artist),
            Some(StandardTagKey::Album) => set(&mut meta.album),
            Some(StandardTagKey::AlbumArtist) => set(&mut meta.album_artist),
            Some(StandardTagKey::Date) | Some(StandardTagKey::OriginalDate) => set(&mut meta.year),
            Some(StandardTagKey::Genre) => set(&mut meta.genre),
            Some(StandardTagKey::TrackNumber) => set(&mut meta.track),
            Some(StandardTagKey::DiscNumber) => set(&mut meta.disc),
            Some(StandardTagKey::Comment) => set(&mut meta.comment),
            Some(StandardTagKey::Composer) => set(&mut meta.composer),
            Some(StandardTagKey::ReplayGainTrackGain) => meta.replaygain_track_gain = meta.replaygain_track_gain.or(parse_gain(value)),
            Some(StandardTagKey::ReplayGainAlbumGain) => meta.replaygain_album_gain = meta.replaygain_album_gain.or(parse_gain(value)),
            Some(StandardTagKey::ReplayGainTrackPeak) => meta.replaygain_track_peak = meta.replaygain_track_peak.or(parse_gain(value)),
            Some(StandardTagKey::ReplayGainAlbumPeak) => meta.replaygain_album_peak = meta.replaygain_album_peak.or(parse_gain(value)),
            _ => {}
        }
    }
}

impl Decoder {
    /// Opens a local file. Returns the decoder and the tags found while probing.
    pub fn open_file(path: &Path) -> Result<(Decoder, TrackMeta), String> {
        let file = std::fs::File::open(path).map_err(|e| format!("{}: {e}", path.display()))?;
        let ext = path.extension().and_then(|e| e.to_str()).map(str::to_ascii_lowercase);
        let (dec, mut meta) = Self::open_source(Box::new(file), ext.as_deref(), true)?;
        if let Some(rev) = id3v1(path) {
            apply_tags(&mut meta, &rev);
        }
        Ok((dec, meta))
    }

    pub fn open_source(source: Box<dyn MediaSource>, ext: Option<&str>, seekable: bool) -> Result<(Decoder, TrackMeta), String> {
        let byte_len = source.byte_len();
        let mss = MediaSourceStream::new(source, Default::default());
        let fmt_opts = FormatOptions { enable_gapless: true, ..Default::default() };
        let mut probed = symphonia::default::get_probe()
            .format(&hint_for(ext), mss, &fmt_opts, &MetadataOptions::default())
            .map_err(|e| format!("unsupported format: {e}"))?;
        let mut meta = TrackMeta::default();
        if let Some(rev) = probed.metadata.get().as_ref().and_then(|m| m.current()) {
            apply_tags(&mut meta, rev);
        }
        let mut reader = probed.format;
        if let Some(rev) = reader.metadata().current() {
            apply_tags(&mut meta, rev);
        }
        let (track, codec) = audio_track(reader.as_ref())?;
        let params = &track.codec_params;
        let sample_rate = params.sample_rate.unwrap_or(44_100);
        let channels = params.channels.map(|c| c.count() as u16).unwrap_or(2);
        // `n_frames` is in time-base units (milliseconds for Matroska), not samples.
        let length_ms = params.n_frames.map(|n| match params.time_base {
            Some(tb) => {
                let t = tb.calc_time(n);
                t.seconds * 1000 + (t.frac * 1000.0) as u64
            }
            None => n * 1000 / sample_rate as u64,
        });
        let codec_name = codecs().get_codec(params.codec).map(|d| d.short_name.to_uppercase()).unwrap_or_default();
        let bitrate_kbps = match (params.bits_per_sample, length_ms, byte_len) {
            // Lossless/PCM: average over the file is the honest figure.
            (_, Some(l), Some(b)) if l > 0 => Some((b * 8 / l) as u32),
            _ => None,
        };
        meta.length_ms = length_ms;
        meta.sample_rate = Some(sample_rate);
        meta.channels = Some(channels);
        meta.codec = Some(codec_name.clone());
        meta.bitrate_kbps = bitrate_kbps;
        let info = StreamInfo { sample_rate, channels, length_ms, bitrate_kbps, codec: codec_name, seekable };
        Ok((
            Decoder {
                reader,
                codec,
                track_id: track.id,
                info,
                sample_buf: None,
                skip_frames: 0,
                byte_len,
                bytes_acc: 0,
                frames_acc: 0,
                packets_seen: 0,
                length_known: length_ms.is_some(),
                songs: None,
            },
            meta,
        ))
    }

    /// Names each next song of a chain in `songs`, as a radio's titles are.
    pub(crate) fn name_songs_in(&mut self, songs: TitleSlot) {
        self.songs = Some(songs);
    }

    /// Names the song of the stream just started, from its tags.
    fn name_song(&mut self) {
        let Some(songs) = &self.songs else { return };
        let mut meta = TrackMeta::default();
        if let Some(rev) = self.reader.metadata().skip_to_latest() {
            apply_tags(&mut meta, rev);
        }
        if let Some(name) = song_name(&meta) {
            *songs.lock() = Some(name);
        }
    }

    /// Appends the next chunk of interleaved stereo samples to `out`.
    pub fn next(&mut self, out: &mut Vec<f32>) -> Result<Status, String> {
        loop {
            let packet = match self.reader.next_packet() {
                Ok(p) => p,
                Err(SymError::IoError(e)) if e.kind() == std::io::ErrorKind::UnexpectedEof => return Ok(Status::Eof),
                // A new stream of a chain (the next song of an Ogg radio):
                // a new track, maybe with another codec setup.
                Err(SymError::ResetRequired) => {
                    let (track, codec) = audio_track(self.reader.as_ref())?;
                    (self.track_id, self.codec) = (track.id, codec);
                    self.name_song();
                    continue;
                }
                Err(e) => return Err(format!("read error: {e}")),
            };
            if packet.track_id() != self.track_id {
                continue;
            }
            self.track_bitrate(packet.data.len() as u64, packet.dur);
            let decoded = match self.codec.decode(&packet) {
                Ok(d) => d,
                Err(SymError::DecodeError(e)) => {
                    log::debug!("skipping corrupt packet: {e}");
                    continue;
                }
                Err(SymError::IoError(e)) if e.kind() == std::io::ErrorKind::UnexpectedEof => return Ok(Status::Eof),
                Err(e) => return Err(format!("decode error: {e}")),
            };
            let spec = *decoded.spec();
            let frames = decoded.frames();
            if frames == 0 {
                continue;
            }
            if spec.rate != self.info.sample_rate {
                self.info.sample_rate = spec.rate;
            }
            let buf = match &mut self.sample_buf {
                Some(b) if b.capacity() >= decoded.capacity() * spec.channels.count() => b,
                slot => slot.insert(SampleBuffer::new(decoded.capacity() as u64, spec)),
            };
            buf.copy_interleaved_ref(decoded);
            let ch = spec.channels.count().max(1);
            let samples = &buf.samples()[..frames * ch];
            let skip = (self.skip_frames as usize).min(frames);
            self.skip_frames -= skip as u64;
            to_stereo(&samples[skip * ch..], ch, out);
            if out.is_empty() {
                continue;
            }
            return Ok(Status::Data);
        }
    }

    fn track_bitrate(&mut self, bytes: u64, dur: u64) {
        self.packets_seen += 1;
        self.bytes_acc += bytes;
        self.frames_acc += dur;
        // Re-evaluate about twice a second of audio.
        if self.frames_acc >= self.info.sample_rate as u64 / 2 && self.frames_acc > 0 {
            let kbps = (self.bytes_acc * 8 * self.info.sample_rate as u64 / self.frames_acc / 1000) as u32;
            if !self.length_known {
                if let (Some(len), true) = (self.byte_len, kbps > 0) {
                    self.info.length_ms = Some(len * 8 / kbps as u64);
                }
            }
            if self.info.codec != "FLAC" && !self.info.codec.starts_with("PCM") {
                self.info.bitrate_kbps = Some(kbps);
            }
            self.bytes_acc = 0;
            self.frames_acc = 0;
        }
    }

    /// Seeks to `ms`, returns the position actually reached.
    pub fn seek(&mut self, ms: u64) -> Result<u64, String> {
        let time = Time::new(ms / 1000, (ms % 1000) as f64 / 1000.0);
        let seeked = self
            .reader
            .seek(SeekMode::Accurate, SeekTo::Time { time, track_id: Some(self.track_id) })
            .map_err(|e| format!("seek failed: {e}"))?;
        self.codec.reset();
        self.skip_frames = seeked.required_ts.saturating_sub(seeked.actual_ts);
        let tb = self.reader.tracks().iter().find(|t| t.id == self.track_id).and_then(|t| t.codec_params.time_base);
        let reached = match tb {
            Some(tb) => {
                let t = tb.calc_time(seeked.required_ts);
                t.seconds * 1000 + (t.frac * 1000.0) as u64
            }
            None => ms,
        };
        Ok(reached)
    }
}

/// "Artist - Title", as radios name a song, or the title alone.
fn song_name(meta: &TrackMeta) -> Option<String> {
    let title = meta.title.as_deref()?;
    Some(meta.artist.as_ref().map_or_else(|| title.to_string(), |artist| format!("{artist} - {title}")))
}

/// The first audio track of `reader`, and a decoder for it.
fn audio_track(reader: &dyn FormatReader) -> Result<(Track, Box<dyn symphonia::core::codecs::Decoder>), String> {
    let track = reader.tracks().iter().find(|t| t.codec_params.codec != CODEC_TYPE_NULL).ok_or("no audio track")?.clone();
    let codec = codecs().make(&track.codec_params, &DecoderOptions::default()).map_err(|e| format!("unsupported codec: {e}"))?;
    Ok((track, codec))
}

/// Converts interleaved `ch`-channel audio to stereo and appends it.
fn to_stereo(samples: &[f32], ch: usize, out: &mut Vec<f32>) {
    match ch {
        1 => out.extend(samples.iter().flat_map(|&s| [s, s])),
        2 => out.extend_from_slice(samples),
        _ => {
            // Simple fold-down: front L/R plus the rest spread to both sides.
            for frame in samples.chunks_exact(ch) {
                let rest: f32 = frame[2..].iter().sum::<f32>() * (0.5 / (ch - 2) as f32);
                out.push((frame[0] + rest) * 0.7);
                out.push((frame[1] + rest) * 0.7);
            }
        }
    }
}

/// Reads tags and stream properties without decoding the audio.
pub fn read_meta(path: &Path) -> Result<TrackMeta, String> {
    let (mut dec, mut meta) = Decoder::open_file(path)?;
    if meta.length_ms.is_none() {
        // No frame count in the header (e.g. CBR MP3 without Xing): estimate from a few packets.
        let mut scratch = Vec::new();
        for _ in 0..40 {
            scratch.clear();
            match dec.next(&mut scratch) {
                Ok(Status::Data) if dec.info.length_ms.is_some() => break,
                Ok(Status::Data) => {}
                _ => break,
            }
        }
        meta.length_ms = dec.info.length_ms;
    }
    meta.bitrate_kbps = meta.bitrate_kbps.or(dec.info.bitrate_kbps);
    Ok(meta)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stereo_conversion() {
        let mut out = vec![];
        to_stereo(&[0.1, 0.2], 1, &mut out);
        assert_eq!(out, [0.1, 0.1, 0.2, 0.2]);
        out.clear();
        to_stereo(&[1.0, 0.0, 0.0, 0.0, 0.0, 0.0], 6, &mut out);
        assert_eq!(out, [0.7, 0.0]);
    }

    fn fixture(name: &str) -> std::path::PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures").join(name)
    }

    /// Decodes the whole file; returns what it says about itself and the samples.
    fn decode_all(path: &Path) -> (StreamInfo, Vec<f32>) {
        let (mut dec, _) = Decoder::open_file(path).unwrap();
        let mut out = Vec::new();
        while let Status::Data = dec.next(&mut out).unwrap() {}
        (dec.info.clone(), out)
    }

    fn rms(samples: &[f32]) -> f32 {
        (samples.iter().map(|s| s * s).sum::<f32>() / samples.len() as f32).sqrt()
    }

    #[test]
    fn opus_in_ogg_and_webm_plays_its_one_second_tone() {
        for name in ["tone.opus", "tone.webm"] {
            let (info, out) = decode_all(&fixture(name));
            assert_eq!((info.codec.as_str(), info.sample_rate, info.channels), ("OPUS", 48_000, 2), "{name}");
            let frames = out.len() / 2;
            assert!((47_000..=49_000).contains(&frames), "{name}: {frames} frames");
            // A sine at -6 dB: amplitude 0.5, RMS 0.354.
            let level = rms(&out[4_800..out.len() - 4_800]);
            assert!((0.32..0.39).contains(&level), "{name}: RMS {level}");
        }
    }

    /// `first`, then `second`, as an Ogg radio sends one song after another:
    /// a chain of two streams, the second with a serial number of its own.
    /// Read as a stream is, without seeking.
    fn chained(first: &str, second: &str) -> Decoder {
        let one = std::fs::read(fixture(first)).unwrap();
        let two = with_serial(&std::fs::read(fixture(second)).unwrap(), 0x5EC0_0D02);
        let radio = symphonia::core::io::ReadOnlySource::new(std::io::Cursor::new([one, two].concat()));
        Decoder::open_source(Box::new(radio), None, false).unwrap().0
    }

    /// The Ogg pages of `ogg` given the serial number `serial`.
    fn with_serial(ogg: &[u8], serial: u32) -> Vec<u8> {
        let mut out = ogg.to_vec();
        let mut at = 0;
        while at + 27 <= out.len() {
            assert_eq!(&out[at..at + 4], b"OggS", "a page at {at}");
            let segments = out[at + 26] as usize;
            let body: usize = out[at + 27..at + 27 + segments].iter().map(|&n| n as usize).sum();
            let end = at + 27 + segments + body;
            out[at + 14..at + 18].copy_from_slice(&serial.to_le_bytes());
            out[at + 22..at + 26].fill(0);
            let crc = ogg_crc(&out[at..end]);
            out[at + 22..at + 26].copy_from_slice(&crc.to_le_bytes());
            at = end;
        }
        out
    }

    /// The CRC of an Ogg page: polynomial 0x04C11DB7, not reflected, from 0.
    fn ogg_crc(page: &[u8]) -> u32 {
        page.iter().fold(0u32, |crc, &byte| {
            (0..8).fold(crc ^ (u32::from(byte) << 24), |c, _| if c & 0x8000_0000 != 0 { (c << 1) ^ 0x04C1_1DB7 } else { c << 1 })
        })
    }

    #[test]
    fn a_page_given_its_own_serial_still_reads() {
        let one = std::fs::read(fixture("tone.opus")).unwrap();
        assert_eq!(with_serial(&with_serial(&one, 7), u32::from_le_bytes(one[14..18].try_into().unwrap())), one);
    }

    // Each new song of an Ogg radio is a new stream, with a new track id;
    // waiting for the old one's packets skipped the rest, and on a radio,
    // which never ends, never came back to the player.
    #[test]
    fn a_chained_ogg_plays_every_stream_in_it() {
        for name in ["tone.opus", "silence.ogg"] {
            let (_, once) = decode_all(&fixture(name));
            let (mut dec, mut twice) = (chained(name, name), Vec::new());
            while let Status::Data = dec.next(&mut twice).unwrap() {}
            let (once, twice) = (once.len() / 2, twice.len() / 2);
            assert!(twice.abs_diff(2 * once) < once / 20, "{name}: {once} frames once, {twice} chained");
        }
    }

    // An Ogg radio names the next song in the tags of its stream, not in
    // ICY metadata.
    #[test]
    fn the_next_stream_of_a_chain_names_its_song() {
        let (mut dec, titles) = (chained("tone.opus", "tone-tagged.opus"), TitleSlot::default());
        dec.name_songs_in(titles.clone());
        while let Status::Data = dec.next(&mut Vec::new()).unwrap() {}
        assert_eq!(titles.lock().as_deref(), Some("Band - Second"));
    }

    #[test]
    fn a_song_is_named_by_artist_and_title_or_its_title_alone() {
        let meta = |artist: Option<&str>, title: Option<&str>| TrackMeta {
            artist: artist.map(Into::into),
            title: title.map(Into::into),
            ..Default::default()
        };
        assert_eq!(song_name(&meta(Some("Band"), Some("Song"))).as_deref(), Some("Band - Song"));
        assert_eq!(song_name(&meta(None, Some("Song"))).as_deref(), Some("Song"));
        assert_eq!(song_name(&meta(Some("Band"), None)), None);
    }

    #[test]
    fn an_opus_file_seeks_to_the_middle() {
        let (mut dec, _) = Decoder::open_file(&fixture("tone.opus")).unwrap();
        let reached = dec.seek(500).unwrap();
        let mut out = Vec::new();
        while let Status::Data = dec.next(&mut out).unwrap() {}
        let left = 48 * (1_000 - reached as usize);
        assert!(out.len() / 2 > left - 1_000 && out.len() / 2 < left + 1_000, "{} frames after {reached} ms", out.len() / 2);
    }
}
