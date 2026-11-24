//! Title-bar shortcuts: what a right or middle click, or the mouse wheel
//! turned while the pointer is over the title bar (the part that drags the
//! window), of the main, equalizer or playlist window does, whether the
//! window is full-size or shaded. Chosen under
//! Preferences > Shortcuts, kept in the `title_shortcuts` setting as
//! `window:button:action` rules joined by `;`, for example
//! `eq:middle:pause;main:right:next`.

use popamp_core::Command;
use popamp_skin::layout::WindowKey;

/// The windows that have shortcuts.
pub const TARGETS: [(WindowKey, &str, &str); 3] = [
    (WindowKey::Main, "main", tr_const!("Main")),
    (WindowKey::Eq, "eq", tr_const!("Equalizer")),
    (WindowKey::Playlist, "playlist", tr_const!("Playlist")),
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Click {
    Right,
    Middle,
    /// The wheel turned away from the user, once for each notch.
    ScrollUp,
    ScrollDown,
}

/// The mouse buttons and wheel turns that have shortcuts.
pub const CLICKS: [(Click, &str, &str); 4] = [
    (Click::Right, "right", tr_const!("Right Mouse")),
    (Click::Middle, "middle", tr_const!("Middle Mouse")),
    (Click::ScrollUp, "scrollup", tr_const!("Scroll Up")),
    (Click::ScrollDown, "scrolldown", tr_const!("Scroll Down")),
];

/// What a click can do.
pub const ACTIONS: [(Command, &str, &str); 8] = [
    (Command::Previous, "previous", tr_const!("Previous track")),
    (Command::Next, "next", tr_const!("Next track")),
    (Command::Pause, "pause", tr_const!("Pause")),
    (Command::Stop, "stop", tr_const!("Stop")),
    (Command::ToggleShuffle, "shuffle", tr_const!("Shuffle on/off")),
    (Command::ToggleRepeat, "repeat", tr_const!("Repeat on/off")),
    (Command::VolumeUp, "volup", tr_const!("Vol Up")),
    (Command::VolumeDown, "voldown", tr_const!("Vol Down")),
];

/// One rule: a click on a window's title bar runs a command.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Rule {
    pub window: WindowKey,
    pub click: Click,
    pub command: Command,
}

impl Rule {
    /// Whether this rule and `other` are for the same click on the same window.
    fn same_click(&self, other: &Rule) -> bool {
        self.window == other.window && self.click == other.click
    }
}

/// The rules in `spec`, one per click of a window: what does not read as a
/// rule is left out, and of two rules for the same click the first counts.
pub fn parse(spec: &str) -> Vec<Rule> {
    let mut rules: Vec<Rule> = Vec::new();
    for rule in spec.split(';').filter_map(parse_rule) {
        if !rules.iter().any(|r| r.same_click(&rule)) {
            rules.push(rule);
        }
    }
    rules
}

fn parse_rule(rule: &str) -> Option<Rule> {
    let mut parts = rule.split(':').map(str::trim);
    let (w, c, a) = (parts.next()?, parts.next()?, parts.next()?);
    if parts.next().is_some() {
        return None;
    }
    Some(Rule {
        window: TARGETS.iter().find(|t| t.1 == w)?.0,
        click: CLICKS.iter().find(|t| t.1 == c)?.0,
        command: ACTIONS.iter().find(|t| t.1 == a)?.0,
    })
}

/// Puts `rule` in `rules`: in place of the rule for the same click on the
/// same window, else at the end.
pub fn assign(rules: &mut Vec<Rule>, rule: Rule) {
    match rules.iter_mut().find(|r| r.same_click(&rule)) {
        Some(old) => *old = rule,
        None => rules.push(rule),
    }
}

/// `rule` as a row of the table in Preferences: window, click and action,
/// split by tabs into columns.
pub fn row(rule: &Rule) -> String {
    let tr = crate::i18n::tr;
    let w = TARGETS.iter().find(|t| t.0 == rule.window).map_or("", |t| tr(t.2));
    let c = CLICKS.iter().find(|t| t.0 == rule.click).map_or("", |t| tr(t.2));
    let a = ACTIONS.iter().find(|t| t.0 == rule.command).map_or("", |t| tr(t.2));
    format!("{w}\t{c}\t{a}")
}

/// The setting that holds `rules`.
pub fn format(rules: &[Rule]) -> String {
    let name = |w: WindowKey, c: Click, a: Command| {
        let w = TARGETS.iter().find(|t| t.0 == w).map_or("", |t| t.1);
        let c = CLICKS.iter().find(|t| t.0 == c).map_or("", |t| t.1);
        let a = ACTIONS.iter().find(|t| t.0 == a).map_or("", |t| t.1);
        format!("{w}:{c}:{a}")
    };
    rules.iter().map(|r| name(r.window, r.click, r.command)).collect::<Vec<_>>().join(";")
}

/// What a click on the title bar of window `window` runs under `spec`;
/// of two rules for the same click, the first counts.
pub fn find(spec: &str, window: WindowKey, click: Click) -> Option<Command> {
    parse(spec).into_iter().find(|r| r.window == window && r.click == click).map(|r| r.command)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_rule_is_found_by_window_and_button() {
        let spec = "eq:middle:pause;main:right:next";
        assert_eq!(find(spec, WindowKey::Eq, Click::Middle), Some(Command::Pause));
        assert_eq!(find(spec, WindowKey::Main, Click::Right), Some(Command::Next));
        assert_eq!(find(spec, WindowKey::Eq, Click::Right), None);
        assert_eq!(find(spec, WindowKey::Playlist, Click::Middle), None);
        assert_eq!(find("", WindowKey::Main, Click::Right), None);
    }

    #[test]
    fn the_wheel_has_rules_too() {
        let spec = "eq:scrollup:volup;eq:scrolldown:voldown";
        assert_eq!(find(spec, WindowKey::Eq, Click::ScrollUp), Some(Command::VolumeUp));
        assert_eq!(find(spec, WindowKey::Eq, Click::ScrollDown), Some(Command::VolumeDown));
        assert_eq!(find(spec, WindowKey::Main, Click::ScrollUp), None);
    }

    #[test]
    fn the_first_of_two_rules_for_a_click_counts() {
        assert_eq!(find("main:right:stop;main:right:next", WindowKey::Main, Click::Right), Some(Command::Stop));
    }

    #[test]
    fn a_second_rule_for_the_same_click_is_dropped_when_read() {
        let rules = parse("main:right:stop;eq:middle:pause;main:right:next");
        assert_eq!(format(&rules), "main:right:stop;eq:middle:pause");
    }

    #[test]
    fn assigning_a_click_again_changes_its_action_in_place() {
        let mut rules = parse("main:right:next;eq:middle:pause");
        assign(&mut rules, Rule { window: WindowKey::Main, click: Click::Right, command: Command::Stop });
        assert_eq!(format(&rules), "main:right:stop;eq:middle:pause");
        assign(&mut rules, Rule { window: WindowKey::Playlist, click: Click::ScrollUp, command: Command::VolumeUp });
        assert_eq!(format(&rules), "main:right:stop;eq:middle:pause;playlist:scrollup:volup");
    }

    #[test]
    fn a_rule_reads_as_a_table_row() {
        let rule = Rule { window: WindowKey::Eq, click: Click::Middle, command: Command::Pause };
        assert_eq!(row(&rule), "Equalizer\tMiddle Mouse\tPause");
    }

    #[test]
    fn rules_survive_being_written_and_read() {
        // Every window and click once, every action somewhere.
        let rules: Vec<Rule> = TARGETS
            .iter()
            .flat_map(|w| CLICKS.iter().map(move |c| (w.0, c.0)))
            .zip(ACTIONS.iter().cycle())
            .map(|((window, click), a)| Rule { window, click, command: a.0 })
            .collect();
        assert_eq!(parse(&format(&rules)), rules);
    }

    #[test]
    fn what_is_not_a_rule_is_left_out() {
        let spec = "nonsense; main:right:fly ; main:left:next; main:right:next:extra; Eq:middle:pause; playlist : middle : repeat";
        assert_eq!(parse(spec), [Rule { window: WindowKey::Playlist, click: Click::Middle, command: Command::ToggleRepeat }]);
    }
}
