//! A window's picture in skin pixels. Drawing calls take skin coordinates;
//! the canvas puts them on a bitmap `scale` times larger, so a vector skin
//! can be drawn straight at screen size instead of being blown up later.

use crate::font::Font;
use crate::scale::Scale;
use popamp_skin::layout::Align;
use popamp_skin::{Bitmap, Rect, Rgba};

pub struct Canvas<'a> {
    pub bitmap: &'a mut Bitmap,
    pub scale: Scale,
}

impl<'a> Canvas<'a> {
    pub fn new(bitmap: &'a mut Bitmap, scale: Scale) -> Self {
        Canvas { bitmap, scale }
    }

    /// The screen rectangle that skin rectangle `x, y, w, h` covers; edges
    /// round the same way everywhere, so neighbours meet without gaps.
    fn screen(&self, x: i32, y: i32, w: i32, h: i32) -> (i32, i32, i32, i32) {
        let s = self.scale;
        let (x0, y0) = (s.px(x), s.px(y));
        (x0, y0, s.px(x + w) - x0, s.px(y + h) - y0)
    }

    pub fn fill(&mut self, x: i32, y: i32, w: i32, h: i32, c: u32) {
        let (x, y, w, h) = self.screen(x, y, w, h);
        self.bitmap.fill(x, y, w, h, c);
    }

    pub fn set(&mut self, x: i32, y: i32, c: u32) {
        self.fill(x, y, 1, 1, c);
    }

    /// Copies a rectangle of `src` to `(dx, dy)`, each skin pixel blown up
    /// to the screen pixels it covers.
    #[allow(clippy::too_many_arguments)]
    pub fn blit(&mut self, src: &Bitmap, sx: i32, sy: i32, w: i32, h: i32, dx: i32, dy: i32) {
        if self.scale.is_one() {
            return self.bitmap.blit(src, sx, sy, w, h, dx, dy);
        }
        let s = self.scale;
        let (x0, y0, pw, ph) = self.screen(dx, dy, w, h);
        for y in y0..y0 + ph {
            let row = sy + s.skin(y) - dy;
            for x in x0..x0 + pw {
                if let Some(p) = src.get(sx + s.skin(x) - dx, row) {
                    self.bitmap.set(x, y, p);
                }
            }
        }
    }

    /// Lays `src` over the screen rectangle that skin rectangle `at`
    /// covers (`src` is that size), clipped to skin rectangle `clip`.
    pub fn overlay(&mut self, src: &Rgba, at: Rect, clip: Rect) {
        let (x, y, _, _) = self.screen(at.x, at.y, at.w, at.h);
        self.bitmap.overlay(src, x, y, self.screen(clip.x, clip.y, clip.w, clip.h));
    }

    /// The screen size of skin rectangle `r`.
    pub fn screen_size(&self, r: Rect) -> (i32, i32) {
        let (_, _, w, h) = self.screen(r.x, r.y, r.w, r.h);
        (w, h)
    }

    /// Draws `text` in a TrueType font `size` skin pixels tall with its top
    /// at `(x, y)`, clipped to columns `clip_x..clip_x + clip_w`.
    #[allow(clippy::too_many_arguments)]
    pub fn text(&mut self, font: &mut Font, x: i32, y: i32, text: &str, size: u32, color: u32, clip: Rect) {
        let s = self.scale;
        let (cx, _, cw, _) = self.screen(clip.x, clip.y, clip.w, clip.h);
        let px = s.px(size as i32) as u32;
        let baseline = s.px(y) + font.ascent(px);
        font.draw_at(self.bitmap, s.px(x), baseline, text, size as f32 * s.factor(), color, cx, cw);
    }

    /// Draws `text` in `r`, `size` skin pixels tall, placed across it by
    /// `align`, with the ink of the characters in `sample` centred from top
    /// to bottom: where the text sits does not hang on how much room a font
    /// leaves above it.
    #[allow(clippy::too_many_arguments)]
    pub fn text_centred(&mut self, font: &mut Font, r: Rect, text: &str, sample: &str, size: u32, color: u32, align: Align) {
        let size = size as f32 * self.scale.factor();
        let (x, y, w, h) = self.screen(r.x, r.y, r.w, r.h);
        let Some(baseline) = centred_baseline(font, sample, size, y, h) else { return };
        let left = match align {
            Align::Left => x,
            Align::Center => x + (w - font.width_at(text, size)) / 2,
            Align::Right => x + w - font.width_at(text, size),
        };
        font.draw_at(self.bitmap, left, baseline, text, size, color, x, w);
    }

    /// Draws `text` `size` skin pixels tall from `x`, its capitals centred
    /// from top to bottom in `clip`, and cut to `clip`'s width.
    pub fn text_line(&mut self, font: &mut Font, x: i32, clip: Rect, text: &str, size: u32, color: u32) {
        let size = size as f32 * self.scale.factor();
        let (cx, cy, cw, ch) = self.screen(clip.x, clip.y, clip.w, clip.h);
        let Some(baseline) = centred_baseline(font, "H", size, cy, ch) else { return };
        font.draw_at(self.bitmap, self.scale.px(x), baseline, text, size, color, cx, cw);
    }

    /// The width of `text` in skin pixels, rounded up.
    pub fn text_width(&self, font: &mut Font, text: &str, size: u32) -> i32 {
        let s = self.scale;
        s.skin_up(font.width_at(text, size as f32 * s.factor()))
    }
}

/// The baseline that centres the ink of `sample` in `h` pixels from `y`.
fn centred_baseline(font: &mut Font, sample: &str, size: f32, y: i32, h: i32) -> Option<i32> {
    let (top, bottom) = font.ink(sample, size)?;
    Some(y + (h - (top - bottom)) / 2 + top)
}

#[cfg(test)]
mod tests {
    use super::*;
    use popamp_core::config::Scaling;

    #[test]
    fn skin_pixels_cover_whole_screen_pixels_without_gaps() {
        let mut b = Bitmap::new(6, 1, 0);
        let mut c = Canvas::new(&mut b, Scale::new(150, Scaling::Sharp));
        c.set(0, 0, 1);
        c.set(1, 0, 2);
        c.fill(2, 0, 2, 1, 3);
        assert_eq!(b.px, [1, 1, 2, 3, 3, 3]);
    }

    #[test]
    fn blit_blows_pixels_up() {
        let mut src = Bitmap::new(2, 1, 0);
        src.px.copy_from_slice(&[7, 9]);
        let mut b = Bitmap::new(5, 2, 0);
        Canvas::new(&mut b, Scale::new(200, Scaling::Sharp)).blit(&src, 0, 0, 2, 1, 0, 0);
        assert_eq!(b.px, [7, 7, 9, 9, 0, 7, 7, 9, 9, 0]);
    }

    #[test]
    fn aligned_text_starts_left_ends_right_or_sits_in_the_middle() {
        let mut font = crate::font::tests::clock_font();
        let r = Rect { x: 0, y: 0, w: 80, h: 16 };
        // The columns the ink covers.
        let mut ink = |align| {
            let mut b = Bitmap::new(80, 16, 0);
            Canvas::new(&mut b, Scale::new(100, Scaling::Sharp)).text_centred(&mut font, r, "00:00", "0123456789", 13, 0xffffff, align);
            let cols: Vec<usize> = (0..80).filter(|&x| (0..16).any(|y| b.px[y * 80 + x] != 0)).collect();
            (cols[0], *cols.last().unwrap())
        };
        let (left, right, middle) = (ink(Align::Left), ink(Align::Right), ink(Align::Center));
        assert!(left.0 <= 2, "{left:?}");
        // The pen ends at the edge; the last digit's own margin is left.
        assert!((72..80).contains(&right.1), "{right:?}");
        assert_eq!(right.1 - right.0, left.1 - left.0, "the same text, moved");
        let (to_right, to_middle) = ((right.0 - left.0) as i32, (middle.0 - left.0) as i32);
        assert!((to_middle - to_right / 2).abs() <= 1, "{left:?} {middle:?} {right:?}");
    }
}
