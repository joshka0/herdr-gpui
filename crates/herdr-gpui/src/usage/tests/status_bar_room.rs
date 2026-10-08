//! How much of the status bar usage takes.
use super::*;

fn named(name: &str, used: f64) -> Window {
    Window::new(Kind::Named(name.into()), used, None, None)
}

#[test]
fn a_detailed_segment_names_its_two_tightest_windows_in_report_order() {
    let windows = [
        named("a", 40.),
        named("b", 10.),
        named("c", 70.),
        named("d", 20.),
    ];
    let names = |windows: &[Window]| {
        super::super::render::bar_windows(windows)
            .map(|window| window.kind.clone())
            .collect::<Vec<_>>()
    };
    assert_eq!(
        names(&windows),
        [Kind::Named("a".into()), Kind::Named("c".into())]
    );
    // Ties keep report order, and fewer windows than room show them all.
    let even = [named("a", 5.), named("b", 5.), named("c", 5.)];
    assert_eq!(
        names(&even),
        [Kind::Named("a".into()), Kind::Named("b".into())]
    );
    assert_eq!(names(&even[..1]), [Kind::Named("a".into())]);
    assert!(names(&[]).is_empty());
}
