//! A vector skin's SVG pictures, rasterized for the size they are shown at
//! and kept until the skin changes.

use crate::bitmap::Rgba;
use crate::sprites::Rect;
use resvg::{tiny_skia, usvg};
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

/// Picture number, area of it, and the screen size it is drawn at.
type Key = (u16, Rect, i32, i32);

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
        let key = (id, src, w, h);
        if let Some(r) = self.cache.borrow().get(&key) {
            return Some(r.clone());
        }
        let tree = self.trees.get(id as usize)?;
        let mut pixmap = tiny_skia::Pixmap::new(w.max(1) as u32, h.max(1) as u32)?;
        let (sx, sy) = (w as f32 / src.w.max(1) as f32, h as f32 / src.h.max(1) as f32);
        let at = tiny_skia::Transform::from_row(sx, 0.0, 0.0, sy, -src.x as f32 * sx, -src.y as f32 * sy);
        resvg::render(tree, at, &mut pixmap.as_mut());
        let px = pixmap.pixels().iter().map(|p| u32::from_be_bytes([p.alpha(), p.red(), p.green(), p.blue()])).collect();
        let raster = Rc::new(Rgba { w: pixmap.width() as i32, h: pixmap.height() as i32, px });
        let mut cache = self.cache.borrow_mut();
        // Old scales pile up as the user changes the size; start over then.
        if cache.len() > 2048 {
            cache.clear();
        }
        cache.insert(key, raster.clone());
        Some(raster)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const HALF_RED: &[u8] = br##"<svg xmlns="http://www.w3.org/2000/svg" width="2" height="1">
        <rect x="0" y="0" width="1" height="1" fill="#ff0000"/></svg>"##;

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
