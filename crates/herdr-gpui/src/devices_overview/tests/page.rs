use crate::{
    HerdrWindow,
    browser::{Location, Store},
    controls::Command,
    sidebar::layout_tests::{fixture_window, full_draw, snapshot},
    state::ConnectionStatus,
};
use gpui::{Entity, VisualTestContext};
use std::sync::Arc;

fn draw(cx: &mut VisualTestContext) {
    cx.update(|window, cx| full_draw(window, cx).clear(cx));
}

/// The fixture window, connected and showing workspace `w0`, whose snapshot
/// lists two working agents.
fn window(cx: &mut gpui::TestAppContext) -> (Entity<HerdrWindow>, &mut VisualTestContext) {
    cx.add_window_view(|window, cx| {
        let mut view = fixture_window(window, cx);
        let mut shown = snapshot(40);
        shown.focused_workspace_id = Some("w0".into());
        shown.focused_tab_id = Some("t0".into());
        view.live.snapshot = Some(Arc::new(shown));
        view.live.status = ConnectionStatus::Connected;
        view
    })
}

fn open(view: &Entity<HerdrWindow>, cx: &mut VisualTestContext) {
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.command(Command::DevicesOverview, window, cx);
        })
    });
    draw(cx);
}

fn overview_tabs(view: &Entity<HerdrWindow>, cx: &mut VisualTestContext) -> usize {
    cx.update(|_, cx| {
        let scope = crate::browser::scope(&view.read(cx).endpoints[0]);
        cx.try_global::<Store>().map_or(0, |store| {
            store
                .in_workspace(&scope, "w0")
                .filter(|tab| tab.location == Some(Location::Devices))
                .count()
        })
    })
}

#[gpui::test]
fn the_overview_opens_once_as_a_tab_and_lists_each_device(cx: &mut gpui::TestAppContext) {
    let (view, cx) = window(cx);
    draw(cx);
    assert!(cx.debug_bounds("devices-tab").is_none());

    open(&view, cx);
    assert_eq!(overview_tabs(&view, cx), 1);
    let tab = cx.debug_bounds("devices-tab").unwrap();
    for part in [
        "devices-activity",
        "devices-totals",
        "devices-table",
        "devices-add",
    ] {
        let bounds = cx.debug_bounds(part).unwrap_or_else(|| panic!("{part}"));
        assert!(tab.contains(&bounds.origin), "{part} lies in the tab");
    }
    assert!(cx.debug_bounds("devices-row-local").is_some());
    assert!(view.read_with(cx, |view, _| view.devices_overview_open()));

    // Opening it again brings back the same tab.
    open(&view, cx);
    assert_eq!(overview_tabs(&view, cx), 1);
}

#[gpui::test]
fn the_tick_records_each_connected_devices_agents(cx: &mut gpui::TestAppContext) {
    let (view, cx) = window(cx);
    open(&view, cx);
    let devices = view.read_with(cx, |view, _| view.overview_devices());
    assert_eq!(devices.len(), 1);
    assert_eq!(devices[0].link, crate::devices_overview::Link::Online);
    assert_eq!(devices[0].tally.working, 2);
    cx.update(|_, cx| view.update(cx, |view, cx| view.poll_devices_overview(cx)));
    let lane = view.read_with(cx, |view, _| view.devices_overview.history.lane("local"));
    assert_eq!(lane.last().copied().flatten().map(|c| c.working), Some(2));
}

#[gpui::test]
fn view_all_devices_opens_a_lane_per_device(cx: &mut gpui::TestAppContext) {
    let (view, cx) = window(cx);
    open(&view, cx);
    let closed = cx.debug_bounds("devices-activity").unwrap();
    let toggle = cx.debug_bounds("devices-lanes-toggle").unwrap();
    cx.simulate_click(toggle.center(), gpui::Modifiers::default());
    draw(cx);
    let open = cx.debug_bounds("devices-activity").unwrap();
    assert!(open.size.height > closed.size.height, "the lanes take room");
    let id = view.read_with(cx, |view, _| {
        *view.devices_overview.pages.keys().next().unwrap()
    });
    assert!(view.read_with(cx, |view, _| view.devices_overview.pages[&id].lanes));
}

#[gpui::test]
fn closing_the_tab_drops_its_page(cx: &mut gpui::TestAppContext) {
    let (view, cx) = window(cx);
    open(&view, cx);
    let id = view.read_with(cx, |view, _| {
        *view.devices_overview.pages.keys().next().unwrap()
    });
    cx.update(|_, cx| Store::update(cx, |store| store.close(id)));
    cx.update(|_, cx| view.update(cx, |view, cx| view.poll_devices_overview(cx)));
    assert!(!view.read_with(cx, |view, _| view.devices_overview_open()));
}

#[gpui::test]
fn a_narrow_tab_wraps_the_load_columns_instead_of_cutting_them(cx: &mut gpui::TestAppContext) {
    let (view, cx) = window(cx);
    cx.simulate_resize(gpui::size(gpui::px(1400.), gpui::px(900.)));
    open(&view, cx);
    let wide_row = cx.debug_bounds("devices-row-local").unwrap();
    let wide_totals = cx.debug_bounds("devices-totals").unwrap();
    let wide_load = cx.debug_bounds("devices-load-local").unwrap();
    assert!(
        wide_load.top() < wide_row.top() + (wide_row.size.height / 2.),
        "one line when there is room"
    );

    cx.simulate_resize(gpui::size(gpui::px(820.), gpui::px(900.)));
    draw(cx);
    let tab = cx.debug_bounds("devices-tab").unwrap();
    let row = cx.debug_bounds("devices-row-local").unwrap();
    let load = cx.debug_bounds("devices-load-local").unwrap();
    let header = cx.debug_bounds("devices-load-header").unwrap();
    assert!(load.right() <= tab.right(), "{load:?} inside {tab:?}");
    assert!(row.size.height > wide_row.size.height, "the load wrapped");
    let totals = cx.debug_bounds("devices-totals").unwrap();
    assert_eq!(
        totals.size.height, wide_totals.size.height,
        "the four totals keep one row"
    );
    assert!(
        (header.left() - load.left()).abs() < gpui::px(1.),
        "the header wraps in step: {header:?} {load:?}"
    );
}
