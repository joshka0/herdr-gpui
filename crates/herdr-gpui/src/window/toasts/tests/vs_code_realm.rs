//! Toasts keep to the Herdr realm: the VS Code page beside it draws above
//! GPUI, so a toast in the window's right corners would be hidden.
use super::*;
use crate::{controls::Command, sidebar::layout_tests::snapshot};
use herdr_client::protocol::ToastHerdrPosition;
use std::sync::Arc;

#[gpui::test]
fn right_corner_toasts_sit_left_of_the_vs_code_column(cx: &mut TestAppContext) {
    let (view, cx) = cx.add_window_view(|window, cx| {
        let mut view = fixture_window(window, cx);
        let mut shown = snapshot(40);
        shown.focused_workspace_id = Some("w0".into());
        view.live.snapshot = Some(Arc::new(shown));
        view
    });
    cx.simulate_resize(size(px(1400.), px(700.)));
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.command(Command::ToggleCode, window, cx);
            let mut wire = notification("Corner");
            wire.position = Some(ToastHerdrPosition::TopRight);
            view.endpoints[0]
                .toasts
                .receive([Notice::new(wire, Instant::now()).preview()]);
        });
        window.draw(cx).clear(cx);
    });
    let code = cx.debug_bounds("code").unwrap();
    let toast = cx.debug_bounds("toast-local-0").unwrap();
    assert_eq!(toast.right(), code.left() - px(12.));
    assert_eq!(toast.top(), px(72.));
}
