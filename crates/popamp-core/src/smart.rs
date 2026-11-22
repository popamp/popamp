//! Smart playlists: lists of library tracks picked by rules, kept in
//! `smart.toml`.
//!
//! A list is `[[smart]]` with a `name`, whether `all` or `any` of its
//! rules must hold (`match`), its order (`sort`, by several fields) and
//! how much of the result it keeps (`limit`, in tracks or minutes). A rule
//! is `[[smart.rule]]` with `field`, `op` and `value`, or a group of rules
//! with its own `match` and `rules`. The rules are a structure, not a
//! query language, so there is nothing to parse but TOML.
//!
//! Terms:
//! - *test*: a rule on one field of a track.
//! - *group*: rules that hold together (`all`) or one of them (`any`).

use std::cmp::Ordering;
use std::hash::{DefaultHasher, Hash, Hasher};
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::library::{Filter, Library, Sort, Track};
use crate::stats::TrackStats;
use crate::text::natural_cmp;

/// What `smart.toml` says of itself at its top, each time it is written.
const HEADER: &str = r#"# Smart playlists of Popamp's media library.
#
# Each [[smart]] is a list: a name, its rules, and optionally
#   match = "all" (every rule holds, the default) or "any" (one does),
#   sort  = [{ by = "field", desc = true }, ...]  (several levels;
#           by = "random" shuffles, anew each time the list is opened),
#   limit = { tracks = 100 } or { minutes = 60 }.
# A rule is field + op + value:
#   any           (every word in some tag or the path) op: has
#   text fields   title artist album genre path      ops: is has missing
#   number fields year number length(seconds)
#                 plays rating(0-5 stars)            ops: is less greater missing
#   date fields   added played                       ops: in_last (days) missing
# A rule can instead be a group: { match = "any", rules = [ {...}, {...} ] }.
"#;

/// The lists written into `smart.toml` when there is no such file; from
/// then on they are the user's, to change or remove.
const BUILT_IN: &str = r#"
# Popamp wrote the lists below once; change or remove them as you like.

[[smart]]
name = "Recently added"
sort = [{ by = "added", desc = true }]
[[smart.rule]]
field = "added"
op = "in_last"
value = 30

[[smart]]
name = "Last played"
sort = [{ by = "played", desc = true }]
limit = { tracks = 100 }
[[smart.rule]]
field = "plays"
op = "greater"
value = 0

[[smart]]
name = "Most played"
sort = [{ by = "plays", desc = true }, { by = "played", desc = true }]
limit = { tracks = 100 }
[[smart.rule]]
field = "plays"
op = "greater"
value = 0

[[smart]]
name = "Never played"
[[smart.rule]]
field = "plays"
op = "is"
value = 0

[[smart]]
name = "Random 50"
sort = [{ by = "random" }]
limit = { tracks = 50 }

[[smart]]
name = "Favourites"
sort = [{ by = "rating", desc = true }]
[[smart.rule]]
field = "rating"
op = "greater"
value = 3
"#;

const DAY: u64 = 86_400;

/// The whole of a new `smart.toml`.
fn new_file() -> String {
    format!("{HEADER}{BUILT_IN}")
}

/// What a rule can look at.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Field {
    /// Every word somewhere in the tags or the path, as the library's
    /// search finds tracks.
    Any,
    Title,
    Artist,
    Album,
    Genre,
    Path,
    Year,
    /// The track number.
    Number,
    /// In seconds.
    Length,
    Plays,
    /// Stars, 0 when unrated.
    Rating,
    /// When the library first saw the file.
    Added,
    /// When the track last counted as listened to.
    Played,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Kind {
    Search,
    Text,
    Number,
    Date,
}

impl Field {
    /// Every field, in the order a form offers them.
    pub const ALL: [Field; 13] = [
        Field::Any,
        Field::Title,
        Field::Artist,
        Field::Album,
        Field::Genre,
        Field::Year,
        Field::Number,
        Field::Length,
        Field::Path,
        Field::Rating,
        Field::Plays,
        Field::Played,
        Field::Added,
    ];

    fn kind(self) -> Kind {
        match self {
            Field::Any => Kind::Search,
            Field::Title | Field::Artist | Field::Album | Field::Genre | Field::Path => Kind::Text,
            Field::Year | Field::Number | Field::Length | Field::Plays | Field::Rating => Kind::Number,
            Field::Added | Field::Played => Kind::Date,
        }
    }

    fn name(self) -> String {
        format!("{self:?}").to_lowercase()
    }

    /// The ops that suit the field.
    pub fn ops(self) -> &'static [Op] {
        match self.kind() {
            Kind::Search => &[Op::Has],
            Kind::Text => &[Op::Is, Op::Has, Op::Missing],
            Kind::Number => &[Op::Is, Op::Less, Op::Greater, Op::Missing],
            Kind::Date => &[Op::InLast, Op::Missing],
        }
    }

    fn text(self, t: &Track) -> &str {
        match self {
            Field::Title => &t.title,
            Field::Artist => &t.artist,
            Field::Album => &t.album,
            Field::Genre => &t.genre,
            Field::Path => &t.path,
            _ => "",
        }
    }

    /// The field as a number (a date in seconds since the Unix epoch);
    /// none when the track has no such value.
    fn number(self, t: &Track, s: &TrackStats) -> Option<u64> {
        let set = |n: u64| (n > 0).then_some(n);
        match self {
            Field::Year => t.year.split(|c: char| !c.is_ascii_digit()).next().and_then(|y| y.parse().ok()),
            Field::Number => set(t.number.into()),
            Field::Length => t.length_ms.map(|ms| ms / 1000),
            Field::Plays => Some(s.plays.into()),
            Field::Rating => Some(s.rating.stars().into()),
            Field::Added => set(t.added),
            Field::Played => set(s.last_played),
            _ => None,
        }
    }

    fn compare(self, a: (&Track, &TrackStats), b: (&Track, &TrackStats)) -> Ordering {
        match self.kind() {
            Kind::Text => natural_cmp(self.text(a.0), self.text(b.0)),
            _ => self.number(a.0, a.1).cmp(&self.number(b.0, b.1)),
        }
    }
}

/// How a rule's field is tested, as written in the file.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Op {
    Is,
    Has,
    Less,
    Greater,
    Missing,
    InLast,
}

impl Op {
    /// How a number compares to the value for the op to hold.
    fn order(self) -> Ordering {
        match self {
            Op::Less => Ordering::Less,
            Op::Greater => Ordering::Greater,
            _ => Ordering::Equal,
        }
    }

    fn name(self) -> &'static str {
        match self {
            Op::Is => "is",
            Op::Has => "has",
            Op::Less => "less",
            Op::Greater => "greater",
            Op::Missing => "missing",
            Op::InLast => "in_last",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(untagged)]
enum Value {
    Number(u64),
    Text(String),
}

/// Whether every rule must hold or one is enough.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Match {
    #[default]
    All,
    Any,
}

/// A rule on one field; only the tests that suit the field's kind exist.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Test {
    /// The text equals the value, whatever the case.
    TextIs(Field, String),
    /// The text holds the value, whatever the case; for [`Field::Any`],
    /// each word of it is somewhere.
    TextHas(Field, String),
    /// The field's number compares to the value as given.
    Number(Field, Ordering, u64),
    /// The date is no more than this many days ago.
    InLast(Field, u64),
    /// The track has no value: empty text, no number or 0, no date.
    Missing(Field),
}

impl Test {
    fn new(field: Field, op: Op, value: Option<Value>) -> Result<Test, String> {
        if !field.ops().contains(&op) {
            return Err(format!("\"{}\" does not suit the field \"{}\"", op.name(), field.name()));
        }
        if op == Op::Missing {
            return Ok(Test::Missing(field));
        }
        match field.kind() {
            Kind::Search | Kind::Text if op == Op::Is => text_value(value).map(|v| Test::TextIs(field, v)),
            Kind::Search | Kind::Text => text_value(value).map(|v| Test::TextHas(field, v)),
            _ if op == Op::InLast => number_value(value).map(|n| Test::InLast(field, n)),
            _ => number_value(value).map(|n| Test::Number(field, op.order(), n)),
        }
    }

    /// A test from a form's field, op and the text typed as its value.
    pub fn from_text(field: Field, op: Op, value: &str) -> Result<Test, String> {
        let value = (!value.trim().is_empty()).then(|| Value::Text(value.trim().to_string()));
        Test::new(field, op, value)
    }

    /// The field, op and value, as a form shows them; the value is empty
    /// for `missing`.
    pub fn parts(&self) -> (Field, Op, String) {
        let (field, op, value) = self.raw();
        let text = match value {
            Some(Value::Text(s)) => s,
            Some(Value::Number(n)) => n.to_string(),
            None => String::new(),
        };
        (field, op, text)
    }

    fn raw(&self) -> (Field, Op, Option<Value>) {
        match self {
            Test::TextIs(f, v) => (*f, Op::Is, Some(Value::Text(v.clone()))),
            Test::TextHas(f, v) => (*f, Op::Has, Some(Value::Text(v.clone()))),
            Test::Number(f, order, n) => {
                let op = match order {
                    Ordering::Less => Op::Less,
                    Ordering::Equal => Op::Is,
                    Ordering::Greater => Op::Greater,
                };
                (*f, op, Some(Value::Number(*n)))
            }
            Test::InLast(f, days) => (*f, Op::InLast, Some(Value::Number(*days))),
            Test::Missing(f) => (*f, Op::Missing, None),
        }
    }

    fn passes(&self, t: &Track, s: &TrackStats, now: u64) -> bool {
        match self {
            Test::TextIs(f, v) => f.text(t).to_lowercase() == v.to_lowercase(),
            Test::TextHas(Field::Any, v) => t.has(&words(v)),
            Test::TextHas(f, v) => f.text(t).to_lowercase().contains(&v.to_lowercase()),
            Test::Number(f, order, v) => f.number(t, s).is_some_and(|n| n.cmp(v) == *order),
            Test::InLast(f, days) => f.number(t, s).is_some_and(|at| at.saturating_add(days.saturating_mul(DAY)) >= now),
            Test::Missing(f) if f.kind() == Kind::Text => f.text(t).trim().is_empty(),
            Test::Missing(f) => f.number(t, s).is_none_or(|n| n == 0),
        }
    }
}

fn text_value(value: Option<Value>) -> Result<String, String> {
    match value {
        Some(Value::Text(s)) => Ok(s),
        Some(Value::Number(n)) => Ok(n.to_string()),
        None => Err("the rule needs a value".into()),
    }
}

fn number_value(value: Option<Value>) -> Result<u64, String> {
    match value {
        Some(Value::Number(n)) => Ok(n),
        Some(Value::Text(s)) => s.trim().parse().map_err(|_| format!("\"{s}\" is not a whole number")),
        None => Err("the rule needs a value".into()),
    }
}

/// A test, or a group of rules.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(try_from = "RawRule", into = "RawRule")]
pub enum Rule {
    Test(Test),
    Group { matching: Match, rules: Vec<Rule> },
}

/// A rule as the file has it, checked into a [`Rule`] as it is read.
#[derive(Deserialize, Serialize, Default)]
#[serde(deny_unknown_fields)]
struct RawRule {
    #[serde(skip_serializing_if = "Option::is_none")]
    field: Option<Field>,
    #[serde(skip_serializing_if = "Option::is_none")]
    op: Option<Op>,
    #[serde(skip_serializing_if = "Option::is_none")]
    value: Option<Value>,
    #[serde(rename = "match", skip_serializing_if = "Option::is_none")]
    matching: Option<Match>,
    #[serde(skip_serializing_if = "Option::is_none")]
    rules: Option<Vec<Rule>>,
}

impl From<Rule> for RawRule {
    fn from(rule: Rule) -> RawRule {
        match rule {
            Rule::Test(test) => {
                let (field, op, value) = test.raw();
                RawRule { field: Some(field), op: Some(op), value, ..RawRule::default() }
            }
            Rule::Group { matching, rules } => RawRule { matching: Some(matching), rules: Some(rules), ..RawRule::default() },
        }
    }
}

impl TryFrom<RawRule> for Rule {
    type Error = String;

    fn try_from(r: RawRule) -> Result<Rule, String> {
        match (r.field, r.rules) {
            (Some(field), None) => {
                let op = r.op.ok_or_else(|| format!("the rule on \"{}\" needs an op", field.name()))?;
                Test::new(field, op, r.value).map(Rule::Test)
            }
            (None, Some(rules)) => Ok(Rule::Group { matching: r.matching.unwrap_or_default(), rules }),
            (Some(_), Some(_)) => Err("a rule has a field or rules, not both".into()),
            (None, None) => Err("a rule needs a field and an op, or rules".into()),
        }
    }
}

impl Rule {
    fn passes(&self, t: &Track, s: &TrackStats, now: u64) -> bool {
        match self {
            Rule::Test(test) => test.passes(t, s, now),
            Rule::Group { matching, rules } => holds(*matching, rules, t, s, now),
        }
    }
}

/// Whether `rules` hold for the track as `matching` asks; no rules let
/// every track through.
fn holds(matching: Match, rules: &[Rule], t: &Track, s: &TrackStats, now: u64) -> bool {
    match matching {
        _ if rules.is_empty() => true,
        Match::All => rules.iter().all(|r| r.passes(t, s, now)),
        Match::Any => rules.iter().any(|r| r.passes(t, s, now)),
    }
}

/// What a list is ordered by.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SortBy {
    Field(Field),
    /// A shuffle, fixed by the seed the list is drawn with.
    Random,
}

const RANDOM: &str = "random";

impl<'de> Deserialize<'de> for SortBy {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<SortBy, D::Error> {
        use serde::de::IntoDeserializer;
        let word = String::deserialize(d)?;
        if word == RANDOM {
            return Ok(SortBy::Random);
        }
        Field::deserialize(word.as_str().into_deserializer()).map(SortBy::Field)
    }
}

impl Serialize for SortBy {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        match self {
            SortBy::Field(f) => f.serialize(s),
            SortBy::Random => s.serialize_str(RANDOM),
        }
    }
}

/// Where a track falls in the shuffle of `seed`.
fn shuffled(seed: u64, t: &Track) -> u64 {
    let mut h = DefaultHasher::new();
    (seed, &t.path).hash(&mut h);
    h.finish()
}

/// One level of a list's order.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct SortKey {
    pub by: SortBy,
    #[serde(default, skip_serializing_if = "is_false")]
    pub desc: bool,
}

fn is_false(b: &bool) -> bool {
    !*b
}

/// How much of its result a list keeps, from the top.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "lowercase", deny_unknown_fields)]
pub enum Limit {
    Tracks(usize),
    Minutes(u64),
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct SmartList {
    pub name: String,
    #[serde(rename = "match", default)]
    pub matching: Match,
    /// Ties, and a list without an order, go by artist and album.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub sort: Vec<SortKey>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub limit: Option<Limit>,
    #[serde(rename = "rule", default)]
    pub rules: Vec<Rule>,
}

impl SmartList {
    /// The list's tracks, as places in `lib.tracks`, at `now` (seconds
    /// since the Unix epoch). `query` searches within them, as the
    /// library's search does, after the limit. `seed` fixes a random
    /// order: the same seed, the same shuffle.
    pub fn tracks(&self, lib: &Library, query: &str, now: u64, seed: u64) -> Vec<usize> {
        let all = lib.find(&Filter::default(), Sort::Artist, false);
        let mut found: Vec<(usize, TrackStats)> = all
            .into_iter()
            .map(|i| (i, lib.stats.get(&lib.tracks[i].path)))
            .filter(|(i, s)| holds(self.matching, &self.rules, &lib.tracks[*i], s, now))
            .collect();
        found.sort_by(|(a, sa), (b, sb)| self.order(seed, (&lib.tracks[*a], sa), (&lib.tracks[*b], sb)));
        let mut found: Vec<usize> = found.into_iter().map(|(i, _)| i).collect();
        self.cut(lib, &mut found);
        let words = words(query);
        found.retain(|&i| words.is_empty() || lib.tracks[i].has(&words));
        found
    }

    fn order(&self, seed: u64, a: (&Track, &TrackStats), b: (&Track, &TrackStats)) -> Ordering {
        let level = |k: &SortKey| {
            let o = match k.by {
                SortBy::Field(f) => f.compare(a, b),
                SortBy::Random => shuffled(seed, a.0).cmp(&shuffled(seed, b.0)),
            };
            if k.desc {
                o.reverse()
            } else {
                o
            }
        };
        self.sort.iter().map(level).find(|o| o.is_ne()).unwrap_or(Ordering::Equal)
    }

    fn cut(&self, lib: &Library, found: &mut Vec<usize>) {
        match self.limit {
            None => {}
            Some(Limit::Tracks(n)) => found.truncate(n),
            Some(Limit::Minutes(m)) => {
                let mut total = 0;
                let fits = found.iter().take_while(|&&i| {
                    total += lib.tracks[i].length_ms.unwrap_or(0);
                    total <= m * 60_000
                });
                let n = fits.count();
                found.truncate(n);
            }
        }
    }
}

/// The words of a search, in lower case.
fn words(text: &str) -> Vec<String> {
    text.split_whitespace().map(str::to_lowercase).collect()
}

/// `smart.toml` holding `lists`.
pub fn to_text(lists: &[SmartList]) -> String {
    #[derive(Serialize)]
    struct File<'a> {
        smart: &'a [SmartList],
    }
    let body = toml::to_string(&File { smart: lists }).unwrap_or_else(|e| {
        log::warn!("smart playlists: cannot write them out: {e}");
        String::new()
    });
    format!("{HEADER}\n{body}")
}

/// Writes `lists` as the whole of `path`.
pub fn save(path: &Path, lists: &[SmartList]) -> std::io::Result<()> {
    crate::files::replace(path, &to_text(lists))
}

/// The lists in `text`, and what was wrong with the ones left out.
pub fn parse(text: &str) -> (Vec<SmartList>, Vec<String>) {
    let table: toml::Table = match text.parse() {
        Ok(t) => t,
        Err(e) => return (Vec::new(), vec![e.to_string()]),
    };
    let Some(entries) = table.get("smart") else { return (Vec::new(), Vec::new()) };
    let Some(entries) = entries.as_array() else { return (Vec::new(), vec!["\"smart\" must be [[smart]] lists".into()]) };
    let (mut lists, mut errors) = (Vec::new(), Vec::new());
    for (n, entry) in entries.iter().enumerate() {
        match entry.clone().try_into::<SmartList>() {
            Ok(list) => lists.push(list),
            Err(e) => errors.push(format!("list {}: {}", n + 1, e.message())),
        }
    }
    (lists, errors)
}

/// The lists of `path`. When there is no such file the built-in lists
/// are written there first, so they come once and are the user's after.
pub fn load(path: &Path) -> Vec<SmartList> {
    let text = match std::fs::read_to_string(path) {
        Ok(text) => text,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            let text = new_file();
            if let Err(e) = crate::files::replace(path, &text) {
                log::warn!("smart playlists: cannot write {}: {e}", path.display());
            }
            text
        }
        Err(e) => {
            log::warn!("smart playlists: cannot read {}: {e}", path.display());
            return Vec::new();
        }
    };
    let (lists, errors) = parse(&text);
    for e in errors {
        log::warn!("smart playlists: {}: {e}", path.display());
    }
    lists
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::stats::Rating;

    const NOW: u64 = 1_000 * DAY;

    /// An empty folder of its own for test `name`.
    fn dir(name: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("popamp-smart-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        dir
    }

    fn track(path: &str, artist: &str, title: &str, added_days_ago: u64, secs: u64) -> Track {
        Track {
            path: path.into(),
            artist: artist.into(),
            title: title.into(),
            added: NOW - added_days_ago * DAY,
            length_ms: Some(secs * 1000),
            ..Track::default()
        }
    }

    /// A library with stats; `plays` per track, in order.
    fn library(dir: &Path) -> Library {
        let mut lib = Library {
            tracks: vec![
                track("/m/a.mp3", "Alpha", "Apple", 5, 200),
                track("/m/b.mp3", "Beta", "Banana", 40, 300),
                track("/m/c.mp3", "Gamma", "Cherry", 2, 100),
                track("/m/d.mp3", "", "Date", 100, 400),
            ],
            ..Library::default()
        };
        let file = dir.join("stats.txt");
        for (path, plays, at) in [("/m/a.mp3", 2, 10), ("/m/b.mp3", 5, 3), ("/m/c.mp3", 1, 1)] {
            for _ in 0..plays {
                lib.stats.record_play(&file, path, NOW - at * DAY);
            }
        }
        lib.stats.rate(&file, "/m/b.mp3", Rating::new(5).unwrap());
        lib.stats.rate(&file, "/m/c.mp3", Rating::new(4).unwrap());
        lib
    }

    fn titles(lib: &Library, list: &SmartList, query: &str) -> Vec<String> {
        list.tracks(lib, query, NOW, 0).into_iter().map(|i| lib.tracks[i].title.clone()).collect()
    }

    fn one(text: &str) -> SmartList {
        let (lists, errors) = parse(text);
        assert_eq!(errors, Vec::<String>::new());
        lists.into_iter().next().expect("a list")
    }

    fn built_in(name: &str) -> SmartList {
        parse(&new_file()).0.into_iter().find(|l| l.name == name).expect("built in")
    }

    #[test]
    fn the_built_in_lists_read_and_pick_their_tracks() {
        let dir = dir("the_built_in_lists_r");
        let lib = library(&dir);
        let (lists, errors) = parse(&new_file());
        assert_eq!((lists.len(), errors), (6, vec![]));
        assert_eq!(titles(&lib, &built_in("Recently added"), ""), ["Cherry", "Apple"]);
        assert_eq!(titles(&lib, &built_in("Last played"), ""), ["Cherry", "Banana", "Apple"]);
        assert_eq!(titles(&lib, &built_in("Most played"), ""), ["Banana", "Apple", "Cherry"]);
        assert_eq!(titles(&lib, &built_in("Never played"), ""), ["Date"]);
        assert_eq!(titles(&lib, &built_in("Favourites"), ""), ["Banana", "Cherry"]);
    }

    #[test]
    fn text_rules_ignore_case_and_a_group_takes_any_of_its_rules() {
        let dir = dir("text_rules_ignore_ca");
        let lib = library(&dir);
        let list = one(r#"
            [[smart]]
            name = "x"
            [[smart.rule]]
            field = "length"
            op = "less"
            value = 350
            [[smart.rule]]
            match = "any"
            rules = [{ field = "artist", op = "is", value = "ALPHA" }, { field = "title", op = "has", value = "ERR" }]
            "#);
        assert_eq!(titles(&lib, &list, ""), ["Apple", "Cherry"]);
        assert_eq!(titles(&lib, &list, "cher"), ["Cherry"], "the search narrows the list");
    }

    #[test]
    fn missing_finds_the_empty_and_any_takes_one_rule() {
        let dir = dir("missing_finds_the_em");
        let lib = library(&dir);
        let list = one(r#"
            [[smart]]
            name = "x"
            match = "any"
            rule = [{ field = "artist", op = "missing" }, { field = "rating", op = "is", value = 4 }]
            "#);
        assert_eq!(titles(&lib, &list, ""), ["Date", "Cherry"], "no artist comes first in artist order");
    }

    #[test]
    fn a_limit_keeps_the_top_by_tracks_or_minutes() {
        let dir = dir("a_limit_keeps_the_to");
        let lib = library(&dir);
        let by = |limit: &str| one(&format!("[[smart]]\nname = \"x\"\nsort = [{{ by = \"length\" }}]\nlimit = {limit}\n"));
        assert_eq!(titles(&lib, &by("{ tracks = 2 }"), ""), ["Cherry", "Apple"]);
        assert_eq!(titles(&lib, &by("{ minutes = 10 }"), ""), ["Cherry", "Apple", "Banana"], "100+200+300 s fit in 600");
        let none = one("[[smart]]\nname = \"x\"\n");
        assert_eq!(titles(&lib, &none, "").len(), 4, "no rules is every track");
    }

    #[test]
    fn a_random_order_holds_for_its_seed_and_changes_with_it() {
        let dir = dir("a_random_order_holds");
        let lib = library(&dir);
        let list = built_in("Random 50");
        let order = |seed| list.tracks(&lib, "", NOW, seed);
        assert_eq!(order(1).len(), 4, "every track, up to 50");
        assert_eq!(order(1), order(1), "the same seed, the same order");
        assert!((2..20).any(|seed| order(seed) != order(1)), "another seed, another order");
        let text = to_text(std::slice::from_ref(&list));
        assert!(text.contains(r#"by = "random""#), "{text}");
        assert_eq!(parse(&text).0, [list], "and it reads back");
    }

    #[test]
    fn a_wrong_list_is_left_out_with_the_reason_and_the_rest_read() {
        let text = r#"
            [[smart]]
            name = "bad op"
            rule = [{ field = "added", op = "has", value = "x" }]
            [[smart]]
            name = "bad value"
            rule = [{ field = "year", op = "less", value = "soon" }]
            [[smart]]
            name = "unknown field"
            rule = [{ field = "mood", op = "is", value = "x" }]
            [[smart]]
            name = "good"
            rule = [{ field = "year", op = "greater", value = "1990" }]
            "#;
        let (lists, errors) = parse(text);
        assert_eq!(lists.iter().map(|l| l.name.as_str()).collect::<Vec<_>>(), ["good"]);
        assert_eq!(errors.len(), 3);
        assert!(errors[0].contains("\"has\" does not suit the field \"added\""), "{}", errors[0]);
        assert!(errors[1].contains("\"soon\" is not a whole number"), "{}", errors[1]);
        assert!(errors[2].contains("mood"), "{}", errors[2]);
        assert_eq!(parse("not toml =").0, vec![]);
    }

    #[test]
    fn the_built_in_lists_are_written_once_and_then_are_the_users() {
        let dir = dir("the_built_in_lists_a");
        let file = dir.join("smart.toml");
        assert_eq!(load(&file).len(), 6);
        assert_eq!(std::fs::read_to_string(&file).unwrap(), new_file());
        std::fs::write(&file, "# all gone\n").unwrap();
        assert_eq!(load(&file), vec![], "removed lists do not come back");
    }

    #[test]
    fn lists_written_out_read_back_the_same() {
        let mut lists = parse(&new_file()).0;
        lists.push(one(r#"
            [[smart]]
            name = "Nested"
            match = "any"
            limit = { minutes = 90 }
            [[smart.rule]]
            field = "artist"
            op = "is"
            value = "AC/DC"
            [[smart.rule]]
            match = "all"
            rules = [{ field = "genre", op = "missing" }, { field = "any", op = "has", value = "live 1977" }]
            "#));
        let text = to_text(&lists);
        assert!(text.starts_with(HEADER), "the help stays at the top");
        assert_eq!(parse(&text), (lists, vec![]));
    }

    #[test]
    fn any_finds_every_word_somewhere_as_the_search_does() {
        let dir = dir("any_finds");
        let lib = library(&dir);
        let list = one("[[smart]]\nname = \"x\"\nrule = [{ field = \"any\", op = \"has\", value = \"GAMMA cher\" }]\n");
        assert_eq!(titles(&lib, &list, ""), ["Cherry"]);
        let test = Test::from_text(Field::Title, Op::Is, " Apple ").unwrap();
        assert_eq!(test.parts(), (Field::Title, Op::Is, "Apple".to_string()), "typed text keeps its case");
        assert!(Test::from_text(Field::Any, Op::Is, "x").is_err(), "any only has");
        assert!(Test::from_text(Field::Title, Op::Has, "").is_err(), "has needs a value");
    }

    #[test]
    fn a_year_reads_from_a_full_date() {
        let t = Track { year: "1999-05-12".into(), ..Track::default() };
        assert_eq!(Field::Year.number(&t, &TrackStats::default()), Some(1999));
        assert_eq!(Field::Year.number(&Track::default(), &TrackStats::default()), None);
    }
}
