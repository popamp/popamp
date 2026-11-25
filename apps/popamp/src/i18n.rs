//! The words of menus and windows in the user's language.
//!
//! The code names each text by its key: `tr!("library.menu.addFolder")` is
//! its text in the language of `locales/<code>.toml`, or in English from
//! `locales/en.toml` where that file has none. The part before the last dot
//! is the file's section, `[library.menu]`. A text with values names them in
//! braces, filled in after the lookup: `tr!("prefs.field.scanEveryRange",
//! max = MAX)`. A count picks one of the language's plural forms:
//! `trn!(n, "library.nTracks")`.
//!
//! The files are built into the program. The test at the end reads the
//! sources and checks the files against the keys they use.

use std::collections::HashMap;
use std::fmt::Display;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::OnceLock;

/// The languages the program speaks; English is the one in the code.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Lang {
    En,
    Ru,
    Ko,
    /// Simplified, as on the mainland and in Singapore.
    Zh,
}

impl Lang {
    pub const ALL: [Lang; 4] = [Lang::En, Lang::Ru, Lang::Ko, Lang::Zh];

    /// Its code, as locale names start with it and the settings keep it.
    pub fn code(self) -> &'static str {
        match self {
            Lang::En => "en",
            Lang::Ru => "ru",
            Lang::Ko => "ko",
            Lang::Zh => "zh",
        }
    }

    /// Its own name for itself, as a list of languages shows it.
    pub fn name(self) -> &'static str {
        match self {
            Lang::En => "English",
            Lang::Ru => "Русский",
            Lang::Ko => "한국어",
            Lang::Zh => "简体中文",
        }
    }

    fn file(self) -> &'static str {
        match self {
            Lang::En => include_str!("../locales/en.toml"),
            Lang::Ru => include_str!("../locales/ru.toml"),
            Lang::Ko => include_str!("../locales/ko.toml"),
            Lang::Zh => include_str!("../locales/zh.toml"),
        }
    }

    /// How many plural forms its counted texts have.
    #[cfg(test)]
    fn forms(self) -> usize {
        match self {
            Lang::En => 2,
            Lang::Ru => 3,
            Lang::Ko | Lang::Zh => 1,
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
            Lang::Ko | Lang::Zh => 0,
        }
    }

    fn index(self) -> usize {
        Lang::ALL.iter().position(|&l| l == self).unwrap_or(0)
    }

    /// The language of a locale name: `ru_RU.UTF-8`, `ru-RU`, `ru`.
    /// Chinese in traditional characters (`zh_TW`, `zh-Hant-HK`) is none:
    /// Popamp has it only in simplified ones.
    pub fn of_locale(name: &str) -> Option<Lang> {
        let mut parts = name.trim().split(['_', '-', '.', '@']).map(str::to_ascii_lowercase);
        let code = parts.next()?;
        if code == "zh" && parts.any(|p| ["tw", "hk", "mo", "hant"].contains(&p.as_str())) {
            return None;
        }
        Lang::ALL.into_iter().find(|l| l.code() == code)
    }
}

/// A text: one, or one per plural form.
#[derive(Debug)]
enum Text {
    One(&'static str),
    Forms(Vec<&'static str>),
}

/// A language's texts by their full key: `library.menu.addFolder`.
type Table = HashMap<&'static str, Text>;

/// A texts file: sections of `key = "text"`, or a list of the plural
/// forms. Each file is read once and kept while the program runs.
fn parse(file: &str) -> Result<Table, String> {
    let doc: toml::Table = file.parse().map_err(|e: toml::de::Error| e.to_string())?;
    let mut table = Table::new();
    add(&mut table, "", doc)?;
    Ok(table)
}

/// Puts the texts of `section` and of the sections in it into `table`.
fn add(table: &mut Table, section: &str, values: toml::Table) -> Result<(), String> {
    let leak = |s: String| -> &'static str { Box::leak(s.into_boxed_str()) };
    for (name, value) in values {
        let key = if section.is_empty() { name } else { format!("{section}.{name}") };
        let text = match value {
            toml::Value::Table(inner) => {
                add(table, &key, inner)?;
                continue;
            }
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
    Ok(())
}

fn table(lang: Lang) -> &'static Table {
    static TABLES: [OnceLock<Table>; Lang::ALL.len()] = [const { OnceLock::new() }; Lang::ALL.len()];
    TABLES[lang.index()].get_or_init(|| {
        parse(lang.file()).unwrap_or_else(|e| {
            log::warn!("texts for {}: {e}", lang.code());
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

/// The text of `key` in the current language, else in English, else the
/// key itself, so that a missing text shows where it is missing.
pub fn tr(key: &'static str) -> &'static str {
    let one = |lang| match table(lang).get(key) {
        Some(Text::One(t)) => Some(*t),
        Some(Text::Forms(f)) => f.first().copied(),
        None => None,
    };
    one(current()).or_else(|| one(Lang::En)).unwrap_or(key)
}

/// The form of the counted text `key` that goes with `n`, as [`tr`] finds it.
pub fn tr_n(n: u64, key: &'static str) -> &'static str {
    let form = |lang: Lang| match table(lang).get(key) {
        Some(Text::Forms(f)) => f.get(lang.form(n)).copied(),
        Some(Text::One(t)) => Some(*t),
        None => None,
    };
    form(current()).or_else(|| form(Lang::En)).unwrap_or(key)
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

/// `tr!("menu.main.play")` is a `&'static str`; `tr!("dialog.saveFailed",
/// e = err, b)` a `String` with the values written in (`b` alone stands
/// for `b = b`).
macro_rules! tr {
    (@value $k:ident) => { $k };
    (@value $k:ident = $v:expr) => { $v };
    ($key:literal) => { $crate::i18n::tr($key) };
    ($key:literal, $($k:ident $(= $v:expr)?),+ $(,)?) => {
        $crate::i18n::fill($crate::i18n::tr($key), &[$((stringify!($k), &tr!(@value $k $(= $v)?) as &dyn std::fmt::Display)),+])
    };
}

/// `tr_const!("library.column.artist")` is the key itself, for a constant:
/// it marks the key, whose text is shown through [`tr`].
macro_rules! tr_const {
    ($key:literal) => {
        $key
    };
}

/// `trn!(n, "library.nTracks")`: the form that goes with the count `n`,
/// with `{n}` and any further values written in, as in `tr!`.
macro_rules! trn {
    ($n:expr, $key:literal $(, $k:ident $(= $v:expr)?)* $(,)?) => {{
        let n = $n;
        $crate::i18n::fill(
            $crate::i18n::tr_n(n as u64, $key),
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
    // The primary language of a LANGID is its low ten bits; for Chinese
    // the rest says the region: 0x0804 the mainland, 0x1004 Singapore.
    let id = unsafe { windows_sys::Win32::Globalization::GetUserDefaultUILanguage() };
    match id & 0x3ff {
        0x09 => Some(Lang::En),
        0x19 => Some(Lang::Ru),
        0x12 => Some(Lang::Ko),
        0x04 if matches!(id, 0x0804 | 0x1004) => Some(Lang::Zh),
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
        assert_eq!([0, 1, 2, 5].map(|n| Lang::Ko.form(n)), [0, 0, 0, 0]);
        assert_eq!([0, 1, 2, 5].map(|n| Lang::Zh.form(n)), [0, 0, 0, 0]);
    }

    #[test]
    fn locale_names_give_their_language() {
        assert_eq!(Lang::of_locale("ru_RU.UTF-8"), Some(Lang::Ru));
        assert_eq!(Lang::of_locale("en-GB"), Some(Lang::En));
        assert_eq!(Lang::of_locale("ko_KR.UTF-8"), Some(Lang::Ko));
        assert_eq!(Lang::of_locale("zh_CN.UTF-8"), Some(Lang::Zh));
        assert_eq!(Lang::of_locale("zh-Hans-CN"), Some(Lang::Zh));
        assert_eq!(Lang::of_locale("zh_SG"), Some(Lang::Zh));
        assert_eq!(Lang::of_locale("zh_TW.UTF-8"), None);
        assert_eq!(Lang::of_locale("zh-Hant-HK"), None);
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

    /// A key the sources use, and where.
    #[derive(Debug)]
    struct Key {
        key: String,
        /// Whether it is counted (`trn!`).
        counted: bool,
        /// The names of the values given after it.
        values: BTreeSet<String>,
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

    /// The source with each comment line blanked, its lines kept: a
    /// `tr!("…")` that a comment quotes translates nothing.
    fn without_comment_lines(src: &str) -> String {
        src.split_inclusive('\n').map(|line| if line.trim_start().starts_with("//") { "\n" } else { line }).collect()
    }

    /// Every `tr!`, `trn!` and `tr_const!` key in the `.rs` files under `dir`.
    fn keys(dir: &Path, out: &mut Vec<Key>) {
        for entry in std::fs::read_dir(dir).unwrap().flatten() {
            let path = entry.path();
            if path.is_dir() {
                keys(&path, out);
            } else if path.extension().is_some_and(|e| e == "rs") && !path.ends_with("i18n.rs") {
                let src = without_comment_lines(&std::fs::read_to_string(&path).unwrap());
                let name = path.file_name().unwrap().to_string_lossy().into_owned();
                for pat in ["tr!(", "trn!(", "tr_const!("] {
                    // A macro name of its own: not the end of a longer one.
                    let starts = src
                        .match_indices(pat)
                        .map(|(pos, _)| pos)
                        .filter(|&pos| !src[..pos].ends_with(|c: char| c.is_alphanumeric() || c == '_'));
                    for pos in starts {
                        let at = format!("{name}:{}", src[..pos].lines().count());
                        let mut args = &src[pos + pat.len()..];
                        if pat == "trn!(" {
                            // trn!(count, "key"): the count runs to the key.
                            args = &args[args.find('"').unwrap_or_else(|| panic!("{at}: trn! needs its key"))..];
                        }
                        let (key, rest) = literal(args).unwrap_or_else(|| panic!("{at}: {pat} takes a string literal"));
                        let mut values = values(rest);
                        if pat == "trn!(" {
                            values.insert("n".into());
                        }
                        out.push(Key { key, counted: pat == "trn!(", values, at });
                    }
                }
            }
        }
    }

    /// The keys of a texts file in its order, each with the comment on the
    /// line right above it.
    fn layout(file: &str) -> Vec<(String, Option<String>)> {
        let (mut section, mut comment, mut out) = (String::new(), None, Vec::new());
        for line in file.lines().map(str::trim) {
            if let Some(name) = line.strip_prefix('[').and_then(|l| l.strip_suffix(']')) {
                section = name.to_string();
                comment = None;
            } else if let Some(text) = line.strip_prefix('#') {
                comment = Some(text.strip_prefix(' ').unwrap_or(text).to_string());
            } else if let Some((name, _)) = line.split_once('=') {
                out.push((format!("{section}.{}", name.trim()), comment.take()));
            } else {
                comment = None;
            }
        }
        out
    }

    /// The comment a translation carries over a key: its English, the
    /// forms of a counted one between bars, a line break as `\n`, a text
    /// with spaces at its ends in quotes.
    fn english_comment(text: &Text) -> String {
        let forms = match text {
            Text::One(t) => vec![*t],
            Text::Forms(f) => f.clone(),
        };
        forms
            .iter()
            .map(|t| t.replace('\n', "\\n"))
            .map(|t| if t.trim() == t { t } else { format!("\"{t}\"") })
            .collect::<Vec<_>>()
            .join(" | ")
    }

    fn all_placeholders(text: &Text) -> BTreeSet<&str> {
        match text {
            Text::One(t) => placeholders(t),
            Text::Forms(f) => f.iter().flat_map(|t| placeholders(t)).collect(),
        }
    }

    fn placeholders(text: &str) -> BTreeSet<&str> {
        text.split('{')
            .skip(1)
            .filter_map(|p| p.split_once('}').map(|(name, _)| name))
            .filter(|n| n.chars().all(|c| c.is_alphanumeric() || c == '_'))
            .collect()
    }

    #[test]
    fn the_code_uses_the_keys_of_the_english_file() {
        let mut found = Vec::new();
        keys(&Path::new(env!("CARGO_MANIFEST_DIR")).join("src"), &mut found);
        assert!(found.len() > 10, "the scan of the sources found almost nothing");
        let en = parse(Lang::En.file()).unwrap_or_else(|e| panic!("locales/en.toml: {e}"));
        let mut problems = Vec::new();
        for key in &found {
            let fault = match (en.get(key.key.as_str()), key.counted) {
                (None, _) => Some("is not in locales/en.toml".to_string()),
                (Some(Text::Forms(_)), false) => Some("is counted in en.toml: use trn!".to_string()),
                (Some(Text::One(_)), true) => Some("is not counted in en.toml: give it [one, many]".to_string()),
                (Some(Text::Forms(f)), true) if f.len() != Lang::En.forms() => Some("needs two forms in en.toml".to_string()),
                (Some(text), _) => {
                    let named: BTreeSet<String> = all_placeholders(text).into_iter().map(String::from).collect();
                    (key.values != named).then(|| format!("names {named:?} but is given {:?}", key.values))
                }
            };
            if let Some(fault) = fault {
                problems.push(format!("{}: {:?} {fault}", key.at, key.key));
            }
        }
        let used: BTreeSet<&str> = found.iter().map(|k| k.key.as_str()).collect();
        let mut stale: Vec<&&str> = en.keys().filter(|k| !used.contains(**k)).collect();
        stale.sort();
        if !stale.is_empty() {
            problems.push(format!("locales/en.toml has keys the program does not use: {stale:?}"));
        }
        assert!(problems.is_empty(), "{}", problems.join("\n"));
    }

    #[test]
    fn every_language_follows_the_english_file() {
        let en = parse(Lang::En.file()).unwrap();
        let order: Vec<String> = layout(Lang::En.file()).into_iter().map(|(k, _)| k).collect();
        assert_eq!(order.len(), en.len(), "locales/en.toml: one line a key, in sections");
        let mut problems = Vec::new();
        for lang in Lang::ALL.into_iter().filter(|&l| l != Lang::En) {
            let file = format!("locales/{}.toml", lang.code());
            let table = parse(lang.file()).unwrap_or_else(|e| panic!("{file}: {e}"));
            let lines = layout(lang.file());
            let keys: Vec<&String> = lines.iter().map(|(k, _)| k).collect();
            if keys != order.iter().collect::<Vec<_>>() {
                let missing: Vec<&String> = order.iter().filter(|k| !keys.contains(k)).collect();
                let extra: Vec<&&String> = keys.iter().filter(|k| !order.contains(k)).collect();
                problems.push(format!("{file}: keys not as in en.toml; lacks {missing:?}, has besides {extra:?}"));
            }
            for (key, comment) in &lines {
                let (Some(english), Some(text)) = (en.get(key.as_str()), table.get(key.as_str())) else { continue };
                let wanted = english_comment(english);
                if comment.as_deref() != Some(wanted.as_str()) {
                    problems.push(format!("{file}: {key}: the line above should be `# {wanted}`, is {comment:?}"));
                }
                let fault = match (english, text) {
                    (Text::One(_), Text::Forms(_)) => Some("is not counted: give one text".to_string()),
                    (Text::Forms(_), Text::One(_)) => Some("is counted: give its forms".to_string()),
                    (Text::Forms(_), Text::Forms(f)) if f.len() != lang.forms() => Some(format!("needs {} forms", lang.forms())),
                    (Text::Forms(_), Text::Forms(f)) => {
                        let named = all_placeholders(english);
                        f.iter().find(|t| !placeholders(t).is_subset(&named)).map(|t| format!("{t:?} has braces the English lacks"))
                    }
                    (Text::One(e), Text::One(t)) => (placeholders(t) != placeholders(e)).then(|| format!("{t:?} has other braces")),
                };
                if let Some(fault) = fault {
                    problems.push(format!("{file}: {key} {fault}"));
                }
            }
        }
        assert!(problems.is_empty(), "{}", problems.join("\n"));
    }

    #[test]
    fn a_text_falls_back_to_english_then_to_its_key() {
        set(Lang::Ru);
        let shown = (tr("common.cancel"), tr("no.such.key"));
        set(Lang::En);
        assert_eq!(shown, ("Отмена", "no.such.key"));
        assert_eq!(tr("common.cancel"), "Cancel");
        assert_eq!(fill(tr_n(5, "library.nTracks"), &[("n", &5)]), "5 tracks");
    }
}
