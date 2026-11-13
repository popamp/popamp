//! A vector skin's colour sets: `[colors.<set>]` tables of named colours in
//! `skin.toml`. One set is in use at a time. Its colours reach the SVG
//! pictures as a style sheet (an element with `class="accent"` is filled
//! with the colour named `accent`) and the file's own colour keys by name.

use std::collections::HashMap;
use std::ops::Range;
use toml::Spanned;

/// `[colors]` as written: set name -> colour name -> `#rrggbb`.
pub type SetsFile = HashMap<String, Spanned<HashMap<String, Spanned<String>>>>;

/// A mistake and where in the file it is.
pub type Fault = (Range<usize>, String);

/// Classes that give a named colour to a line and to a gradient stop.
const FORMS: [(&str, &str); 3] = [("", "fill"), ("-stroke", "stroke"), ("-stop", "stop-color")];

#[derive(Debug, Default)]
pub struct Palette {
    /// The sets' names in the order the file has them.
    pub sets: Vec<String>,
    /// Which of `sets` is in use.
    pub chosen: usize,
    colors: HashMap<String, u32>,
}

impl Palette {
    /// Reads the sets and picks `wanted`, or the first when the skin has
    /// no set of that name. The first set names every colour; the others
    /// change the ones they list.
    pub fn read(file: &SetsFile, wanted: Option<&str>) -> Result<Palette, Fault> {
        let mut tables: Vec<_> = file.iter().collect();
        tables.sort_by_key(|(_, t)| t.span().start);
        let sets: Vec<String> = tables.iter().map(|(name, _)| name.to_string()).collect();
        let chosen = wanted.and_then(|w| sets.iter().position(|s| s == w)).unwrap_or(0);
        let mut colors = HashMap::new();
        for (i, (set, table)) in tables.iter().enumerate() {
            for (name, value) in table.get_ref() {
                let fail = |msg: String| (value.span(), msg);
                check_name(name).map_err(fail)?;
                let color = hex(value.get_ref()).ok_or_else(|| fail(not_a_colour(value.get_ref())))?;
                if i == 0 || i == chosen {
                    colors.insert(name.clone(), color);
                }
                if !tables[0].1.get_ref().contains_key(name) {
                    return Err(fail(format!("set `{set}` has a colour `{name}` that the first set, `{}`, does not name", sets[0])));
                }
            }
        }
        Ok(Palette { sets, chosen, colors })
    }

    /// A colour written `#rrggbb` or by its name in the sets.
    pub fn color(&self, word: &str) -> Result<u32, String> {
        match hex(word).or_else(|| self.colors.get(word).copied()) {
            Some(c) => Ok(c),
            None if self.colors.is_empty() => Err(not_a_colour(word)),
            None => Err(format!("{} or use one of the skin's colours: {}", not_a_colour(word), self.names().join(", "))),
        }
    }

    /// The style sheet that gives the pictures the set's colours; `None`
    /// for a skin without sets.
    pub fn css(&self) -> Option<String> {
        let rule = |name: &String| {
            let color = self.colors[name];
            // Drawing programs write colours as inline styles, which only
            // an important rule overrides.
            FORMS.map(|(suffix, property)| format!(".{name}{suffix}{{{property}:#{color:06x} !important}}")).concat()
        };
        (!self.colors.is_empty()).then(|| self.names().iter().map(rule).collect())
    }

    fn names(&self) -> Vec<String> {
        let mut names: Vec<String> = self.colors.keys().cloned().collect();
        names.sort();
        names
    }
}

fn not_a_colour(word: &str) -> String {
    format!("`{word}` is not a colour; write it #rrggbb")
}

fn hex(s: &str) -> Option<u32> {
    let digits = s.strip_prefix('#').filter(|d| d.len() == 6 && d.bytes().all(|b| b.is_ascii_hexdigit()))?;
    u32::from_str_radix(digits, 16).ok()
}

/// A colour's name is a class in the pictures, so it is a plain word.
fn check_name(name: &str) -> Result<(), String> {
    let word = name.starts_with(|c: char| c.is_ascii_alphabetic()) && name.chars().all(|c| c.is_ascii_alphanumeric() || "-_".contains(c));
    let form = FORMS.iter().any(|(suffix, _)| !suffix.is_empty() && name.ends_with(suffix));
    match (word, form) {
        (false, _) => Err(format!("`{name}` cannot name a colour: use letters, digits, `-` and `_`, starting with a letter")),
        (true, true) => {
            Err(format!("`{name}` cannot name a colour: `-stroke` and `-stop` at the end mark another colour's line and gradient forms"))
        }
        (true, false) => Ok(()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::Deserialize;

    #[derive(Deserialize)]
    struct File {
        colors: SetsFile,
    }

    const SETS: &str = "[colors.pink]\nback = \"#ffc0e0\"\naccent = \"#ff0080\"\n[colors.\"Dark blue\"]\nback = \"#000020\"\n";

    fn read(text: &str, wanted: Option<&str>) -> Result<Palette, String> {
        let file: File = toml::from_str(text).unwrap();
        Palette::read(&file.colors, wanted).map_err(|(span, msg)| format!("{}: {msg}", &text[span]))
    }

    #[test]
    fn the_first_set_is_used_unless_another_is_wanted() {
        for wanted in [None, Some("pink"), Some("gone")] {
            let p = read(SETS, wanted).unwrap();
            assert_eq!(p.sets, ["pink", "Dark blue"], "in the file's order");
            assert_eq!((p.chosen, p.color("back")), (0, Ok(0xffc0e0)));
        }
    }

    #[test]
    fn another_set_changes_only_the_colours_it_lists() {
        let p = read(SETS, Some("Dark blue")).unwrap();
        assert_eq!(p.chosen, 1);
        assert_eq!((p.color("back"), p.color("accent")), (Ok(0x000020), Ok(0xff0080)));
        assert_eq!(p.color("#010203"), Ok(0x010203), "a colour can still be written out");
    }

    #[test]
    fn the_style_sheet_colours_fills_lines_and_gradients() {
        let css = read("[colors.a]\nink = \"#0000ff\"\n", None).unwrap().css().unwrap();
        assert_eq!(css, ".ink{fill:#0000ff !important}.ink-stroke{stroke:#0000ff !important}.ink-stop{stop-color:#0000ff !important}");
        assert_eq!(Palette::default().css(), None, "no sets, no style sheet");
    }

    #[test]
    fn mistakes_are_told_apart() {
        let e = |text: &str| read(text, None).err().unwrap();
        assert_eq!(e("[colors.a]\nink = \"blue\"\n"), "\"blue\": `blue` is not a colour; write it #rrggbb");
        assert!(e("[colors.a]\n\"my ink\" = \"#000000\"\n").contains("`my ink` cannot name a colour: use letters"));
        assert!(e("[colors.a]\nink-stop = \"#000000\"\n").contains("mark another colour's"));
        let e = e("[colors.a]\nink = \"#000000\"\n[colors.b]\nimk = \"#ffffff\"\n");
        assert_eq!(e, "\"#ffffff\": set `b` has a colour `imk` that the first set, `a`, does not name");
        assert_eq!(Palette::default().color("green").unwrap_err(), "`green` is not a colour; write it #rrggbb");
        let p = read(SETS, None).unwrap();
        assert_eq!(p.color("bck").unwrap_err(), "`bck` is not a colour; write it #rrggbb or use one of the skin's colours: accent, back");
    }
}
