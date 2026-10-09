//! Settings > Plugins lists what hosts report, previews the rows, and edits
//! the shared layout only through explicit, ready switches.
use super::*;
use core::prelude::v1::test;
use gpui::{TestAppContext, VisualTestContext, px, size};
use herdr_client::{ConnectTarget, protocol::ClientShellSnapshot};
use herdr_settings::Edit;
use std::sync::Arc;

fn connected(live: &mut crate::state::LiveState, summary: &str, ci: Option<&str>) {
    let mut snapshot: ClientShellSnapshot = serde_json::from_str(include_str!(
        "../../../../herdr-protocol/tests/fixtures/endpoint-snapshot-v1.json"
    ))
    .unwrap();
    for agent in &mut snapshot.agents {
        agent.tokens = vec![("summary".into(), summary.into())];
        agent.state_labels = vec![("working".into(), "deep in the mines".into())];
    }
    for workspace in &mut snapshot.workspaces {
        workspace.tokens = ci
            .map(|ci| vec![("ci".into(), ci.into())])
            .unwrap_or_default();
    }
    live.snapshot = Some(Arc::new(snapshot));
    live.status = crate::state::ConnectionStatus::Connected;
}

/// The local host reports `$summary` and `$ci`; a second host reports another
/// `$summary`; a disabled host's values never appear.
fn hosts(window: &mut Window, cx: &mut Context<HerdrWindow>) -> HerdrWindow {
    let mut view = crate::sidebar::layout_tests::fixture_window(window, cx);
    connected(&mut view.live, "fix auth", Some("green"));
    let endpoint = |id: &str, enabled| {
        crate::endpoint::Endpoint::new(
            id.into(),
            id.into(),
            ConnectTarget::Socket(format!("/unused-plugins-{id}.sock").into()),
            enabled,
        )
    };
    let mut devbox = endpoint("devbox", true);
    connected(&mut devbox.live, "deploy", None);
    let mut disabled = endpoint("disabled", false);
    connected(&mut disabled.live, "hidden", Some("hidden"));
    view.endpoints.extend([devbox, disabled]);
    view
}

fn open(cx: &mut TestAppContext) -> (Entity<SettingsWindow>, &mut VisualTestContext) {
    let main = cx.add_window(hosts);
    let weak = cx.update(|cx| main.update(cx, |_, _, cx| cx.weak_entity()).unwrap());
    let (view, cx) = cx.add_window_view(|_, cx| {
        let mut view = SettingsWindow::new(weak, cx);
        view.section = Section::Plugins;
        view.config.sidebar_layout = crate::config::SidebarLayout::from_daemon_config(
            &"[ui.sidebar.agents]\nrows = [[\"agent\"], [\"$summary\"], [\"$old\"]]\n"
                .parse()
                .unwrap(),
        )
        .unwrap();
        view.plugins.edits = Some(Vec::new());
        view
    });
    cx.simulate_resize(size(px(1000.), px(900.)));
    (view, cx)
}

fn draw(cx: &mut VisualTestContext) {
    cx.update(|window, cx| crate::sidebar::layout_tests::full_draw(window, cx).clear(cx));
}

fn click(cx: &mut VisualTestContext, id: &'static str) {
    draw(cx);
    let bounds = cx.debug_bounds(id).unwrap();
    cx.simulate_click(bounds.center(), Default::default());
    cx.run_until_parked();
}

fn recorded(
    view: &Entity<SettingsWindow>,
    cx: &mut VisualTestContext,
) -> Vec<(SidebarScope, String, bool)> {
    view.read_with(cx, |view, _| {
        view.plugins
            .edits
            .iter()
            .flatten()
            .map(|edit| match edit {
                Edit::SidebarToken {
                    scope,
                    token,
                    shown,
                } => (*scope, token.clone(), *shown),
                other => panic!("unexpected edit {other:?}"),
            })
            .collect()
    })
}

#[gpui::test]
fn switches_edit_shared_rows_only_when_ready(cx: &mut TestAppContext) {
    let (view, cx) = open(cx);
    draw(cx);
    for id in [
        "plugin-value-agents-state_text",
        "plugin-value-agents-$summary",
        "plugin-value-agents-$old",
        "plugin-value-spaces-$ci",
        "plugin-preview",
    ] {
        assert!(cx.debug_bounds(id).is_some(), "{id}");
    }
    // Without shared settings every switch is inert.
    click(cx, "plugin-value-agents-$summary");
    assert!(recorded(&view, cx).is_empty());

    view.update(cx, |view, cx| {
        view.shared = Some(herdr_settings::Settings::parse_text("").unwrap());
        cx.notify();
    });
    for id in [
        "plugin-value-agents-$summary",
        "plugin-value-agents-$old",
        "plugin-value-agents-state_text",
        "plugin-value-spaces-$ci",
    ] {
        click(cx, id);
    }
    assert_eq!(
        recorded(&view, cx),
        [
            (SidebarScope::Agents, "$summary".into(), false),
            (SidebarScope::Agents, "$old".into(), false),
            (SidebarScope::Agents, "state_text".into(), true),
            (SidebarScope::Spaces, "$ci".into(), true),
        ]
    );

    // A save in flight disables them again.
    view.update(cx, |view, cx| {
        view.saving = true;
        cx.notify();
    });
    click(cx, "plugin-value-spaces-$ci");
    assert_eq!(recorded(&view, cx).len(), 4);
}

#[gpui::test]
fn search_filters_values_and_the_preview_stays(cx: &mut TestAppContext) {
    let (view, cx) = open(cx);
    view.update(cx, |view, cx| {
        view.plugins.search.update(cx, |input, cx| {
            input.set_text_selected("GREEN", cx);
        });
    });
    draw(cx);
    assert!(cx.debug_bounds("plugin-value-spaces-$ci").is_some());
    assert!(cx.debug_bounds("plugin-value-agents-$summary").is_none());
    assert!(cx.debug_bounds("plugin-value-agents-state_text").is_none());
    assert!(cx.debug_bounds("plugin-preview").is_some());
    view.update(cx, |view, cx| {
        view.plugins.search.update(cx, |input, cx| {
            input.set_text_selected("nothing like this", cx);
        });
    });
    draw(cx);
    assert!(cx.debug_bounds("plugin-value-spaces-$ci").is_none());
    assert!(cx.debug_bounds("plugin-preview").is_some());
}

#[gpui::test]
fn the_preview_sits_beside_the_list_and_wraps_below_when_narrow(cx: &mut TestAppContext) {
    let (_view, cx) = open(cx);
    draw(cx);
    let switch = cx.debug_bounds("plugin-value-agents-$summary").unwrap();
    let preview = cx.debug_bounds("plugin-preview").unwrap();
    assert!(switch.right() <= preview.left(), "{switch:?} {preview:?}");
    cx.simulate_resize(size(px(640.), px(900.)));
    draw(cx);
    let switch = cx.debug_bounds("plugin-value-agents-$summary").unwrap();
    let preview = cx.debug_bounds("plugin-preview").unwrap();
    assert!(preview.top() >= switch.bottom(), "{switch:?} {preview:?}");
    assert!(preview.right() <= px(640.), "{preview:?}");
}
