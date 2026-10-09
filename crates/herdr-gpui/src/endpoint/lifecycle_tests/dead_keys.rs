use super::*;
use gpui::EntityInputHandler;

/// A dead key composes like any IME: Option-E marks `´` with the caret after
/// it, the key typed next is the input method's rather than an escape code,
/// and only the composed `é` reaches the pane.
#[gpui::test]
fn a_dead_key_reaches_the_pane_only_as_the_composed_character(cx: &mut gpui::TestAppContext) {
    let (fixture, cx) = cx.add_window_view(|window, cx| {
        Fixture(cx.new(|cx| crate::sidebar::layout_tests::fixture_window(window, cx)))
    });
    let view = fixture.update(cx, |fixture, _| fixture.0.clone());
    let (endpoint, mut server) = connected_endpoint("dead-keys");
    let down = |keystroke: &str| gpui::KeyDownEvent {
        keystroke: gpui::Keystroke::parse(keystroke).unwrap(),
        is_held: false,
        prefer_character_input: false,
    };
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            prepare_mouse(view, endpoint);
            view.live.activation = Some(crate::state::SurfaceActivation {
                request: "activate-1".into(),
                boot: view.live.snapshot.as_ref().unwrap().boot_id.clone(),
                revision: Some(view.live.surface.as_ref().unwrap().projection_revision),
                failed: false,
                focus: None,
                active: true,
            });
            view.replace_and_mark_text_in_range(None, "\u{b4}", Some(1..1), window, cx);
            assert_eq!(view.marked, "\u{b4}");
            assert_eq!(
                view.selected_text_range(false, window, cx).map(|s| s.range),
                Some(1..1)
            );
            assert_eq!(view.marked_text_range(window, cx), Some(0..1));
            // The key that finishes the accent belongs to the input method.
            view.key_down(&down("e"), window, cx);
            view.replace_text_in_range(None, "\u{e9}", window, cx);
            assert!(view.marked.is_empty());
            assert_eq!(view.marked_selection, None);
            assert_eq!(view.marked_text_range(window, cx), None);
            // A clause past the text, as a stale input method might send,
            // falls back to the caret after the composition.
            view.replace_and_mark_text_in_range(None, "kan", Some(1..9), window, cx);
            assert_eq!(
                view.selected_text_range(false, window, cx).map(|s| s.range),
                Some(3..3)
            );
            view.unmark_text(window, cx);
            view.send(ClientPaneInputEvent::TextCommit("sentinel".into()), cx);
        });
    });
    for text in ["\u{e9}", "sentinel"] {
        assert_eq!(
            server.receive(),
            ClientMessage::ClientShellPaneInput {
                pane_id: "w1:p1".into(),
                events: vec![ClientPaneInputEvent::TextCommit(text.into())],
            }
        );
    }
}

/// Moving to another pane mid-composition drops it, so the next keystroke
/// cannot carry the old text into the new pane.
#[gpui::test]
fn navigating_away_drops_the_composition(cx: &mut gpui::TestAppContext) {
    let (fixture, cx) = cx.add_window_view(|window, cx| {
        Fixture(cx.new(|cx| crate::sidebar::layout_tests::fixture_window(window, cx)))
    });
    let view = fixture.update(cx, |fixture, _| fixture.0.clone());
    let (endpoint, _server) = connected_endpoint("dead-keys-navigate");
    cx.update(|_, cx| {
        view.update(cx, |view, cx| {
            prepare_mouse(view, endpoint);
            view.marked = "\u{b4}".into();
            view.marked_selection = Some(1..1);
            view.navigate(NavigationTarget::Pane("w1:p2"), cx);
            assert!(view.marked.is_empty());
            assert_eq!(view.marked_selection, None);
        });
    });
}
