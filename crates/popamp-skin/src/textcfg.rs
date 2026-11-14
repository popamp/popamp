//! Text files inside classic skins: `viscolor.txt`, `pledit.txt`,
//! `region.txt`, and the `text.bmp` glyph map.

use crate::bitmap::rgb;
use popamp_core::ini::Ini;

/// The 24 visualizer colours.
pub fn parse_viscolor(text: &str, defaults: &[u32; 24]) -> [u32; 24] {
    let mut colors = *defaults;
    let mut idx = 0;
    for line in text.lines() {
        if idx >= 24 {
            break;
        }
        let data = line.split("//").next().unwrap_or("");
        let nums: Vec<u8> = data
            .split(|c: char| !c.is_ascii_digit())
            .filter(|s| !s.is_empty())
            .take(3)
            .filter_map(|s| s.parse::<u32>().ok().map(|v| v.min(255) as u8))
            .collect();
        if nums.len() == 3 {
            colors[idx] = rgb(nums[0], nums[1], nums[2]);
            idx += 1;
        }
    }
    colors
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlColors {
    pub normal: u32,
    pub current: u32,
    pub normal_bg: u32,
    pub selected_bg: u32,
    pub font: String,
}

impl Default for PlColors {
    fn default() -> Self {
        Self {
            normal: rgb(0x00, 0xff, 0x00),
            current: rgb(0xff, 0xff, 0xff),
            normal_bg: 0,
            selected_bg: rgb(0, 0, 0xff),
            font: "Arial".into(),
        }
    }
}

/// How a vector skin writes a text: in a TrueType font from the skin,
/// or the system's sans-serif font when `font` is `None`.
#[derive(Debug, Clone, PartialEq)]
pub struct TextStyle {
    pub font: Option<Vec<u8>>,
    /// Height in skin pixels.
    pub size: i32,
    pub color: u32,
}

pub fn parse_color(s: &str) -> Option<u32> {
    let hex = s.trim().trim_start_matches('#');
    let hex: String = hex.chars().take_while(|c| c.is_ascii_hexdigit()).collect();
    match hex.len() {
        6 => u32::from_str_radix(&hex, 16).ok(),
        3 => {
            let v = u32::from_str_radix(&hex, 16).ok()?;
            let (r, g, b) = ((v >> 8) & 0xf, (v >> 4) & 0xf, v & 0xf);
            Some(rgb((r * 17) as u8, (g * 17) as u8, (b * 17) as u8))
        }
        _ => None,
    }
}

pub fn parse_pledit(text: &str, defaults: &PlColors) -> PlColors {
    let ini = Ini::parse(text);
    let color = |key: &str, d: u32| ini.get("Text", key).and_then(parse_color).unwrap_or(d);
    PlColors {
        normal: color("Normal", defaults.normal),
        current: color("Current", defaults.current),
        normal_bg: color("NormalBG", defaults.normal_bg),
        selected_bg: color("SelectedBG", defaults.selected_bg),
        font: ini.get("Text", "Font").map(str::trim).filter(|f| !f.is_empty()).unwrap_or(&defaults.font).to_string(),
    }
}

/// Window shapes from `region.txt`: a list of polygons per section.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Regions {
    pub normal: Option<Vec<Vec<(i32, i32)>>>,
    pub equalizer: Option<Vec<Vec<(i32, i32)>>>,
    pub window_shade: Option<Vec<Vec<(i32, i32)>>>,
    pub equalizer_ws: Option<Vec<Vec<(i32, i32)>>>,
}

fn parse_section(ini: &Ini, name: &str) -> Option<Vec<Vec<(i32, i32)>>> {
    let ints = |s: &str| -> Vec<i32> { s.split(|c: char| c == ',' || c.is_whitespace()).filter_map(|p| p.trim().parse().ok()).collect() };
    let counts = ints(ini.get(name, "NumPoints")?);
    let coords = ints(ini.get(name, "PointList")?);
    let mut polys = Vec::new();
    let mut it = coords.as_chunks::<2>().0.iter().map(|&[x, y]| (x, y));
    for n in counts {
        let poly: Vec<(i32, i32)> = it.by_ref().take(n.max(0) as usize).collect();
        if poly.len() >= 3 {
            polys.push(poly);
        }
    }
    (!polys.is_empty()).then_some(polys)
}

pub fn parse_region(text: &str) -> Regions {
    let ini = Ini::parse(text);
    Regions {
        normal: parse_section(&ini, "Normal"),
        equalizer: parse_section(&ini, "Equalizer"),
        window_shade: parse_section(&ini, "WindowShade"),
        equalizer_ws: parse_section(&ini, "EqualizerWS"),
    }
}

/// Rasterizes polygons (even-odd per polygon, union across polygons) into a
/// `w*h` mask. Winamp treats points as pixel corners.
pub fn region_mask(polys: &[Vec<(i32, i32)>], w: i32, h: i32) -> Vec<bool> {
    let mut mask = vec![false; (w.max(0) * h.max(0)) as usize];
    for poly in polys {
        for y in 0..h {
            let sy = y as f32 + 0.5;
            let mut xs = Vec::new();
            for i in 0..poly.len() {
                let (a, b) = (poly[i], poly[(i + 1) % poly.len()]);
                let (ay, by) = (a.1 as f32, b.1 as f32);
                if (ay <= sy && by > sy) || (by <= sy && ay > sy) {
                    xs.push(a.0 as f32 + (sy - ay) / (by - ay) * (b.0 - a.0) as f32);
                }
            }
            xs.sort_by(f32::total_cmp);
            for [a, b] in xs.as_chunks::<2>().0 {
                let x0 = (a - 0.5).ceil().max(0.0) as i32;
                let x1 = ((b - 0.5).ceil() as i32).min(w);
                for x in x0..x1 {
                    mask[(y * w + x) as usize] = true;
                }
            }
        }
    }
    mask
}

pub const GLYPH_W: i32 = 5;
pub const GLYPH_H: i32 = 6;

/// Cell (column, row) of `c` in `text.bmp`; unknown characters map to space.
pub fn glyph_cell(c: char) -> (i32, i32) {
    glyph(c).unwrap_or(SPACE)
}

const SPACE: (i32, i32) = (30, 0);

/// Cell (column, row) of `c` in `text.bmp`, if the font has it.
pub fn glyph(c: char) -> Option<(i32, i32)> {
    let c = fold_char(c);
    if c.is_ascii_lowercase() {
        return Some((c as i32 - 'a' as i32, 0));
    }
    if c.is_ascii_digit() {
        return Some((c as i32 - '0' as i32, 1));
    }
    Some(match c {
        ' ' => SPACE,
        '"' => (26, 0),
        '@' => (27, 0),
        '…' => (10, 1),
        '.' => (11, 1),
        ':' => (12, 1),
        '(' | '<' | '{' => (13, 1),
        ')' | '>' | '}' => (14, 1),
        '-' | '~' => (15, 1),
        '\'' | '`' => (16, 1),
        '!' => (17, 1),
        '_' => (18, 1),
        '+' => (19, 1),
        '\\' => (20, 1),
        '/' => (21, 1),
        '[' => (22, 1),
        ']' => (23, 1),
        '^' => (24, 1),
        '&' => (25, 1),
        '%' => (26, 1),
        ',' | ';' => (27, 1),
        '=' => (28, 1),
        '$' => (29, 1),
        '#' => (30, 1),
        'å' => (0, 2),
        'ö' => (1, 2),
        'ä' => (2, 2),
        '?' => (3, 2),
        '*' => (4, 2),
        _ => return None,
    })
}

/// The colour of the letters in `text.bmp`: the commonest one after the
/// background's.
pub fn text_ink(font: &crate::Bitmap) -> u32 {
    let mut counts = std::collections::HashMap::new();
    for &p in &font.px {
        *counts.entry(p).or_insert(0usize) += 1;
    }
    let mut by_use: Vec<(usize, u32)> = counts.into_iter().map(|(p, n)| (n, p)).collect();
    by_use.sort_unstable_by(|a, b| b.cmp(a));
    by_use.get(1).or(by_use.first()).map_or(0, |&(_, p)| p)
}

/// Lower-cases and strips accents the skin font can't show.
fn fold_char(c: char) -> char {
    let lower = c.to_lowercase().next().unwrap_or(c);
    match lower {
        'å' | 'ö' | 'ä' => lower,
        'à' | 'á' | 'â' | 'ã' | 'ā' => 'a',
        'ç' | 'č' => 'c',
        'è' | 'é' | 'ê' | 'ë' | 'ē' => 'e',
        'ì' | 'í' | 'î' | 'ï' => 'i',
        'ñ' | 'ń' => 'n',
        'ò' | 'ó' | 'ô' | 'õ' | 'ø' => 'o',
        'ù' | 'ú' | 'û' | 'ü' => 'u',
        'ý' | 'ÿ' => 'y',
        'š' | 'ś' => 's',
        'ž' | 'ź' | 'ż' => 'z',
        'ß' => 's',
        '“' | '”' => '"',
        '‘' | '’' => '\'',
        '–' | '—' => '-',
        other => other,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_skin_font_has_latin_letters_but_not_cyrillic() {
        assert!("Hello, World! 1999 (4:46) café".chars().all(|c| glyph(c).is_some()));
        assert_eq!(glyph('Б'), None);
        assert_eq!(glyph_cell('Б'), glyph_cell(' '), "drawn as a space");
    }

    #[test]
    fn the_letters_are_the_commonest_colour_after_the_background() {
        let mut font = crate::Bitmap::new(10, 2, 0x000011);
        font.px[..6].fill(0x00ff00);
        font.px[6] = 0xff0000;
        assert_eq!(text_ink(&font), 0x00ff00);
        assert_eq!(text_ink(&crate::Bitmap::new(2, 2, 0x123456)), 0x123456, "one colour is all there is");
    }

    #[test]
    fn viscolor() {
        let d = [0u32; 24];
        let c = parse_viscolor("0,0,0, // bg\n24,33,41 // dots\n 239, 49, 16\nnonsense\n", &d);
        assert_eq!(c[0], 0);
        assert_eq!(c[1], rgb(24, 33, 41));
        assert_eq!(c[2], rgb(239, 49, 16));
        assert_eq!(c[3], 0);
    }

    #[test]
    fn pledit() {
        let c =
            parse_pledit("[Text]\nNormal=#00FF00\nCurrent=#fff\nNormalBG=#000000\nSelectedBG=0000C6\nFont=Tahoma\n", &PlColors::default());
        assert_eq!(c.current, 0xffffff);
        assert_eq!(c.selected_bg, 0x0000c6);
        assert_eq!(c.font, "Tahoma");
        let d = parse_pledit("garbage", &PlColors::default());
        assert_eq!(d, PlColors::default());
    }

    #[test]
    fn region() {
        let r = parse_region("[Normal]\nNumPoints=4, 4\nPointList=0,0, 275,0, 275,116, 0,116,  0,0 2,0 2,2 0,2\n[WindowShade]\nNumPoints=2\nPointList=0,0,1,1\n");
        let n = r.normal.unwrap();
        assert_eq!(n.len(), 2);
        assert_eq!(n[0][2], (275, 116));
        assert!(r.window_shade.is_none());
        let mask = region_mask(&[vec![(1, 1), (3, 1), (3, 3), (1, 3)]], 4, 4);
        let on: Vec<usize> = mask.iter().enumerate().filter(|(_, b)| **b).map(|(i, _)| i).collect();
        assert_eq!(on, [5, 6, 9, 10]);
    }

    #[test]
    fn glyphs() {
        assert_eq!(glyph_cell('A'), (0, 0));
        assert_eq!(glyph_cell('z'), (25, 0));
        assert_eq!(glyph_cell('7'), (7, 1));
        assert_eq!(glyph_cell('é'), glyph_cell('e'));
        assert_eq!(glyph_cell('Ö'), (1, 2));
        assert_eq!(glyph_cell('€'), (30, 0));
    }
}
