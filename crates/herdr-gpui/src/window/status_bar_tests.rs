//! The status bar at the window's foot: Toggle Status Bar hides and restores
//! it for the session, and `status_bar` in the config chooses where it starts.

#![allow(clippy::unwrap_used)]

use super::HerdrWindow;
use crate::{
    controls::Command,
    sidebar::layout_tests::{fixture_window, full_draw},
};
use gpui::TestAppContext;

fn run(view: &gpui::Entity<HerdrWindow>, command: Command, cx: &mut gpui::VisualTestContext) {
    cx.update(|window, cx| view.update(cx, |view, cx| view.command(command, window, cx)));
}

fn drawn(cx: &mut gpui::VisualTestContext) -> bool {
    cx.update(|window, cx| full_draw(window, cx).clear(cx));
    cx.debug_bounds("connection-status").is_some()
}

#[gpui::test]
fn toggle_hides_and_restores_the_status_bar(cx: &mut TestAppContext) {
    let (view, cx) = cx.add_window_view(fixture_window);
    assert!(drawn(cx));

    run(&view, Command::ToggleStatusBar, cx);
    assert!(!view.read_with(cx, |view, _| view.status_bar_visible));
    assert!(!drawn(cx));
    // The body takes the rows the bar gave up.
    assert!(cx.debug_bounds("window-body").is_some());

    run(&view, Command::ToggleStatusBar, cx);
    assert!(view.read_with(cx, |view, _| view.status_bar_visible));
    assert!(drawn(cx));
}

#[gpui::test]
fn a_window_starts_with_the_configured_status_bar(cx: &mut TestAppContext) {
    let (view, cx) = cx.add_window_view(|window, cx| {
        let mut view = fixture_window(window, cx);
        view.config.status_bar = false;
        view.status_bar_visible = view.config.status_bar;
        view
    });
    assert!(!drawn(cx));
    run(&view, Command::ToggleStatusBar, cx);
    assert!(drawn(cx));
}
