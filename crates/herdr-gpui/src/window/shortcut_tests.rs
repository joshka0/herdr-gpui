//! Window shortcuts that follow macOS: Cmd-` cycles through the app's
//! windows in the order they opened.
use super::commands::next_window;

#[test]
fn cycling_windows_wraps_in_opening_order() {
    assert_eq!(next_window(&[1, 2, 3], 1), Some(2));
    assert_eq!(next_window(&[1, 2, 3], 3), Some(1));
    assert_eq!(next_window(&[1], 1), None, "a lone window stays");
    assert_eq!(
        next_window(&[1, 2], 9),
        None,
        "an unknown window moves none"
    );
}
