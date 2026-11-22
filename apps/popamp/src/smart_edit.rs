//! The smart playlist editor: a form with a row per rule, as the rules
//! are kept in `smart.toml`. A row is a field, an op and a value, or a
//! group whose rules are the indented rows under it.
//!
//! Terms:
//! - *draft*: the list as the form shows it, rows flat, groups by depth;
//!   it becomes a [`SmartList`] on OK, when its rules are checked.

use crate::library::Editing;
use crate::model::Model;
use crate::panels::{button, Act};
use crate::widgets::{Event, Form, Rect, Widget, BUTTON};
use popamp_core::smart::{Field, Limit, Match, Op, Rule, SmartList, SortBy, SortKey, Test};

const W: i32 = 660;
/// The height of a rule row.
const ROW: i32 = 30;
const RULES_Y: i32 = 76;
/// Rows shown before the rules scroll.
const MAX_ROWS: usize = 9;

/// The height of the rules area: its rows and the add buttons, no more.
fn rules_h(rows: usize) -> i32 {
    rows.min(MAX_ROWS) as i32 * ROW + 40
}
/// How far a group's rules are indented.
const INDENT: i32 = 24;
const MATCHES: [&str; 2] = ["all", "any"];
const LIMITS: [&str; 3] = ["No limit", "Tracks", "Minutes"];
/// The levels of order the form offers; more in the file are kept.
const LEVELS: usize = 2;

fn field_label(f: Field) -> &'static str {
    match f {
        Field::Any => "Any text",
        Field::Title => "Title",
        Field::Artist => "Artist",
        Field::Album => "Album",
        Field::Genre => "Genre",
        Field::Year => "Year",
        Field::Number => "Track number",
        Field::Length => "Length (seconds)",
        Field::Path => "File path",
        Field::Rating => "Rating (stars)",
        Field::Plays => "Plays",
        Field::Played => "Last played",
        Field::Added => "Date added",
    }
}

fn op_label(op: Op) -> &'static str {
    match op {
        Op::Is => "is",
        Op::Has => "contains",
        Op::Less => "is less than",
        Op::Greater => "is more than",
        Op::Missing => "is empty",
        Op::InLast => "in the last (days)",
    }
}

/// The fields a list can be ordered by: all but the search over every tag.
/// What a list can be ordered by, in the order the form offers it.
fn sort_choices() -> Vec<SortBy> {
    Field::ALL.into_iter().filter(|&f| f != Field::Any).map(SortBy::Field).chain([SortBy::Random]).collect()
}

fn sort_label(by: SortBy) -> &'static str {
    match by {
        SortBy::Field(f) => field_label(f),
        SortBy::Random => "Random",
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum Item {
    Test { field: Field, op: Op, value: String },
    Group(Match),
}

/// One row of the draft; the rules of a group follow it one level deeper.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Row {
    depth: usize,
    item: Item,
}

impl Row {
    fn blank(depth: usize) -> Row {
        Row { depth, item: Item::Test { field: Field::Artist, op: Op::Is, value: String::new() } }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct Draft {
    name: String,
    matching: Match,
    rows: Vec<Row>,
    sort: Vec<SortKey>,
    limit: Option<Limit>,
    /// The limit as typed, read on OK.
    limit_text: String,
}

fn flatten(rules: &[Rule], depth: usize, out: &mut Vec<Row>) {
    for rule in rules {
        match rule {
            Rule::Test(t) => {
                let (field, op, value) = t.parts();
                out.push(Row { depth, item: Item::Test { field, op, value } });
            }
            Rule::Group { matching, rules } => {
                out.push(Row { depth, item: Item::Group(*matching) });
                flatten(rules, depth + 1, out);
            }
        }
    }
}

/// The rules of the rows from `*at` on at `depth`, numbered for errors.
fn tree(rows: &[Row], at: &mut usize, depth: usize) -> Result<Vec<Rule>, String> {
    let mut rules = Vec::new();
    while let Some(row) = rows.get(*at).filter(|r| r.depth == depth) {
        *at += 1;
        rules.push(match &row.item {
            Item::Test { field, op, value } => Rule::Test(Test::from_text(*field, *op, value).map_err(|e| format!("Rule {}: {e}.", *at))?),
            Item::Group(matching) => Rule::Group { matching: *matching, rules: tree(rows, at, depth + 1)? },
        });
    }
    Ok(rules)
}

impl Draft {
    fn new(list: &SmartList) -> Draft {
        let mut rows = Vec::new();
        flatten(&list.rules, 0, &mut rows);
        if rows.is_empty() {
            rows.push(Row::blank(0));
        }
        let limit_text = match list.limit {
            Some(Limit::Tracks(n)) => n.to_string(),
            Some(Limit::Minutes(n)) => n.to_string(),
            None => String::new(),
        };
        Draft { name: list.name.clone(), matching: list.matching, rows, sort: list.sort.clone(), limit: list.limit, limit_text }
    }

    /// Where the rows of the group at `g` end.
    fn end_of(&self, g: usize) -> usize {
        let depth = self.rows[g].depth;
        (g + 1..self.rows.len()).find(|&i| self.rows[i].depth <= depth).unwrap_or(self.rows.len())
    }

    /// A blank rule at the end of group `g`, or of the list.
    fn add_rule(&mut self, g: Option<usize>) {
        match g {
            Some(g) => self.rows.insert(self.end_of(g), Row::blank(self.rows[g].depth + 1)),
            None => self.rows.push(Row::blank(0)),
        }
    }

    fn add_group(&mut self) {
        self.rows.push(Row { depth: 0, item: Item::Group(Match::Any) });
        self.rows.push(Row::blank(1));
    }

    /// Takes out row `i`, with the rules of a group.
    fn remove(&mut self, i: usize) {
        let end = self.end_of(i);
        self.rows.drain(i..end);
    }

    fn to_list(&self) -> Result<SmartList, String> {
        let name = self.name.trim();
        if name.is_empty() {
            return Err("The playlist needs a name.".into());
        }
        let mut at = 0;
        let rules = tree(&self.rows, &mut at, 0)?;
        let number = || self.limit_text.trim().parse().ok().filter(|&n| n > 0).ok_or("The limit needs a whole number above 0.");
        let limit = match self.limit {
            None => None,
            Some(Limit::Tracks(_)) => Some(Limit::Tracks(number()? as usize)),
            Some(Limit::Minutes(_)) => Some(Limit::Minutes(number()?)),
        };
        Ok(SmartList { name: name.to_string(), matching: self.matching, rules, sort: self.sort.clone(), limit })
    }
}

/// The controls of a row of the form.
enum RowControls {
    Test { field: usize, op: usize, value: usize },
    Group { matching: usize },
}

pub struct SmartEditor {
    pub(crate) form: Form<Act>,
    index: Option<usize>,
    draft: Draft,
    name: usize,
    matching: usize,
    rows: Vec<RowControls>,
    /// Field and direction of each level of order.
    sort: [(usize, usize); LEVELS],
    limit: (usize, usize),
    error: usize,
}

impl SmartEditor {
    pub fn new(model: &Model) -> SmartEditor {
        let editing = model.library.editing.clone().unwrap_or_else(|| Editing { index: None, list: SmartList::default() });
        let draft = Draft::new(&editing.list);
        let mut e = SmartEditor {
            form: Form::new(W, 0),
            index: editing.index,
            draft,
            name: 0,
            matching: 0,
            rows: Vec::new(),
            sort: [(0, 0); LEVELS],
            limit: (0, 0),
            error: 0,
        };
        e.build();
        e
    }

    /// Lays the form out for the draft.
    fn build(&mut self) {
        let rules_h = rules_h(self.draft.rows.len());
        let below = RULES_Y + rules_h + 10;
        let mut form = Form::new(W, below + 3 * ROW + 10 + BUTTON.1 + 10);
        form.set_view(Rect::new(10, RULES_Y, W - 20, rules_h));
        form.add(Rect::new(10, 10, 60, 24), Widget::Label { text: "Name:".into(), dim: false });
        self.name = form.add(Rect::new(70, 10, W - 80, 24), Widget::Input { text: self.draft.name.clone(), cursor: 0 });
        form.add(Rect::new(10, 44, 120, 24), Widget::Label { text: "Tracks that match".into(), dim: false });
        self.matching = form.add(Rect::new(130, 44, 70, 24), choice(&MATCHES, self.draft.matching as usize));
        form.add(Rect::new(210, 44, 200, 24), Widget::Label { text: "of these rules:".into(), dim: false });
        self.rows = (0..self.draft.rows.len()).map(|i| self.add_row(&mut form, i)).collect();
        let y = RULES_Y + 4 + self.draft.rows.len() as i32 * ROW + 4;
        form.add_on(
            0,
            Rect::new(14, y, BUTTON.0, BUTTON.1),
            Widget::Button { label: "Add rule".into(), action: Act::SmartAdd(None), default: false },
        );
        form.add_on(
            0,
            Rect::new(20 + BUTTON.0, y, BUTTON.0, BUTTON.1),
            Widget::Button { label: "Add group".into(), action: Act::SmartGroup, default: false },
        );

        let choices = sort_choices();
        for level in 0..LEVELS {
            let (x, y) = (10 + level as i32 * 320, below);
            let text = if level == 0 { "Order by" } else { "then by" };
            form.add(Rect::new(x, y, 64, 24), Widget::Label { text: text.into(), dim: false });
            let key = self.draft.sort.get(level);
            let picked = key.and_then(|k| choices.iter().position(|&b| b == k.by)).map_or(0, |n| n + 1);
            let options: Vec<&str> = std::iter::once("—").chain(choices.iter().map(|&b| sort_label(b))).collect();
            let by = form.add(Rect::new(x + 64, y, 130, 24), choice(&options, picked));
            let dir = form.add(Rect::new(x + 200, y, 100, 24), choice(&["ascending", "descending"], key.is_some_and(|k| k.desc) as usize));
            form.set_disabled(dir, key.is_some_and(|k| k.by == SortBy::Random));
            self.sort[level] = (by, dir);
        }
        let y = below + ROW + 4;
        form.add(Rect::new(10, y, 64, 24), Widget::Label { text: "Limit".into(), dim: false });
        let kind = match self.draft.limit {
            None => 0,
            Some(Limit::Tracks(_)) => 1,
            Some(Limit::Minutes(_)) => 2,
        };
        let limit_kind = form.add(Rect::new(74, y, 130, 24), choice(&LIMITS, kind));
        let limit_value = form.add(Rect::new(210, y, 90, 24), Widget::Input { text: self.draft.limit_text.clone(), cursor: 0 });
        form.set_disabled(limit_value, kind == 0);
        self.limit = (limit_kind, limit_value);
        let by = form.h - BUTTON.1 - 10;
        self.error = form.add(Rect::new(10, by - ROW - 2, W - 20, 24), Widget::Label { text: String::new(), dim: false });
        button(&mut form, W - 2 * BUTTON.0 - 16, by, "OK", Act::Ok, true);
        button(&mut form, W - BUTTON.0 - 10, by, "Cancel", Act::Close, false);
        self.form = form;
    }

    fn add_row(&self, form: &mut Form<Act>, i: usize) -> RowControls {
        let row = &self.draft.rows[i];
        let (x, y) = (14 + row.depth as i32 * INDENT, RULES_Y + 4 + i as i32 * ROW);
        let remove = Rect::new(W - 14 - 64, y, 64, 24);
        form.add_on(0, remove, Widget::Button { label: "Remove".into(), action: Act::SmartRemove(i), default: false });
        match &row.item {
            Item::Test { field, op, value } => {
                let labels: Vec<&str> = Field::ALL.iter().map(|&f| field_label(f)).collect();
                let picked = Field::ALL.iter().position(|f| f == field).unwrap_or(0);
                let field_c = form.add_on(0, Rect::new(x, y, 130, 24), choice(&labels, picked));
                let ops: Vec<&str> = field.ops().iter().map(|&o| op_label(o)).collect();
                let op_c =
                    form.add_on(0, Rect::new(x + 136, y, 140, 24), choice(&ops, field.ops().iter().position(|o| o == op).unwrap_or(0)));
                let vx = x + 282;
                let value_c = form.add_on(0, Rect::new(vx, y, remove.x - 6 - vx, 24), Widget::Input { text: value.clone(), cursor: 0 });
                form.set_disabled(value_c, *op == Op::Missing);
                RowControls::Test { field: field_c, op: op_c, value: value_c }
            }
            Item::Group(matching) => {
                let matching = form.add_on(0, Rect::new(x, y, 70, 24), choice(&MATCHES, *matching as usize));
                form.add_on(0, Rect::new(x + 76, y, 60, 24), Widget::Label { text: "of these:".into(), dim: false });
                let add = Rect::new(x + 140, y, BUTTON.0, BUTTON.1);
                form.add_on(0, add, Widget::Button { label: "Add rule".into(), action: Act::SmartAdd(Some(i)), default: false });
                RowControls::Group { matching }
            }
        }
    }

    /// Takes what the form shows into the draft. A field that changed
    /// keeps its op when the op suits it, else takes the field's first.
    fn read(&mut self) {
        let f = &self.form;
        self.draft.name = f.text(self.name).to_string();
        self.draft.matching = read_match(f, self.matching);
        for (row, controls) in self.draft.rows.iter_mut().zip(&self.rows) {
            row.item = match controls {
                RowControls::Test { field, op, value } => read_test(f, &row.item, (*field, *op, *value)),
                RowControls::Group { matching } => Item::Group(read_match(f, *matching)),
            };
        }
        self.read_order_and_limit();
    }

    fn read_order_and_limit(&mut self) {
        let f = &self.form;
        let choices = sort_choices();
        let levels = self.sort.iter().filter_map(|&(by, dir)| {
            let n = f.choice(by);
            (n > 0).then(|| SortKey { by: choices[n - 1], desc: f.choice(dir) == 1 })
        });
        let kept = self.draft.sort.iter().skip(LEVELS).copied();
        self.draft.sort = levels.collect::<Vec<_>>().into_iter().chain(kept).collect();
        self.draft.limit_text = f.text(self.limit.1).to_string();
        self.draft.limit = match f.choice(self.limit.0) {
            0 => None,
            1 => Some(Limit::Tracks(0)),
            _ => Some(Limit::Minutes(0)),
        };
    }

    /// Lays the form out again after a change of its rows or choices.
    fn change(&mut self, act: impl FnOnce(&mut Draft)) -> bool {
        self.read();
        act(&mut self.draft);
        self.build();
        true
    }

    /// Saves the list, or says what is wrong with it and stays open.
    fn save(&mut self, model: &mut Model) -> bool {
        self.read();
        match self.draft.to_list() {
            Ok(list) => {
                model.save_smart(self.index, list);
                false
            }
            Err(e) => {
                self.form.set_label(self.error, e);
                true
            }
        }
    }

    pub fn handle(&mut self, ev: Event<Act>, model: &mut Model) -> bool {
        match ev {
            Event::Edited => {
                self.read();
                true
            }
            Event::Selected => self.change(|_| {}),
            Event::Clicked(Act::SmartAdd(g)) => self.change(|d| d.add_rule(g)),
            Event::Clicked(Act::SmartGroup) => self.change(Draft::add_group),
            Event::Clicked(Act::SmartRemove(i)) => self.change(|d| d.remove(i)),
            Event::Clicked(Act::Ok) | Event::Activated => self.save(model),
            Event::Clicked(_) | Event::Cancel => false,
            _ => true,
        }
    }
}

fn read_match(f: &Form<Act>, i: usize) -> Match {
    if f.choice(i) == 0 {
        Match::All
    } else {
        Match::Any
    }
}

/// A rule row as the form shows it; `was` is the row before, whose
/// field the op's options are of.
fn read_test(f: &Form<Act>, was: &Item, (field, op, value): (usize, usize, usize)) -> Item {
    let old = match was {
        Item::Test { field, .. } => *field,
        Item::Group(_) => Field::Any,
    };
    let new = Field::ALL[f.choice(field)];
    let picked = old.ops()[f.choice(op).min(old.ops().len() - 1)];
    let op = if new.ops().contains(&picked) { picked } else { new.ops()[0] };
    Item::Test { field: new, op, value: f.text(value).to_string() }
}

fn choice(options: &[&str], selected: usize) -> Widget<Act> {
    Widget::Choice { options: options.iter().map(|o| o.to_string()).collect(), selected }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::panels::tests::{click_button, press, type_text};
    use crate::panels::{Panel, PanelKind};
    use crate::widgets::Key;
    use popamp_core::Command;

    fn editor(p: &mut Panel) -> &mut SmartEditor {
        let Panel::Smart(e) = p else { panic!("not the smart playlist editor") };
        e
    }

    /// Picks option `n` of drop-down `i`, as the mouse does.
    fn pick(p: &mut Panel, m: &mut Model, i: usize, n: usize) {
        let r = p.form().screen_rect(i);
        p.form_mut().mouse_down(r.x + 2, r.y + 2, false);
        let o = p.form().option_rect(i, n);
        let ev = p.form_mut().mouse_down(o.x + 2, o.y + 4, false);
        p.handle(ev, m);
    }

    /// Clears the field with the keyboard and types `text` into it.
    fn retype(p: &mut Panel, m: &mut Model, i: usize, text: &str) {
        let r = p.form().screen_rect(i);
        p.form_mut().mouse_down(r.x + 2, r.y + 2, false);
        while !p.form().text(i).is_empty() {
            press(p, m, Key::Backspace);
        }
        type_text(p, m, text);
    }

    fn test_controls(p: &mut Panel, row: usize) -> (usize, usize, usize) {
        match editor(p).rows[row] {
            RowControls::Test { field, op, value } => (field, op, value),
            RowControls::Group { .. } => panic!("row {row} is a group"),
        }
    }

    #[test]
    fn a_new_playlist_is_made_in_the_form_and_saved() {
        let mut m = Model::for_tests();
        m.exec(Command::LibrarySmartNew);
        let mut p = Panel::open(PanelKind::SmartPlaylist, &m);
        let name = editor(&mut p).name;
        retype(&mut p, &mut m, name, "Long rock");
        let (field, _, value) = test_controls(&mut p, 0);
        pick(&mut p, &mut m, field, Field::ALL.iter().position(|&f| f == Field::Genre).unwrap());
        let (_, op, value2) = test_controls(&mut p, 0);
        assert_eq!(value, value2, "the form was laid out again with the same controls");
        pick(&mut p, &mut m, op, 1);
        retype(&mut p, &mut m, value, "rock");
        assert!(click_button(&mut p, &mut m, Act::SmartAdd(None)));
        let (field, op, value) = test_controls(&mut p, 1);
        pick(&mut p, &mut m, field, Field::ALL.iter().position(|&f| f == Field::Length).unwrap());
        pick(&mut p, &mut m, op, 2);
        retype(&mut p, &mut m, value, "420");
        let limit = editor(&mut p).limit;
        pick(&mut p, &mut m, limit.0, 1);
        retype(&mut p, &mut m, limit.1, "50");
        assert!(!click_button(&mut p, &mut m, Act::Ok), "OK closes the editor");

        let saved = popamp_core::smart::load(&m.paths.smart_file());
        let list = saved.last().unwrap();
        assert_eq!((list.name.as_str(), list.limit), ("Long rock", Some(Limit::Tracks(50))));
        let parts: Vec<_> = list.rules.iter().map(|r| if let Rule::Test(t) = r { t.parts() } else { panic!() }).collect();
        assert_eq!(parts, [(Field::Genre, Op::Has, "rock".into()), (Field::Length, Op::Greater, "420".into())]);
        assert_eq!(m.library.smart.last(), Some(list), "the library has it too");
        assert_eq!(m.library.smart_index(), Some(m.library.smart.len() - 1), "and shows it");
    }

    #[test]
    fn the_form_grows_with_its_rules_then_scrolls_them() {
        let mut m = Model::for_tests();
        m.exec(Command::LibrarySmartNew);
        let mut p = Panel::open(PanelKind::SmartPlaylist, &m);
        let mut heights = vec![p.form().h];
        for _ in 1..MAX_ROWS + 2 {
            assert!(click_button(&mut p, &mut m, Act::SmartAdd(None)));
            heights.push(p.form().h);
        }
        assert!(heights[..MAX_ROWS].windows(2).all(|w| w[1] == w[0] + ROW), "a row each: {heights:?}");
        assert_eq!(heights[MAX_ROWS - 1], heights[MAX_ROWS + 1], "no taller after {MAX_ROWS} rows");
    }

    #[test]
    fn a_list_of_random_tracks_is_made_without_rules() {
        let mut m = Model::for_tests();
        m.exec(Command::LibrarySmartNew);
        let mut p = Panel::open(PanelKind::SmartPlaylist, &m);
        assert!(click_button(&mut p, &mut m, Act::SmartRemove(0)));
        let by = editor(&mut p).sort[0].0;
        pick(&mut p, &mut m, by, sort_choices().len());
        let limit = editor(&mut p).limit;
        pick(&mut p, &mut m, limit.0, 1);
        retype(&mut p, &mut m, limit.1, "50");
        assert!(!click_button(&mut p, &mut m, Act::Ok), "OK closes the editor");

        let list = popamp_core::smart::load(&m.paths.smart_file()).pop().unwrap();
        assert_eq!(
            (list.rules, list.sort, list.limit),
            (vec![], vec![SortKey { by: SortBy::Random, desc: false }], Some(Limit::Tracks(50)))
        );
    }

    #[test]
    fn a_wrong_rule_keeps_the_editor_open_and_says_why() {
        let mut m = Model::for_tests();
        m.exec(Command::LibrarySmartNew);
        let mut p = Panel::open(PanelKind::SmartPlaylist, &m);
        let (field, _, value) = test_controls(&mut p, 0);
        pick(&mut p, &mut m, field, Field::ALL.iter().position(|&f| f == Field::Year).unwrap());
        retype(&mut p, &mut m, value, "soon");
        let before = m.library.smart.len();
        assert!(click_button(&mut p, &mut m, Act::Ok));
        let error = editor(&mut p).error;
        let Widget::Label { text, .. } = p.form().widget(error) else { panic!() };
        assert!(text.contains("Rule 1") && text.contains("not a whole number"), "{text}");
        assert_eq!(m.library.smart.len(), before, "nothing saved");
    }

    #[test]
    fn groups_take_rules_under_them_and_go_with_them() {
        let mut d = Draft::new(&SmartList { name: "x".into(), ..SmartList::default() });
        d.add_group();
        d.add_rule(Some(1));
        d.add_rule(None);
        assert_eq!(d.rows.iter().map(|r| r.depth).collect::<Vec<_>>(), [0, 0, 1, 1, 0]);
        d.rows[0].item = Item::Test { field: Field::Title, op: Op::Has, value: "a".into() };
        for i in [2, 3, 4] {
            d.rows[i].item = Item::Test { field: Field::Artist, op: Op::Missing, value: String::new() };
        }
        let list = d.to_list().unwrap();
        assert_eq!(Draft::new(&list).rows, d.rows, "the rows come back from the list");
        assert!(matches!(&list.rules[1], Rule::Group { matching: Match::Any, rules } if rules.len() == 2));
        d.remove(1);
        assert_eq!(d.rows.len(), 2, "a group goes with its rules");
    }

    #[test]
    fn an_edited_playlist_replaces_the_old_one_and_delete_asks_first() {
        let mut m = Model::for_tests();
        m.library.smart = popamp_core::smart::load(&m.paths.smart_file());
        m.library.open_view_for_tests(1);
        m.exec(Command::LibrarySmartEdit);
        let mut p = Panel::open(PanelKind::SmartPlaylist, &m);
        let name = editor(&mut p).name;
        assert_eq!(p.form().text(name), "Last played");
        retype(&mut p, &mut m, name, "Lately");
        assert!(!click_button(&mut p, &mut m, Act::Ok));
        let names: Vec<_> = popamp_core::smart::load(&m.paths.smart_file()).into_iter().map(|l| l.name).collect();
        assert_eq!(names, ["Recently added", "Lately", "Most played", "Never played", "Random 50", "Favourites"]);

        m.exec(Command::LibrarySmartDelete);
        assert!(m.library.smart.len() == names.len(), "only asked so far");
        m.library_smart_remove(1);
        assert_eq!(popamp_core::smart::load(&m.paths.smart_file()).len(), names.len() - 1);
    }

    #[test]
    fn a_search_saves_as_a_playlist_of_what_it_finds() {
        let mut m = Model::for_tests();
        m.library.set_query_for_tests("live 77");
        m.exec(Command::LibrarySmartFromSearch);
        let p = Panel::open(PanelKind::SmartPlaylist, &m);
        let Panel::Smart(e) = &p else { panic!() };
        assert_eq!(e.draft.name, "live 77");
        assert_eq!(e.draft.rows, [Row { depth: 0, item: Item::Test { field: Field::Any, op: Op::Has, value: "live 77".into() } }]);
    }
}
