//! A vector skin's SVG pictures, rasterized for the size they are shown at
//! and kept until the skin changes.

use crate::bitmap::Rgba;
use crate::sprites::Rect;
use resvg::{tiny_skia, usvg};
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

/// Picture number, area of it, and how it is drawn.
type Key = (u16, Rect, Fit);

/// How an area of a picture is drawn: stretched to a size, or placed.
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
enum Fit {
    Stretch(i32, i32),
    Placed(Placement),
}

/// Where an area of a picture lands on the screen: in `w` x `h` pixels,
/// drawn `pct` percent big, its exact top left corner `dx`, `dy`
/// hundredths of a pixel right of and below the pixel it is rounded to.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct Placement {
    pub w: i32,
    pub h: i32,
    pub pct: i32,
    pub dx: i32,
    pub dy: i32,
}

/// Options under which a picture may only use what it carries inside itself.
/// A skin can come from anywhere, so its `<image>` must not reach files on disk.
fn self_contained(style_sheet: Option<String>) -> usvg::Options<'static> {
    usvg::Options {
        style_sheet,
        image_href_resolver: usvg::ImageHrefResolver { resolve_string: Box::new(|_, _| None), ..Default::default() },
        ..Default::default()
    }
}

#[derive(Default)]
pub struct Svgs {
    trees: Vec<usvg::Tree>,
    cache: RefCell<HashMap<Key, Rc<Rgba>>>,
}

impl Svgs {
    /// Parses an SVG file and returns its number. `css` is applied over
    /// the file's own styles: it is how a skin's colour set reaches it.
    pub fn add(&mut self, data: &[u8], css: Option<&str>) -> Result<u16, String> {
        let id = u16::try_from(self.trees.len()).map_err(|_| "too many SVG files".to_string())?;
        let tree = usvg::Tree::from_data(data, &self_contained(css.map(str::to_string))).map_err(|e| e.to_string())?;
        self.trees.push(tree);
        Ok(id)
    }

    /// The size of picture `id` in its own units.
    pub fn size(&self, id: u16) -> Option<(f32, f32)> {
        self.trees.get(id as usize).map(|t| (t.size().width(), t.size().height()))
    }

    /// Area `src` of picture `id` drawn to fill `w` x `h` pixels.
    pub fn raster(&self, id: u16, src: Rect, w: i32, h: i32) -> Option<Rc<Rgba>> {
        self.cached((id, src, Fit::Stretch(w, h)))
    }

    /// Area `src` of picture `id` drawn where `at` puts it. Every area is
    /// drawn at the same scale and shifted by the part of a pixel its
    /// corner falls inside, so the pieces of a window line up as in the
    /// picture they come from, whatever size each one is rounded to.
    pub fn raster_placed(&self, id: u16, src: Rect, at: Placement) -> Option<Rc<Rgba>> {
        self.cached((id, src, Fit::Placed(at)))
    }

    fn cached(&self, key: Key) -> Option<Rc<Rgba>> {
        if let Some(r) = self.cache.borrow().get(&key) {
            return Some(r.clone());
        }
        let raster = Rc::new(self.draw(key)?);
        let mut cache = self.cache.borrow_mut();
        // Old scales pile up as the user changes the size; start over then.
        if cache.len() > 2048 {
            cache.clear();
        }
        cache.insert(key, raster.clone());
        Some(raster)
    }

    fn draw(&self, (id, src, fit): Key) -> Option<Rgba> {
        let tree = self.trees.get(id as usize)?;
        let (w, h) = fit.size();
        let mut pixmap = tiny_skia::Pixmap::new(w.max(1) as u32, h.max(1) as u32)?;
        resvg::render(tree, fit.transform(src), &mut pixmap.as_mut());
        let mut px: Vec<u32> = pixmap.pixels().iter().map(|p| u32::from_be_bytes([p.alpha(), p.red(), p.green(), p.blue()])).collect();
        if let Fit::Placed(at) = fit {
            fill_edges(&mut px, src, at);
        }
        Some(Rgba { w: pixmap.width() as i32, h: pixmap.height() as i32, px })
    }
}

impl Fit {
    fn size(self) -> (i32, i32) {
        match self {
            Fit::Stretch(w, h) => (w, h),
            Fit::Placed(at) => (at.w, at.h),
        }
    }

    /// From the picture's units to the pixels area `src` is drawn into.
    fn transform(self, src: Rect) -> tiny_skia::Transform {
        let (sx, sy, ox, oy) = match self {
            Fit::Stretch(w, h) => (w as f32 / src.w.max(1) as f32, h as f32 / src.h.max(1) as f32, 0.0, 0.0),
            Fit::Placed(at) => {
                let f = at.pct as f32 / 100.0;
                (f, f, at.dx as f32 / 100.0, at.dy as f32 / 100.0)
            }
        };
        tiny_skia::Transform::from_row(sx, 0.0, 0.0, sy, ox - src.x as f32 * sx, oy - src.y as f32 * sy)
    }
}

/// Gives the pixels at the ends of `px`, area `src` drawn where `at` puts
/// it, that the area covers only in part the colour of their neighbours.
fn fill_edges(px: &mut [u32], src: Rect, at: Placement) {
    let (w, h) = (at.w.max(1) as usize, at.h.max(1) as usize);
    for (x, from) in edges(at.dx, src.w * at.pct, w) {
        for y in 0..h {
            px[y * w + x] = px[y * w + from];
        }
    }
    for (y, from) in edges(at.dy, src.h * at.pct, h) {
        px.copy_within(from * w..(from + 1) * w, y * w);
    }
}

/// The end pixels of `n` that a span `len` long from `start` (both in
/// hundredths of a pixel) covers only in part, each with the pixel next
/// to it inward. What is drawn there
/// partly comes from beyond the area: from the next piece of the picture,
/// or from nothing.
fn edges(start: i32, len: i32, n: usize) -> Vec<(usize, usize)> {
    if n < 2 {
        return Vec::new();
    }
    let mut out = Vec::new();
    if start > 0 {
        out.push((0, 1));
    }
    if start + len < n as i32 * 100 {
        out.push((n - 1, n - 2));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    const HALF_RED: &[u8] = br##"<svg xmlns="http://www.w3.org/2000/svg" width="2" height="1">
        <rect x="0" y="0" width="1" height="1" fill="#ff0000"/></svg>"##;

    /// 30 x 1: a red stripe one unit wide at x = 10, the rest left empty.
    const STRIPE: &[u8] = br##"<svg xmlns="http://www.w3.org/2000/svg" width="30" height="1">
        <rect x="10" y="0" width="1" height="1" fill="#ff0000"/></svg>"##;

    fn at_150(s: &Svgs, id: u16, src: Rect, w: i32) -> Vec<u32> {
        s.raster_placed(id, src, Placement { w, h: 2, pct: 150, dx: 0, dy: 0 }).unwrap().px[..w as usize].to_vec()
    }

    #[test]
    fn pieces_of_any_width_draw_what_they_share_alike() {
        let mut s = Svgs::default();
        let id = s.add(STRIPE, None).unwrap();
        // 25 units are 37.5 pixels at 150 %, shown in 38; 12 units are 18.
        let wide = at_150(&s, id, Rect { x: 0, y: 0, w: 25, h: 1 }, 38);
        let narrow = at_150(&s, id, Rect { x: 0, y: 0, w: 12, h: 1 }, 18);
        assert_eq!(wide[..18], narrow[..18]);
    }

    #[test]
    fn a_piece_cut_inside_a_pixel_fills_that_pixel_with_its_own_colour() {
        let red_then_white = br##"<svg xmlns="http://www.w3.org/2000/svg" width="4" height="1">
            <rect width="3" height="1" fill="#ff0000"/><rect x="3" width="1" height="1" fill="#ffffff"/></svg>"##;
        let mut s = Svgs::default();
        let id = s.add(red_then_white, None).unwrap();
        let red = Rect { x: 0, y: 0, w: 3, h: 1 };
        // 3 units are 4.5 pixels at 150 %, shown in 5: the last one half covered.
        let r = s.raster_placed(id, red, Placement { w: 5, h: 2, pct: 150, dx: 0, dy: 0 }).unwrap();
        assert_eq!(r.px[4], 0xffff0000, "no white from beyond the edge");
        // Starting half a pixel late, the first one is half covered too.
        let r = s.raster_placed(id, red, Placement { w: 5, h: 2, pct: 150, dx: 50, dy: 0 }).unwrap();
        assert_eq!((r.px[0], r.px[4]), (0xffff0000, 0xffff0000));
    }

    #[test]
    fn pictures_render_at_the_asked_size_and_area() {
        let mut s = Svgs::default();
        let id = s.add(HALF_RED, None).unwrap();
        let full = s.raster(id, Rect { x: 0, y: 0, w: 2, h: 1 }, 4, 2).unwrap();
        assert_eq!(full.px, [0xffff0000, 0xffff0000, 0, 0, 0xffff0000, 0xffff0000, 0, 0]);
        let right = s.raster(id, Rect { x: 1, y: 0, w: 1, h: 1 }, 3, 3).unwrap();
        assert!(right.px.iter().all(|&p| p == 0), "the right half is empty");
    }

    fn red_png() -> Vec<u8> {
        let mut pixmap = tiny_skia::Pixmap::new(1, 1).unwrap();
        pixmap.fill(tiny_skia::Color::from_rgba8(255, 0, 0, 255));
        pixmap.encode_png().unwrap()
    }

    fn data_url(mime: &str, bytes: &[u8]) -> String {
        let escaped: String = bytes.iter().map(|b| format!("%{b:02x}")).collect();
        format!("data:{mime},{escaped}")
    }

    fn picture_of(href: &str) -> Vec<u8> {
        format!(
            r#"<svg xmlns="http://www.w3.org/2000/svg" width="1" height="1">
            <image href="{href}" width="1" height="1"/></svg>"#
        )
        .into_bytes()
    }

    fn drawn(svg: &[u8]) -> u32 {
        let mut s = Svgs::default();
        let id = s.add(svg, None).unwrap();
        s.raster(id, Rect { x: 0, y: 0, w: 1, h: 1 }, 1, 1).unwrap().px[0]
    }

    fn styled(body: &str, css: &str) -> u32 {
        let svg = format!(r#"<svg xmlns="http://www.w3.org/2000/svg" width="1" height="1">{body}</svg>"#);
        let mut s = Svgs::default();
        let id = s.add(svg.as_bytes(), Some(css)).unwrap();
        s.raster(id, Rect { x: 0, y: 0, w: 1, h: 1 }, 1, 1).unwrap().px[0]
    }

    const BLUE: u32 = 0xff0000ff;

    #[test]
    fn a_style_sheet_recolours_what_carries_its_class() {
        let css = ".c { fill: #0000ff !important } .c-stroke { stroke: #0000ff !important } .c-stop { stop-color: #0000ff !important }";
        assert_eq!(styled(r##"<rect class="c" width="1" height="1" fill="#ff0000"/>"##, css), BLUE, "over the fill attribute");
        assert_eq!(styled(r##"<rect class="c" width="1" height="1" style="fill:#ff0000"/>"##, css), BLUE, "over an inline style");
        assert_eq!(styled(r##"<rect width="1" height="1" fill="#ff0000"/>"##, css), 0xffff0000, "the rest keeps its colour");
        assert_eq!(styled(r##"<g class="c"><rect width="1" height="1"/></g>"##, css), BLUE, "a group passes it down");
        assert_eq!(
            styled(r##"<path class="c-stroke" d="M0 0 L1 1" fill="none" stroke="#ff0000" stroke-width="4"/>"##, css),
            BLUE,
            "a line's colour"
        );
        let gradient = r##"<linearGradient id="g"><stop class="c-stop" offset="0" stop-color="#ff0000"/>
            <stop class="c-stop" offset="1" stop-color="#ff0000"/></linearGradient><rect width="1" height="1" fill="url(#g)"/>"##;
        assert_eq!(styled(gradient, css), BLUE, "a gradient's colours");
    }

    /// A real picture on disk, removed when the test ends.
    struct PngOnDisk(std::path::PathBuf);

    impl PngOnDisk {
        fn new(name: &str) -> Self {
            let path = std::env::temp_dir().join(format!("popamp-svg-{name}-{}.png", std::process::id()));
            std::fs::write(&path, red_png()).unwrap();
            Self(path)
        }
    }

    impl Drop for PngOnDisk {
        fn drop(&mut self) {
            let _ = std::fs::remove_file(&self.0);
        }
    }

    #[test]
    fn a_picture_carried_inside_the_svg_is_drawn() {
        assert_eq!(drawn(&picture_of(&data_url("image/png", &red_png()))), 0xffff0000);
    }

    #[test]
    fn a_picture_on_disk_is_not_read() {
        let file = PngOnDisk::new("direct");
        assert_eq!(drawn(&picture_of(file.0.to_str().unwrap())), 0);
    }

    #[test]
    fn an_svg_inside_the_svg_cannot_read_the_disk_either() {
        let file = PngOnDisk::new("nested");
        let inner = picture_of(file.0.to_str().unwrap());
        assert_eq!(drawn(&picture_of(&data_url("image/svg+xml", &inner))), 0);
    }

    #[test]
    fn broken_files_are_errors() {
        assert!(Svgs::default().add(b"<svg", None).is_err());
    }
}
