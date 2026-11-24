//! Skin pixels to screen pixels: window sizes, mouse positions and the
//! scaled picture of a window.

use popamp_core::config::{Scaling, Settings};
use popamp_skin::Bitmap;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Scale {
    /// Screen pixels per 100 skin pixels.
    pct: i32,
    mode: Scaling,
}

impl Scale {
    pub const ONE: Scale = Scale { pct: 100, mode: Scaling::Nearest };

    pub fn of(s: &Settings) -> Scale {
        Scale::new(if s.dsize { s.zoom } else { 100 }, s.wsz_scaling)
    }

    /// `pct` screen pixels per 100 skin pixels.
    pub fn new(pct: i32, mode: Scaling) -> Scale {
        let pct = if mode == Scaling::Integer { pct / 100 * 100 } else { pct };
        Scale { pct: pct.max(100), mode }
    }

    /// The screen length of `n` skin pixels.
    pub fn px(self, n: i32) -> i32 {
        (n * self.pct + 50).div_euclid(100)
    }

    /// How far point `n` in skin pixels lies past the screen pixel
    /// `px(n)` puts it on, in hundredths of a pixel.
    pub fn shift(self, n: i32) -> i32 {
        n * self.pct - 100 * self.px(n)
    }

    pub fn pct(self) -> i32 {
        self.pct
    }

    /// The skin pixel at screen offset `p`.
    pub fn skin(self, p: i32) -> i32 {
        (p * 100).div_euclid(self.pct)
    }

    /// How many skin pixels it takes to cover `p` screen pixels.
    pub fn skin_up(self, p: i32) -> i32 {
        (p * 100 + self.pct - 1).div_euclid(self.pct)
    }

    /// This scale on a screen with `factor` pixels to each unit the
    /// desktop counts in (2 on a Retina Mac): what a window is drawn at.
    pub fn on_screen(self, factor: f64) -> Scale {
        if factor == 1.0 {
            return self;
        }
        Scale::new((self.pct as f64 * factor).round() as i32, self.mode)
    }

    /// Screen pixels per skin pixel.
    pub fn factor(self) -> f32 {
        self.pct as f32 / 100.0
    }

    pub fn is_one(self) -> bool {
        self.pct == 100
    }

    /// Draws `src` scaled to fill the `dst_w` x `dst_h` screen buffer.
    pub fn present(self, src: &Bitmap, dst: &mut [u32], dst_w: usize, dst_h: usize) {
        let (sw, sh) = (src.w.max(0) as usize, src.h.max(0) as usize);
        if sw == 0 || sh == 0 || dst.len() < dst_w * dst_h {
            return;
        }
        if dst_w.is_multiple_of(sw) && dst_h.is_multiple_of(sh) && dst_w / sw == dst_h / sh {
            return whole(src, dst_w / sw, dst, dst_w);
        }
        match self.mode {
            Scaling::Sharp => sharp(src, dst, dst_w, dst_h),
            _ => nearest(src, dst, dst_w, dst_h),
        }
    }
}

/// Whole-multiple upscale: every skin pixel becomes an `s` x `s` block.
fn whole(src: &Bitmap, s: usize, dst: &mut [u32], dst_w: usize) {
    let w = src.w as usize;
    for (y, row) in src.px.chunks_exact(w).enumerate() {
        for sy in 0..s {
            let start = (y * s + sy) * dst_w;
            let Some(out) = dst.get_mut(start..start + w * s) else { return };
            for (x, &p) in row.iter().enumerate() {
                out[x * s..x * s + s].fill(p);
            }
        }
    }
}

fn nearest(src: &Bitmap, dst: &mut [u32], dst_w: usize, dst_h: usize) {
    let (sw, sh) = (src.w as usize, src.h as usize);
    for y in 0..dst_h {
        let row = &src.px[y * sh / dst_h * sw..][..sw];
        for (x, out) in dst[y * dst_w..][..dst_w].iter_mut().enumerate() {
            *out = row[x * sw / dst_w];
        }
    }
}

/// Area average: as if each skin pixel were blown up to a whole multiple
/// and then shrunk with a box filter, so pixel edges stay crisp and every
/// skin pixel keeps the same width on screen, give or take a blended column.
fn sharp(src: &Bitmap, dst: &mut [u32], dst_w: usize, dst_h: usize) {
    let (sw, sh) = (src.w as usize, src.h as usize);
    let xs = shares(sw, dst_w);
    let ys = shares(sh, dst_h);
    for (y, &(sy, wy)) in ys.iter().enumerate() {
        let top = &src.px[sy * sw..][..sw];
        let bottom = &src.px[(sy + 1).min(sh - 1) * sw..][..sw];
        for (x, &(sx, wx)) in xs.iter().enumerate() {
            let nx = (sx + 1).min(sw - 1);
            let w = [wx * wy, (256 - wx) * wy, wx * (256 - wy), (256 - wx) * (256 - wy)];
            let p = [top[sx], top[nx], bottom[sx], bottom[nx]];
            dst[y * dst_w + x] = blend(&p, &w);
        }
    }
}

/// For each screen pixel along an axis (`dst` >= `src`): the first skin
/// pixel it covers and that pixel's share in 1/256ths; the rest of the
/// screen pixel is the next skin pixel.
fn shares(src: usize, dst: usize) -> Vec<(usize, u32)> {
    (0..dst)
        .map(|x| {
            // The screen pixel spans [a, a + src) in 1/dst-ths of a skin pixel.
            let a = x * src;
            let i = a / dst;
            let first = ((i + 1) * dst).min(a + src) - a;
            (i, ((first * 256 + src / 2) / src) as u32)
        })
        .collect()
}

/// Mixes four 0RGB pixels by weights that sum to 65536.
fn blend(p: &[u32; 4], w: &[u32; 4]) -> u32 {
    let channel = |shift: u32| {
        let sum: u32 = p.iter().zip(w).map(|(&c, &k)| ((c >> shift) & 0xff) * k).sum();
        ((sum + 32768) >> 16).min(255) << shift
    };
    channel(16) | channel(8) | channel(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn settings(dsize: bool, zoom: i32, mode: Scaling) -> Settings {
        Settings { dsize, zoom, wsz_scaling: mode, ..Default::default() }
    }

    #[test]
    fn double_size_switches_to_the_chosen_zoom() {
        assert_eq!(Scale::of(&settings(false, 150, Scaling::Sharp)).px(275), 275);
        let s = Scale::of(&settings(true, 150, Scaling::Sharp));
        assert_eq!((s.px(275), s.px(116)), (413, 174));
        assert_eq!((s.skin(0), s.skin(2), s.skin(3), s.skin(412)), (0, 1, 2, 274));
        assert_eq!(Scale::of(&settings(true, 250, Scaling::Integer)).px(10), 20, "integer rounds down");
    }

    fn strip(px: &[u32]) -> Bitmap {
        let mut b = Bitmap::new(px.len() as i32, 1, 0);
        b.px.copy_from_slice(px);
        b
    }

    fn present(s: Scale, src: &Bitmap, w: usize, h: usize) -> Vec<u32> {
        let mut out = vec![0; w * h];
        s.present(src, &mut out, w, h);
        out
    }

    #[test]
    fn whole_multiples_copy_pixels_in_any_mode() {
        let src = strip(&[0x000000, 0xffffff]);
        let sharp = Scale::of(&settings(true, 200, Scaling::Sharp));
        assert_eq!(present(sharp, &src, 4, 2), [0, 0, 0xffffff, 0xffffff, 0, 0, 0xffffff, 0xffffff]);
    }

    #[test]
    fn sharp_blends_only_the_pixel_on_an_edge() {
        // Two pixels at x1.5: black, a half-and-half column, white.
        let src = strip(&[0x000000, 0xffffff]);
        let sharp = Scale::of(&settings(true, 150, Scaling::Sharp));
        assert_eq!(present(sharp, &src, 3, 1), [0x000000, 0x808080, 0xffffff]);
        let nearest = Scale::of(&settings(true, 150, Scaling::Nearest));
        assert_eq!(present(nearest, &src, 3, 1), [0x000000, 0x000000, 0xffffff]);
    }

    #[test]
    fn flat_areas_stay_flat() {
        let src = strip(&[0x123456; 7]);
        let sharp = Scale::of(&settings(true, 175, Scaling::Sharp));
        assert!(present(sharp, &src, 12, 2).iter().all(|&p| p == 0x123456));
    }

    #[test]
    fn a_window_draws_at_its_screens_density() {
        let s = Scale::new(150, Scaling::Sharp);
        assert_eq!(s.on_screen(2.0).px(100), 300, "x1.5 on a Retina screen");
        assert_eq!(s.on_screen(1.0), s);
        assert_eq!(Scale::new(150, Scaling::Integer).on_screen(2.0).px(10), 20, "whole multiples of what the zoom gives");
    }
}
