#![allow(clippy::unwrap_used)]
use super::*;

/// A snapshot of `boot` with `panes` open and `focused` focused.
fn at(boot: &str, panes: &[&str], focused: Option<&str>) -> ClientShellSnapshot {
    let mut snapshot: ClientShellSnapshot = serde_json::from_str(include_str!(
        "../../../../herdr-protocol/tests/fixtures/endpoint-snapshot-v1.json"
    ))
    .unwrap();
    let pane = snapshot.panes[0].clone();
    snapshot.panes = panes
        .iter()
        .map(|id| {
            let mut pane = pane.clone();
            pane.pane_id = (*id).to_owned();
            pane
        })
        .collect();
    snapshot.boot_id = boot.into();
    snapshot.focused_pane_id = focused.map(str::to_owned);
    snapshot
}

const PANES: &[&str] = &["a", "b", "c", "d"];

fn visit(history: &mut History, focused: &str) -> ClientShellSnapshot {
    let snapshot = at("boot", PANES, Some(focused));
    history.observe(Some(&snapshot));
    snapshot
}

/// Goes `step`, then reports the focus arriving as the daemon would.
fn go(history: &mut History, step: Step, snapshot: &ClientShellSnapshot) -> String {
    let pane = history.travel(step, snapshot).unwrap().to_owned();
    visit(history, &pane);
    pane
}

#[test]
fn back_and_forward_walk_the_focus_trail() {
    let mut history = History::default();
    let mut now = visit(&mut history, "a");
    assert!(!history.can(Step::Back, &now));
    for pane in ["b", "c"] {
        now = visit(&mut history, pane);
    }
    assert!(!history.can(Step::Forward, &now));
    assert_eq!(go(&mut history, Step::Back, &now), "b");
    now = at("boot", PANES, Some("b"));
    assert_eq!(go(&mut history, Step::Back, &now), "a");
    now = at("boot", PANES, Some("a"));
    assert!(!history.can(Step::Back, &now));
    assert_eq!(go(&mut history, Step::Forward, &now), "b");
    now = at("boot", PANES, Some("b"));
    assert_eq!(go(&mut history, Step::Forward, &now), "c");
}

#[test]
fn going_somewhere_new_drops_the_entries_ahead() {
    let mut history = History::default();
    for pane in ["a", "b", "c"] {
        visit(&mut history, pane);
    }
    let now = at("boot", PANES, Some("c"));
    go(&mut history, Step::Back, &now);
    let now = visit(&mut history, "d");
    assert!(!history.can(Step::Forward, &now));
    assert_eq!(go(&mut history, Step::Back, &now), "b");
}

#[test]
fn a_second_step_before_the_first_lands_continues_from_it() {
    let mut history = History::default();
    for pane in ["a", "b", "c"] {
        visit(&mut history, pane);
    }
    let now = at("boot", PANES, Some("c"));
    assert_eq!(history.travel(Step::Back, &now), Some("b"));
    assert_eq!(history.travel(Step::Back, &now), Some("a"));
    // A snapshot that changes something else keeps the travel pending.
    visit(&mut history, "c");
    visit(&mut history, "a");
    let now = at("boot", PANES, Some("a"));
    assert_eq!(history.travel(Step::Forward, &now), Some("b"));
}

#[test]
fn a_failed_travel_is_cancelled_and_other_focus_ends_it() {
    let mut history = History::default();
    for pane in ["a", "b", "c"] {
        visit(&mut history, pane);
    }
    let now = at("boot", PANES, Some("c"));
    history.travel(Step::Back, &now);
    history.cancel_travel();
    assert_eq!(history.travel(Step::Back, &now), Some("b"));
    // Focus moving elsewhere first is a new place, not the travel landing.
    let now = visit(&mut history, "d");
    assert!(!history.can(Step::Forward, &now));
    assert_eq!(go(&mut history, Step::Back, &now), "c");
}

#[test]
fn closed_and_focused_panes_are_stepped_over() {
    let mut history = History::default();
    for pane in ["a", "b", "a", "c"] {
        visit(&mut history, pane);
    }
    let closed = at("boot", &["a", "c"], Some("c"));
    assert_eq!(history.travel(Step::Back, &closed), Some("a"));
    history.cancel_travel();
    // Only the focused pane is left behind it.
    let alone = at("boot", &["c"], Some("c"));
    assert!(!history.can(Step::Back, &alone));
}

#[test]
fn the_trail_belongs_to_one_boot_and_survives_a_dropped_connection() {
    let mut history = History::default();
    visit(&mut history, "a");
    let now = visit(&mut history, "b");
    history.observe(None);
    history.observe(Some(&at("boot", PANES, None)));
    assert!(history.can(Step::Back, &now));
    // A snapshot from another boot cannot use this boot's trail.
    assert!(!history.can(Step::Back, &at("reboot", PANES, Some("b"))));
    let rebooted = at("reboot", PANES, Some("b"));
    history.observe(Some(&rebooted));
    assert!(!history.can(Step::Back, &rebooted));
}

#[test]
fn the_trail_is_bounded() {
    let mut history = History::default();
    let panes: Vec<String> = (0..LIMIT + 10).map(|i| format!("p{i}")).collect();
    for pane in &panes {
        history.observe(Some(&at("boot", &[pane.as_str()], Some(pane))));
    }
    assert_eq!(history.panes.len(), LIMIT);
    assert_eq!(history.panes.front().map(String::as_str), Some("p10"));
    assert_eq!(history.cursor, LIMIT - 1);
}
