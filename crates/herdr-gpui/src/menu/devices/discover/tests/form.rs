use super::*;
use crate::{
    endpoint::Endpoint,
    sidebar::layout_tests::{fixture_window, full_draw},
};
use gpui::{Entity, Modifiers, VisualTestContext, px, size};
use herdr_client::ConnectTarget;
use std::sync::mpsc::SyncSender;

/// Tailscale and Bonjour both see `m4max`; Tailscale also sees a device that
/// is already saved; Bonjour fails after reporting.
fn report(source: Source, sender: SyncSender<Event>) {
    let found: Vec<Candidate> = match source {
        Source::SshConfig => Vec::new(),
        Source::Tailscale => vec![
            candidate(
                Source::Tailscale,
                "m4max",
                "m4max.tail1.ts.net",
                &["m4max.tail1.ts.net", "100.84.59.116"],
            ),
            candidate(
                Source::Tailscale,
                "saved",
                "saved.tail1.ts.net",
                &["saved.tail1.ts.net"],
            ),
        ],
        Source::Bonjour => vec![
            candidate(Source::Bonjour, "m4max", "m4max.local", &["m4max.local"]),
            candidate(
                Source::Bonjour,
                "nas",
                "ssh://nas.local:2222",
                &["nas.local"],
            ),
        ],
    };
    for candidate in found {
        let _ = sender.send(Event::Found(candidate));
    }
    let result = match source {
        Source::Bonjour => Err(crate::Error::LocalAddresses(std::io::Error::other(
            "no interfaces",
        ))),
        _ => Ok(()),
    };
    let _ = sender.send(Event::Done(source, result));
}

/// A source that never reports, so the search stays in progress.
fn silent(_: Source, sender: SyncSender<Event>) {
    std::mem::forget(sender);
}

fn open(
    cx: &mut gpui::TestAppContext,
    run: Runner,
) -> (Entity<HerdrWindow>, &mut VisualTestContext) {
    let (view, cx) = cx.add_window_view(fixture_window);
    cx.simulate_resize(size(px(800.), px(700.)));
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.endpoints.push(Endpoint::new(
                "ssh:saved".into(),
                "Saved".into(),
                ConnectTarget::Ssh {
                    target: "me@saved".into(),
                    session: "default".into(),
                },
                true,
            ));
            view.open_add_device(window, cx);
            view.start_device_discovery_with(run, cx);
        });
    });
    (view, cx)
}

fn settle(cx: &mut VisualTestContext) {
    cx.run_until_parked();
    cx.executor().advance_clock(POLL * 2);
    cx.run_until_parked();
    cx.update(|window, cx| full_draw(window, cx).clear(cx));
}

fn discovery(view: &Entity<HerdrWindow>, cx: &mut VisualTestContext) -> (bool, Vec<String>) {
    cx.update(|_, cx| {
        let view = view.read(cx);
        let discovery = view
            .menu
            .device_setup
            .as_ref()
            .and_then(|setup| setup.discovery.as_ref())
            .unwrap();
        (
            discovery.searching(),
            discovery
                .suggestions
                .iter()
                .map(|s| s.target.clone())
                .collect(),
        )
    })
}

fn fields(view: &Entity<HerdrWindow>, cx: &mut VisualTestContext) -> (String, String) {
    cx.update(|_, cx| {
        let setup = view.read(cx).menu.device_setup.as_ref().unwrap();
        (
            setup.fields[0].read(cx).text().to_owned(),
            setup.fields[1].read(cx).text().to_owned(),
        )
    })
}

#[gpui::test]
fn opening_the_form_searches_until_every_source_is_done(cx: &mut gpui::TestAppContext) {
    let (view, cx) = open(cx, silent);
    cx.update(|window, cx| full_draw(window, cx).clear(cx));
    assert!(discovery(&view, cx).0);
    assert!(cx.debug_bounds("device-discovery-progress").is_some());
    assert!(cx.debug_bounds("device-discovery-again").is_none());

    // The quiet runner unit tests use finishes at once with nothing found.
    cx.update(|_, cx| view.update(cx, |view, cx| view.start_device_discovery(cx)));
    settle(cx);
    assert_eq!(discovery(&view, cx), (false, Vec::new()));
    assert!(cx.debug_bounds("device-discovery-progress").is_none());
    assert!(cx.debug_bounds("device-discovery-again").is_some());
}

#[gpui::test]
fn suggestions_merge_hide_saved_devices_and_fill_the_form(cx: &mut gpui::TestAppContext) {
    let (view, cx) = open(cx, report);
    settle(cx);
    let (searching, targets) = discovery(&view, cx);
    assert!(!searching);
    assert_eq!(
        targets,
        [
            "m4max.tail1.ts.net",
            "ssh://nas.local:2222",
            "saved.tail1.ts.net"
        ]
    );
    // `saved` is in the catalog, so only two rows show.
    assert!(cx.debug_bounds("device-suggestion-1").is_some());
    assert!(cx.debug_bounds("device-suggestion-2").is_none());
    cx.update(|_, cx| {
        let view = view.read(cx);
        let discovery = view.menu.device_setup.as_ref().unwrap().discovery.as_ref();
        let failures = &discovery.unwrap().failures;
        assert!(matches!(
            failures.as_slice(),
            [(Source::Bonjour, crate::Error::LocalAddresses(_))]
        ));
    });

    let row = cx.debug_bounds("device-suggestion-0").unwrap().center();
    cx.simulate_click(row, Modifiers::default());
    assert_eq!(
        fields(&view, cx),
        ("m4max.tail1.ts.net".into(), "m4max".into())
    );
    // A label an earlier suggestion filled in follows the next choice.
    cx.update(|window, cx| full_draw(window, cx).clear(cx));
    let row = cx.debug_bounds("device-suggestion-1").unwrap().center();
    cx.simulate_click(row, Modifiers::default());
    assert_eq!(
        fields(&view, cx),
        ("ssh://nas.local:2222".into(), "nas".into())
    );
    // One the user typed stays.
    cx.update(|_, cx| {
        let setup = view.read(cx).menu.device_setup.as_ref().unwrap();
        let label = setup.fields[1].clone();
        label.update(cx, |field, cx| field.set_text_selected("My box", cx));
    });
    cx.update(|window, cx| full_draw(window, cx).clear(cx));
    let row = cx.debug_bounds("device-suggestion-0").unwrap().center();
    cx.simulate_click(row, Modifiers::default());
    assert_eq!(
        fields(&view, cx),
        ("m4max.tail1.ts.net".into(), "My box".into())
    );
}

#[gpui::test]
fn closing_the_form_ends_the_search(cx: &mut gpui::TestAppContext) {
    let (view, cx) = open(cx, silent);
    cx.update(|window, cx| view.update(cx, |view, cx| view.dismiss_menu(window, cx)));
    settle(cx);
    cx.update(|_, cx| assert!(view.read(cx).menu.device_setup.is_none()));
}
