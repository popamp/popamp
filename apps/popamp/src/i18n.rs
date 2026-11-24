//! The words of menus and windows in the user's language.
//!
//! The English text in the code is the key: `tr!("Preferences")` is its
//! translation from `locales/<code>.toml`, or the English itself where the
//! file has none. A text with values names them in braces, filled in after
//! the lookup: `tr!("Scan every: 1 to {max} minutes", max = MAX)`. A count
//! picks one of the language's plural forms:
//! `trn!(n, "{n} file", "{n} files")`, keyed by the English plural.
//!
//! The files are built into the program. The test at the end reads the
//! sources and checks each file against every text they translate.

use std::collections::HashMap;
use std::fmt::Display;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::OnceLock;

/// The languages the program speaks; English is the one in the code.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Lang {
    En,
    Ru,
}

impl Lang {
    pub const ALL: [Lang; 2] = [Lang::En, Lang::Ru];

    /// Its code, as locale names start with it and the settings keep it.
    pub fn code(self) -> &'static str {
        match self {
            Lang::En => "en",
            Lang::Ru => "ru",
        }
    }

    /// Its own name for itself, as a list of languages shows it.
    pub fn name(self) -> &'static str {
        match self {
            Lang::En => "English",
            Lang::Ru => "Русский",
        }
    }

    fn file(self) -> &'static str {
        match self {
            Lang::En => "",
            Lang::Ru => include_str!("../locales/ru.toml"),
        }
    }

    /// How many plural forms its counted texts have.
    #[cfg(test)]
    fn forms(self) -> usize {
        match self {
            Lang::En => 2,
            Lang::Ru => 3,
        }
    }

    /// Which plural form goes with `n`.
    fn form(self, n: u64) -> usize {
        match self {
            Lang::En => usize::from(n != 1),
            // 1, 21, 101 файл; 2-4, 22-24 файла; 5-20, 25-30, 11-14 файлов.
            Lang::Ru => match (n % 10, n % 100) {
                (1, h) if h != 11 => 0,
                (2..=4, h) if !(12..=14).contains(&h) => 1,
                _ => 2,
            },
        }
    }

    fn index(self) -> usize {
        Lang::ALL.iter().position(|&l| l == self).unwrap_or(0)
    }

    /// The language of a locale name: `ru_RU.UTF-8`, `ru-RU`, `ru`.
    pub fn of_locale(name: &str) -> Option<Lang> {
        let code = name.trim().split(['_', '-', '.', '@']).next()?.to_ascii_lowercase();
        Lang::ALL.into_iter().find(|l| l.code() == code)
    }
}

/// A translation: one text, or one per plural form.
#[derive(Debug)]
enum Text {
    One(&'static str),
    Forms(Vec<&'static str>),
}

type Table = HashMap<&'static str, Text>;

/// A translations file: `"English" = "перевод"`, or a list of the plural
/// forms. Each file is read once and kept while the program runs.
fn parse(file: &str) -> Result<Table, String> {
    let leak = |s: String| -> &'static str { Box::leak(s.into_boxed_str()) };
    let doc: toml::Table = file.parse().map_err(|e: toml::de::Error| e.to_string())?;
    let mut table = Table::new();
    for (key, value) in doc {
        let text = match value {
            toml::Value::String(s) => Text::One(leak(s)),
            toml::Value::Array(forms) => Text::Forms(
                forms
                    .into_iter()
                    .map(|f| f.as_str().map(|s| leak(s.to_string())).ok_or(format!("{key}: a form is not a text")))
                    .collect::<Result<_, _>>()?,
            ),
            _ => return Err(format!("{key}: neither a text nor a list of texts")),
        };
        table.insert(leak(key), text);
    }
    Ok(table)
}

fn table(lang: Lang) -> &'static Table {
    static TABLES: [OnceLock<Table>; Lang::ALL.len()] = [const { OnceLock::new() }; Lang::ALL.len()];
    TABLES[lang.index()].get_or_init(|| {
        parse(lang.file()).unwrap_or_else(|e| {
            log::warn!("translations for {}: {e}", lang.code());
            Table::new()
        })
    })
}

static CURRENT: AtomicUsize = AtomicUsize::new(0);

pub fn current() -> Lang {
    Lang::ALL[CURRENT.load(Ordering::Relaxed)]
}

pub fn set(lang: Lang) {
    CURRENT.store(lang.index(), Ordering::Relaxed);
}

/// Puts on the language the settings name by its code, or the system's
/// when they name none Popamp speaks; English when neither does.
pub fn apply_setting(code: &str) {
    set(Lang::of_locale(code).unwrap_or_else(system_language));
}

/// The system's language, or English when Popamp does not speak it.
pub fn system_language() -> Lang {
    system().unwrap_or(Lang::En)
}

/// The translation of `en` in the current language.
pub fn tr(en: &'static str) -> &'static str {
    match table(current()).get(en) {
        Some(Text::One(t)) => t,
        Some(Text::Forms(f)) => f.first().copied().unwrap_or(en),
        None => en,
    }
}

/// The translation of `key` in the current language, none where it has
/// none: for a key that is not the English to show.
pub fn translated(key: &str) -> Option<&'static str> {
    match table(current()).get(key) {
        Some(Text::One(t)) => Some(t),
        _ => None,
    }
}

/// The form of a counted text that goes with `n`: `one` or `many` in
/// English, the translation of `many` in the current language.
pub fn tr_n(n: u64, one: &'static str, many: &'static str) -> &'static str {
    let lang = current();
    let english = if n == 1 { one } else { many };
    match table(lang).get(many) {
        Some(Text::Forms(f)) => f.get(lang.form(n)).copied().unwrap_or(english),
        Some(Text::One(t)) => t,
        None => english,
    }
}

/// `text` with each `{name}` of `values` written in; other braces stay.
pub fn fill(text: &str, values: &[(&str, &dyn Display)]) -> String {
    let mut out = String::with_capacity(text.len() + 16);
    let mut rest = text;
    while let Some(open) = rest.find('{') {
        out.push_str(&rest[..open]);
        let after = &rest[open + 1..];
        let value = after.find('}').and_then(|close| values.iter().find(|(k, _)| *k == &after[..close]).map(|(_, v)| (close, v)));
        match value {
            Some((close, v)) => {
                out.push_str(&v.to_string());
                rest = &after[close + 1..];
            }
            None => {
                out.push('{');
                rest = after;
            }
        }
    }
    out.push_str(rest);
    out
}

/// `tr!("Text")` is a `&'static str`; `tr!("Text {a} {b}", a = x, b)` a
/// `String` with the values written in (`b` alone stands for `b = b`).
macro_rules! tr {
    (@value $k:ident) => { $k };
    (@value $k:ident = $v:expr) => { $v };
    ($en:literal) => { $crate::i18n::tr($en) };
    ($en:literal, $($k:ident $(= $v:expr)?),+ $(,)?) => {
        $crate::i18n::fill($crate::i18n::tr($en), &[$((stringify!($k), &tr!(@value $k $(= $v)?) as &dyn std::fmt::Display)),+])
    };
}

/// `tr_const!("Artist")` is the text itself, for a constant: it marks the
/// text to translate where it is shown, with [`tr`].
macro_rules! tr_const {
    ($en:literal) => {
        $en
    };
}

/// `trn!(n, "{n} file", "{n} files")`: the form that goes with the count
/// `n`, with `{n}` and any further values written in, as in `tr!`.
macro_rules! trn {
    ($n:expr, $one:literal, $many:literal $(, $k:ident $(= $v:expr)?)* $(,)?) => {{
        let n = $n;
        $crate::i18n::fill(
            $crate::i18n::tr_n(n as u64, $one, $many),
            &[("n", &n as &dyn std::fmt::Display) $(, (stringify!($k), &tr!(@value $k $(= $v)?) as &dyn std::fmt::Display))*],
        )
    }};
}

/// The system's language, from the locale variables: `LANGUAGE`, a list
/// in the order the user wants them, counts only beside a locale that is
/// not plain `C`, as in gettext.
#[cfg(not(any(windows, target_os = "macos")))]
fn system() -> Option<Lang> {
    from_env(|k| std::env::var(k).ok())
}

#[cfg_attr(any(windows, target_os = "macos"), allow(dead_code))]
fn from_env(var: impl Fn(&str) -> Option<String>) -> Option<Lang> {
    let locale = ["LC_ALL", "LC_MESSAGES", "LANG"].into_iter().find_map(|k| var(k).filter(|v| !v.is_empty()))?;
    if locale == "C" || locale == "POSIX" || locale.starts_with("C.") {
        return None;
    }
    let list = var("LANGUAGE").unwrap_or_default();
    let lang = list.split(':').chain([locale.as_str()]).find_map(Lang::of_locale);
    lang
}

/// The language of the Windows display.
#[cfg(windows)]
fn system() -> Option<Lang> {
    // The primary language of a LANGID is its low ten bits.
    match unsafe { windows_sys::Win32::Globalization::GetUserDefaultUILanguage() } & 0x3ff {
        0x09 => Some(Lang::En),
        0x19 => Some(Lang::Ru),
        _ => None,
    }
}

/// The first language of the user's list in System Settings that Popamp
/// speaks. A program opened from Finder has no locale variables.
#[cfg(target_os = "macos")]
fn system() -> Option<Lang> {
    let out = std::process::Command::new("defaults").args(["read", "-g", "AppleLanguages"]).output().ok()?;
    apple_languages(&String::from_utf8_lossy(&out.stdout)).or_else(|| from_env(|k| std::env::var(k).ok()))
}

/// The first known language in `defaults`' printout of a list:
/// `(\n    "ru-RU",\n    "en-US"\n)`.
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
fn apple_languages(list: &str) -> Option<Lang> {
    list.split(|c: char| !(c.is_ascii_alphanumeric() || c == '-' || c == '_')).filter(|w| !w.is_empty()).find_map(Lang::of_locale)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeSet;
    use std::path::Path;

    #[test]
    fn values_are_written_into_their_braces() {
        let (n, name) = (3, "x.mp3");
        assert_eq!(fill("{n} of {name}, {other}", &[("n", &n), ("name", &name)]), "3 of x.mp3, {other}");
        assert_eq!(fill("{ unclosed", &[("n", &n)]), "{ unclosed");
    }

    #[test]
    fn russian_counts_take_their_forms() {
        let forms = [1, 2, 4, 5, 11, 12, 14, 21, 22, 25, 101, 111].map(|n| Lang::Ru.form(n));
        assert_eq!(forms, [0, 1, 1, 2, 2, 2, 2, 0, 1, 2, 0, 2]);
        assert_eq!([0, 1, 2].map(|n| Lang::En.form(n)), [1, 0, 1]);
    }

    #[test]
    fn locale_names_give_their_language() {
        assert_eq!(Lang::of_locale("ru_RU.UTF-8"), Some(Lang::Ru));
        assert_eq!(Lang::of_locale("en-GB"), Some(Lang::En));
        assert_eq!(Lang::of_locale("de_DE"), None);
        assert_eq!(Lang::of_locale(""), None);
        assert_eq!(apple_languages("(\n    \"de-DE\",\n    \"ru-RU\"\n)"), Some(Lang::Ru));
    }

    #[test]
    fn locale_variables_pick_the_language() {
        let env =
            |vars: &'static [(&'static str, &'static str)]| move |k: &str| vars.iter().find(|(n, _)| *n == k).map(|(_, v)| v.to_string());
        assert_eq!(from_env(env(&[("LANG", "ru_RU.UTF-8")])), Some(Lang::Ru));
        assert_eq!(from_env(env(&[("LANG", "en_US.UTF-8"), ("LC_MESSAGES", "ru_RU.UTF-8")])), Some(Lang::Ru));
        assert_eq!(from_env(env(&[("LANG", "de_DE.UTF-8"), ("LANGUAGE", "de:ru")])), Some(Lang::Ru));
        assert_eq!(from_env(env(&[("LANG", "C.UTF-8"), ("LANGUAGE", "ru")])), None);
        assert_eq!(from_env(env(&[])), None);
    }

    /// A text of the sources to translate: its English key, and whether it
    /// is counted (`trn!`).
    #[derive(Debug, PartialEq, Eq, PartialOrd, Ord)]
    struct Key {
        text: String,
        counted: bool,
        /// The names of the values given after the texts.
        values: BTreeSet<String>,
        /// Where it is, for the messages.
        at: String,
    }

    /// The names of the values in a macro's arguments after its texts, up
    /// to its closing parenthesis: `a = x, b` names `a` and `b`.
    fn values(rest: &str) -> BTreeSet<String> {
        let (mut depth, mut part, mut names) = (0, String::new(), BTreeSet::new());
        let mut take = |part: &mut String| {
            let name = part.split('=').next().unwrap_or("").trim().to_string();
            if !name.is_empty() && name.chars().all(|c| c.is_alphanumeric() || c == '_') {
                names.insert(name);
            }
            part.clear();
        };
        for c in rest.chars() {
            match c {
                '(' | '[' | '{' => depth += 1,
                ')' | ']' | '}' if depth == 0 => break,
                ')' | ']' | '}' => depth -= 1,
                ',' if depth == 0 => {
                    take(&mut part);
                    continue;
                }
                _ => {}
            }
            part.push(c);
        }
        take(&mut part);
        names
    }

    /// Reads a string literal at the start of `s`: its text and the rest.
    fn literal(s: &str) -> Option<(String, &str)> {
        let body = s.trim_start().strip_prefix('"')?;
        let mut chars = body.char_indices().peekable();
        let mut out = String::new();
        while let Some((i, c)) = chars.next() {
            match c {
                '"' => return Some((out, &body[i + 1..])),
                '\\' => match chars.next()?.1 {
                    'n' => out.push('\n'),
                    't' => out.push('\t'),
                    '0' => out.push('\0'),
                    // A line continued: the next line's indent is left out.
                    '\n' => while chars.next_if(|(_, c)| c.is_whitespace()).is_some() {},
                    'u' => {
                        let hex: String = chars.by_ref().map(|(_, c)| c).skip(1).take_while(|&c| c != '}').collect();
                        out.push(char::from_u32(u32::from_str_radix(&hex, 16).ok()?)?);
                    }
                    c => out.push(c),
                },
                c => out.push(c),
            }
        }
        None
    }

    /// Every `tr!` and `trn!` text in the `.rs` files under `dir`.
    fn keys(dir: &Path, out: &mut Vec<Key>) {
        for entry in std::fs::read_dir(dir).unwrap().flatten() {
            let path = entry.path();
            if path.is_dir() {
                keys(&path, out);
            } else if path.extension().is_some_and(|e| e == "rs") && !path.ends_with("i18n.rs") {
                let src = std::fs::read_to_string(&path).unwrap();
                let name = path.file_name().unwrap().to_string_lossy().into_owned();
                let at = |pos: usize| format!("{name}:{}", src[..pos].lines().count());
                // A macro name of its own: not the end of a longer one.
                let starts = |pat: &'static str| {
                    src.match_indices(pat)
                        .map(|(pos, _)| pos)
                        .filter(|&pos| !src[..pos].ends_with(|c: char| c.is_alphanumeric() || c == '_'))
                };
                for (pat, pos) in starts("tr!(").map(|p| ("tr!(", p)).chain(starts("tr_const!(").map(|p| ("tr_const!(", p))) {
                    let (text, rest) =
                        literal(&src[pos + pat.len()..]).unwrap_or_else(|| panic!("{}: {pat} takes a string literal", at(pos)));
                    out.push(Key { text, counted: false, values: values(rest), at: at(pos) });
                }
                for pos in starts("trn!(") {
                    // trn!(count, "one", "many"): the count runs to the first text.
                    let args = &src[pos + 5..];
                    let one = args.find('"').unwrap_or_else(|| panic!("{}: trn! needs its texts", at(pos)));
                    let (_, rest) = literal(&args[one..]).unwrap_or_else(|| panic!("{}: trn! one form", at(pos)));
                    let rest = rest.trim_start().strip_prefix(',').unwrap_or_else(|| panic!("{}: trn! needs two forms", at(pos)));
                    let (many, rest) = literal(rest).unwrap_or_else(|| panic!("{}: trn! many form", at(pos)));
                    let mut given = values(rest);
                    given.insert("n".into());
                    out.push(Key { text: many, counted: true, values: given, at: at(pos) });
                }
            }
        }
    }

    fn placeholders(text: &str) -> BTreeSet<&str> {
        text.split('{')
            .skip(1)
            .filter_map(|p| p.split_once('}').map(|(name, _)| name))
            .filter(|n| n.chars().all(|c| c.is_alphanumeric() || c == '_'))
            .collect()
    }

    /// The TOML line that a missing text would take, to copy into the file.
    fn line(key: &Key) -> String {
        let q = toml::Value::String(key.text.clone()).to_string();
        match key.counted {
            true => format!("{q} = [{q}, {q}, {q}]  # {}", key.at),
            false => format!("{q} = {q}  # {}", key.at),
        }
    }

    #[test]
    fn every_language_translates_every_text() {
        let mut found = Vec::new();
        keys(&Path::new(env!("CARGO_MANIFEST_DIR")).join("src"), &mut found);
        assert!(found.len() > 10, "the scan of the sources found almost nothing");
        let mut problems = Vec::new();
        for (a, b) in found.iter().zip(found.iter().skip(1)).filter(|(a, b)| a.text == b.text && a.counted != b.counted) {
            problems.push(format!("{:?} is counted at {} but not at {}", a.text, a.at, b.at));
        }
        for key in &found {
            let wanted: BTreeSet<String> = placeholders(&key.text).into_iter().map(String::from).collect();
            if wanted != key.values {
                problems.push(format!("{}: {:?} names {wanted:?} but is given {:?}", key.at, key.text, key.values));
            }
        }
        for lang in Lang::ALL.into_iter().filter(|&l| l != Lang::En) {
            let table = parse(lang.file()).unwrap_or_else(|e| panic!("locales/{}.toml: {e}", lang.code()));
            let mut missing = Vec::new();
            for key in &found {
                let wanted = placeholders(&key.text);
                let fault = match (table.get(key.text.as_str()), key.counted) {
                    (None, _) => {
                        missing.push(line(key));
                        None
                    }
                    (Some(Text::One(t)), false) => (placeholders(t) != wanted).then(|| format!("{t:?} has other braces")),
                    (Some(Text::Forms(_)), false) => Some("is not counted: give one text".to_string()),
                    (Some(Text::One(_)), true) => Some("is counted: give its forms".to_string()),
                    (Some(Text::Forms(f)), true) if f.len() != lang.forms() => Some(format!("needs {} forms", lang.forms())),
                    (Some(Text::Forms(f)), true) => {
                        f.iter().find(|t| !placeholders(t).is_subset(&wanted)).map(|t| format!("{t:?} has braces it lacks"))
                    }
                };
                if let Some(fault) = fault {
                    problems.push(format!("{}: {:?} ({}): {fault}", lang.code(), key.text, key.at));
                }
            }
            missing.sort();
            missing.dedup();
            if !missing.is_empty() {
                problems.push(format!("locales/{}.toml lacks:\n{}", lang.code(), missing.join("\n")));
            }
            let used: BTreeSet<&str> = found.iter().map(|k| k.text.as_str()).collect();
            let mut stale: Vec<&&str> = table.keys().filter(|k| !used.contains(**k)).collect();
            stale.sort();
            if !stale.is_empty() {
                problems.push(format!("locales/{}.toml has texts the program no longer has: {stale:?}", lang.code()));
            }
        }
        assert!(problems.is_empty(), "{}", problems.join("\n"));
    }
}
