//! Classic in-window visualizer: spectrum analyzer and oscilloscope (B9.2).

use crate::canvas::Canvas;
use popamp_core::config::Settings;
use rustfft::{num_complex::Complex, Fft, FftPlanner};
use std::sync::Arc;

const FFT_N: usize = 1024;
pub const BARS: usize = 75;
/// Samples across the oscilloscope, as in Winamp.
const SCOPE_N: usize = 576;
/// Traces the afterglow shows, the newest first, each one vis colour
/// darker: 18 to 22.
const GLOW: usize = 5;

pub struct Vis {
    fft: Arc<dyn Fft<f32>>,
    window: Vec<f32>,
    scratch: Vec<Complex<f32>>,
    samples: Vec<f32>,
    bars: [f32; BARS],
    peaks: [f32; BARS],
    peak_hold: [u8; BARS],
    /// The scope's last traces, the newest first.
    traces: Vec<[f32; SCOPE_N]>,
}

impl Vis {
    pub fn new() -> Self {
        let fft = FftPlanner::new().plan_fft_forward(FFT_N);
        let window = (0..FFT_N).map(|i| 0.5 - 0.5 * (2.0 * std::f32::consts::PI * i as f32 / FFT_N as f32).cos()).collect();
        Self {
            fft,
            window,
            scratch: vec![Complex::default(); FFT_N],
            samples: vec![0.0; FFT_N],
            bars: [0.0; BARS],
            peaks: [0.0; BARS],
            peak_hold: [0; BARS],
            traces: vec![[0.0; SCOPE_N]; GLOW],
        }
    }

    pub fn samples_mut(&mut self) -> &mut [f32] {
        &mut self.samples
    }

    /// Decays everything (used when playback stops).
    pub fn silence(&mut self) {
        self.samples.iter_mut().for_each(|s| *s = 0.0);
    }

    /// Updates bar heights (0..=16) from the current samples.
    pub fn analyze(&mut self, s: &Settings, sample_rate: u32) {
        self.traces.rotate_right(1);
        self.traces[0].copy_from_slice(&self.samples[FFT_N - SCOPE_N..]);
        for (i, c) in self.scratch.iter_mut().enumerate() {
            *c = Complex::new(self.samples[i] * self.window[i], 0.0);
        }
        self.fft.process(&mut self.scratch);
        let rate = sample_rate.max(8000) as f32;
        let bin_hz = rate / FFT_N as f32;
        // Log-spaced bands from 40 Hz to 16 kHz (or Nyquist).
        let (lo, hi) = (40f32, (rate / 2.0).min(16_000.0));
        let fall = [0.25, 0.4, 0.6, 0.9, 1.4][s.safalloff.clamp(0, 4) as usize];
        let peak_fall = [0.05, 0.1, 0.2, 0.35, 0.6][s.sa_peak_falloff.clamp(0, 4) as usize];
        for b in 0..BARS {
            let f0 = lo * (hi / lo).powf(b as f32 / BARS as f32);
            let f1 = lo * (hi / lo).powf((b + 1) as f32 / BARS as f32);
            let (i0, i1) = ((f0 / bin_hz) as usize, ((f1 / bin_hz) as usize).max((f0 / bin_hz) as usize + 1));
            let mut mag = 0f32;
            for c in &self.scratch[i0.min(FFT_N / 2)..i1.min(FFT_N / 2)] {
                mag = mag.max(c.norm());
            }
            // Gentle high-frequency tilt so the treble is visible.
            let db = 20.0 * (mag * (1.0 + b as f32 / 25.0) / (FFT_N as f32 / 4.0) + 1e-9).log10();
            let level = ((db + 60.0) / 60.0 * 16.0).clamp(0.0, 16.0);
            self.bars[b] = if level > self.bars[b] { level } else { (self.bars[b] - fall).max(level).max(0.0) };
            if self.bars[b] >= self.peaks[b] {
                self.peaks[b] = self.bars[b];
                self.peak_hold[b] = 6;
            } else if self.peak_hold[b] > 0 {
                self.peak_hold[b] -= 1;
            } else {
                self.peaks[b] = (self.peaks[b] - peak_fall).max(0.0);
            }
        }
    }

    fn background(b: &mut Canvas, x: i32, y: i32, w: i32, h: i32, colors: &[u32; 24]) {
        b.fill(x, y, w, h, colors[0]);
        for yy in (1..h).step_by(2) {
            for xx in (1..w).step_by(2) {
                b.set(x + xx, y + yy, colors[1]);
            }
        }
    }

    /// Draws the analyzer or scope into a `w`x`h` area (16 px tall classic).
    #[allow(clippy::too_many_arguments)]
    pub fn draw(&self, b: &mut Canvas, x: i32, y: i32, w: i32, h: i32, s: &Settings, colors: &[u32; 24]) {
        Self::background(b, x, y, w, h, colors);
        match s.sa {
            1 => self.draw_spectrum(b, x, y, w, h, s, colors),
            2 => self.draw_scope(b, x, y, w, h, s, colors),
            _ => {}
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn draw_spectrum(&self, b: &mut Canvas, x: i32, y: i32, w: i32, h: i32, s: &Settings, colors: &[u32; 24]) {
        let (bar_w, step) = if s.sa_thick { (3, 4) } else { (1, 1) };
        let n = ((w + (step - bar_w)) / step) as usize;
        let scale = h as f32 / 16.0;
        for i in 0..n {
            // Average the underlying bands for thick bars.
            let lo = i * BARS / n;
            let hi = ((i + 1) * BARS / n).max(lo + 1);
            let level = self.bars[lo..hi].iter().fold(0f32, |a, &v| a.max(v)) * scale;
            let peak = self.peaks[lo..hi].iter().fold(0f32, |a, &v| a.max(v)) * scale;
            let height = level.round() as i32;
            let bx = x + i as i32 * step;
            for row in 0..height {
                let yy = y + h - 1 - row;
                // Row 0 at the top of the area maps to colour 2.
                let color_row = match s.sa_style {
                    1 => (height - 1 - row) as usize * 16 / h.max(1) as usize, // fire: from the bar top
                    2 => (h - height) as usize * 16 / h.max(1) as usize,       // line: by height
                    _ => (yy - y) as usize * 16 / h.max(1) as usize,
                };
                b.fill(bx, yy, bar_w, 1, colors[2 + color_row.min(15)]);
            }
            if s.sa_peaks && peak >= 1.0 {
                let py = y + h - peak.round() as i32;
                if py >= y && py < y + h {
                    b.fill(bx, py, bar_w, 1, colors[23]);
                }
            }
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn draw_scope(&self, b: &mut Canvas, x: i32, y: i32, w: i32, h: i32, s: &Settings, colors: &[u32; 24]) {
        let mid = h / 2;
        // Further from the middle, darker.
        let by_height = |yy: i32| colors[18 + ((yy - mid).unsigned_abs() as usize * 5 / (mid as usize + 1)).min(4)];
        let style = match s.osc_style {
            0 => Trace::Dots,
            2 => Trace::Solid,
            _ => Trace::Lines,
        };
        if s.osc_style != 3 {
            return Self::trace(b, (x, y, w, h), &self.traces[0], style, by_height);
        }
        // The older under the newer, so a pixel shows the youngest trace
        // that lit it.
        for (age, trace) in self.traces.iter().enumerate().rev() {
            Self::trace(b, (x, y, w, h), trace, Trace::Lines, |_| colors[18 + age]);
        }
    }

    /// Draws `samples` across the area, coloured by height.
    fn trace(b: &mut Canvas, (x, y, w, h): (i32, i32, i32, i32), samples: &[f32; SCOPE_N], style: Trace, color: impl Fn(i32) -> u32) {
        let mid = h / 2;
        let mut prev: Option<i32> = None;
        for col in 0..w {
            let v = samples[col as usize * SCOPE_N / w as usize];
            let yy = ((mid as f32 - v * mid as f32 * 1.8).round() as i32).clamp(0, h - 1);
            let from = match style {
                Trace::Dots => yy,
                Trace::Lines => prev.unwrap_or(yy),
                Trace::Solid => mid,
            };
            let (a, z) = (from.min(yy), from.max(yy));
            b.fill(x + col, y + a, 1, z - a + 1, color(yy));
            prev = Some(yy);
        }
    }
}

/// How the scope joins its points.
#[derive(Clone, Copy)]
enum Trace {
    Dots,
    Lines,
    /// From the middle to each point.
    Solid,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scale::Scale;
    use popamp_skin::Bitmap;

    /// Colour `n` of the vis palette is `n` itself.
    const COLORS: [u32; 24] = {
        let mut c = [0; 24];
        let mut i = 0;
        while i < 24 {
            c[i] = i as u32;
            i += 1;
        }
        c
    };

    fn frame(v: &mut Vis, level: f32, s: &Settings) {
        v.samples_mut().fill(level);
        v.analyze(s, 44_100);
    }

    /// The colours drawn in the column at `x`, top to bottom.
    fn column(v: &Vis, s: &Settings, x: i32) -> Vec<u32> {
        let mut bitmap = Bitmap::new(76, 16, 0);
        v.draw(&mut Canvas::new(&mut bitmap, Scale::ONE), 0, 0, 76, 16, s, &COLORS);
        (0..16).map(|y| bitmap.px[(y * 76 + x) as usize]).collect()
    }

    #[test]
    fn the_afterglow_fades_a_trace_over_five_frames() {
        let s = Settings { sa: 2, osc_style: 3, ..Settings::default() };
        let mut v = Vis::new();
        frame(&mut v, 0.4, &s);
        let lit = column(&v, &s, 10).iter().position(|&c| c == 18).expect("the newest trace is the brightest");
        let mut seen = vec![];
        for _ in 0..5 {
            frame(&mut v, -0.4, &s);
            seen.push(column(&v, &s, 10)[lit]);
        }
        assert_eq!(seen[..4], [19, 20, 21, 22], "older, darker");
        assert!(seen[4] < 2, "gone after five frames, the background shows");
        let plain = Settings { osc_style: 1, ..s };
        assert!(column(&v, &plain, 10)[lit] < 2, "the plain lines keep no trace");
    }
}
