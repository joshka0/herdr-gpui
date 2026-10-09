//! Selecting and copying the diff's code with the pointer and keys, and
//! noting a line from its gutter.
use super::{changes, line, the, window};
use crate::review::view::Layout;
use gpui::{Modifiers, MouseButton, point, px};

fn draw(cx: &mut gpui::VisualTestContext) {
    cx.update(|window, cx| crate::sidebar::layout_tests::full_draw(window, cx).clear(cx));
}

fn clipboard(cx: &mut gpui::VisualTestContext) -> Option<String> {
    cx.update(|_, cx| cx.read_from_clipboard())
        .and_then(|item| item.text())
}

#[gpui::test]
fn dragging_over_code_copies_only_code(cx: &mut gpui::TestAppContext) {
    let (view, cx) = window(cx, None);
    cx.update(|window, cx| view.update(cx, |view, cx| view.seed_review(changes(), window, cx)));
    draw(cx);
    draw(cx);
    // From the unchanged line's code to past the end of the added line's.
    let gutter = cx.debug_bounds("review-gutter-line-0-1").unwrap();
    let from = point(gutter.right() + px(1.), gutter.center().y);
    let added = cx.debug_bounds("review-line-0-3").unwrap();
    let to = point(added.right() - px(2.), added.center().y);
    cx.simulate_mouse_down(from, MouseButton::Left, Modifiers::default());
    cx.simulate_mouse_move(to, MouseButton::Left, Modifiers::default());
    cx.simulate_mouse_up(to, MouseButton::Left, Modifiers::default());
    draw(cx);
    cx.simulate_keystrokes("cmd-c");
    assert_eq!(
        clipboard(cx).as_deref(),
        Some("fn a() {}\nfn b() {}\nfn b() { todo!() }")
    );
    // No note was started by selecting.
    view.read_with(cx, |view, _| {
        assert!(view.reviews.values().next().unwrap().draft.is_none());
    });

    // Escape clears it; Cmd-C then copies nothing new.
    cx.write_to_clipboard(gpui::ClipboardItem::new_string("kept".into()));
    cx.simulate_keystrokes("escape cmd-c");
    assert_eq!(clipboard(cx).as_deref(), Some("kept"));
}

#[gpui::test]
fn the_gutter_notes_the_line_and_clicks_select_words_and_lines(cx: &mut gpui::TestAppContext) {
    let (view, cx) = window(cx, None);
    cx.update(|window, cx| view.update(cx, |view, cx| view.seed_review(changes(), window, cx)));
    draw(cx);
    draw(cx);
    let gutter = cx.debug_bounds("review-gutter-line-0-3").unwrap();
    cx.simulate_click(gutter.center(), Modifiers::default());
    view.read_with(cx, |view, _| {
        assert_eq!(view.reviews.values().next().unwrap().draft, Some(line(3)));
    });

    // A double-click on the code selects the word under it, a triple-click
    // the line.
    draw(cx);
    let start = point(gutter.right() + px(2.), gutter.center().y);
    let click = |count: usize, cx: &mut gpui::VisualTestContext| {
        cx.simulate_event(gpui::MouseDownEvent {
            position: start,
            button: MouseButton::Left,
            modifiers: Modifiers::default(),
            click_count: count,
            first_mouse: false,
        });
        cx.simulate_event(gpui::MouseUpEvent {
            position: start,
            button: MouseButton::Left,
            modifiers: Modifiers::default(),
            click_count: count,
        });
    };
    click(2, cx);
    cx.simulate_keystrokes("cmd-c");
    assert_eq!(clipboard(cx).as_deref(), Some("fn"));
    click(3, cx);
    cx.simulate_keystrokes("cmd-c");
    assert_eq!(clipboard(cx).as_deref(), Some("fn b() { todo!() }"));
}

#[gpui::test]
fn select_all_takes_the_file_s_new_side_side_by_side(cx: &mut gpui::TestAppContext) {
    let (view, cx) = window(cx, None);
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.seed_review(changes(), window, cx);
            view.set_review_layout(the(view), Layout::Split, cx);
            let focus = view.reviews.values().next().unwrap().focus.clone();
            window.focus(&focus, cx);
        })
    });
    draw(cx);
    cx.simulate_keystrokes("cmd-a cmd-c");
    assert_eq!(
        clipboard(cx).as_deref(),
        Some("fn a() {}\nfn b() { todo!() }")
    );
}

#[gpui::test]
fn headers_copy_the_path_and_the_hunk(cx: &mut gpui::TestAppContext) {
    let (view, cx) = window(cx, None);
    cx.update(|window, cx| view.update(cx, |view, cx| view.seed_review(changes(), window, cx)));
    draw(cx);
    draw(cx);
    let path = cx.debug_bounds("review-copy-path-0").unwrap();
    cx.simulate_click(path.center(), Modifiers::default());
    assert_eq!(clipboard(cx).as_deref(), Some("src/lib.rs"));
    let hunk = cx.debug_bounds("review-copy-hunk-0-0").unwrap();
    cx.simulate_click(hunk.center(), Modifiers::default());
    assert_eq!(
        clipboard(cx).as_deref(),
        Some("@@ -1,2 +1,2 @@\n fn a() {}\n-fn b() {}\n+fn b() { todo!() }")
    );
    // Neither started a note.
    view.read_with(cx, |view, _| {
        assert!(view.reviews.values().next().unwrap().draft.is_none());
    });
}
