//! The device table: one row per device with its agents, CPU, memory, home
//! disk and uptime. Clicking a row shows that device in the window.

use super::{Device, Link, view::Look};
use crate::{
    HerdrWindow,
    browser::Slot,
    system_load::{self, CPU_WARN, DISK_WARN, MEMORY_WARN, SystemLoad},
};
use gpui::{prelude::*, *};
use herdr_client::protocol::AgentStatus;

const DEVICE_MIN: f32 = 180.;
/// Every column but the device's, which takes the rest.
const COLUMNS: [(&str, f32); 6] = [
    ("Agents", 230.),
    ("CPU", 84.),
    ("Memory", 84.),
    ("Disk free", 96.),
    ("Uptime", 76.),
    ("", 12.),
];
const METER_WIDTH: f32 = 64.;

pub(super) fn devices(
    look: &Look,
    shown: &[&Device],
    load: &SystemLoad,
    slot: Slot,
    cx: &mut Context<HerdrWindow>,
) -> Div {
    let theme = look.theme;
    let header = div()
        .flex()
        .items_center()
        .gap_3()
        .px_3()
        .py_2()
        .border_b_1()
        .border_color(rgb(theme.active))
        .text_size(look.small())
        .text_color(rgb(theme.subtext()))
        .child(div().flex_1().min_w(px(DEVICE_MIN)).child("Device"))
        .children(
            COLUMNS
                .iter()
                .map(|(name, width)| div().flex_none().w(px(*width)).child(*name)),
        );
    let rows = shown.iter().enumerate().map(|(position, device)| {
        let reading = device
            .host
            .as_ref()
            .filter(|_| device.link == Link::Online)
            .and_then(|host| load.get(host));
        let sample = reading.and_then(system_load::Reading::latest);
        let id = device.id.clone();
        div()
            .id(SharedString::from(
                slot.selector(&format!("devices-row-{}", device.id)),
            ))
            .debug_selector({
                let id = device.id.clone();
                move || slot.selector(&format!("devices-row-{id}"))
            })
            .flex()
            .items_center()
            .gap_3()
            .px_3()
            .py_2()
            .cursor_pointer()
            .hover(|style| style.bg(rgb(theme.active)))
            .when(position > 0, |row| {
                row.border_t_1().border_color(rgb(theme.active))
            })
            .on_click(cx.listener(move |this, _, window, cx| {
                this.select_endpoint(&id, cx);
                window.focus(&this.focus, cx);
            }))
            .child(
                div()
                    .flex_1()
                    .min_w(px(DEVICE_MIN))
                    .child(name(look, device)),
            )
            .child(
                div()
                    .flex_none()
                    .w(px(COLUMNS[0].1))
                    .child(agents(look, device)),
            )
            .child(div().flex_none().w(px(COLUMNS[1].1)).child(meter(
                look,
                None,
                sample.and_then(|sample| sample.cpu),
                CPU_WARN,
            )))
            .child(div().flex_none().w(px(COLUMNS[2].1)).child(meter(
                look,
                None,
                sample.and_then(|sample| sample.memory.map(|memory| memory.percent())),
                MEMORY_WARN,
            )))
            .child(div().flex_none().w(px(COLUMNS[3].1)).child({
                let disk = sample.and_then(|sample| sample.disk);
                meter(
                    look,
                    disk.map(|disk| system_load::storage(disk.available)),
                    disk.map(|disk| disk.used_percent()),
                    DISK_WARN,
                )
            }))
            .child(
                div().flex_none().w(px(COLUMNS[4].1)).child(
                    look.mono(
                        sample
                            .and_then(|sample| sample.uptime)
                            .map_or_else(|| "—".to_owned(), system_load::uptime),
                    ),
                ),
            )
            .child(
                svg()
                    .flex_none()
                    .size(px(COLUMNS[5].1))
                    .path("icons/chevron-right.svg")
                    .text_color(rgb(theme.muted)),
            )
    });
    look.panel()
        .debug_selector(move || slot.selector("devices-table"))
        .bg(rgb(theme.background))
        .flex()
        .flex_col()
        .child(header)
        .children(rows)
        .when(shown.is_empty(), |table| {
            table.child(
                div()
                    .p_4()
                    .text_color(rgb(theme.muted))
                    .child("No device, agent or workspace matches the search."),
            )
        })
}

fn name(look: &Look, device: &Device) -> Div {
    div()
        .flex()
        .items_center()
        .gap_2()
        .min_w_0()
        .child(look.link(device.link))
        .child(
            div()
                .flex()
                .flex_col()
                .min_w_0()
                .child(
                    div()
                        .truncate()
                        .font_weight(FontWeight::MEDIUM)
                        .child(device.label.clone()),
                )
                .child(look.mono(device.address.clone()).truncate()),
        )
}

/// A dot per agent, then `2/5 working`, then how many wait for input.
fn agents(look: &Look, device: &Device) -> Div {
    let theme = look.theme;
    let tally = device.tally;
    let text = match device.link {
        Link::Offline => "Offline".to_owned(),
        Link::Connecting => "Connecting".to_owned(),
        Link::Disabled => "Disabled".to_owned(),
        Link::Online if tally.total() == 0 => "No agents".to_owned(),
        Link::Online => format!("{}/{} working", tally.working, tally.total()),
    };
    let blocked = look.status(AgentStatus::Blocked);
    div()
        .flex()
        .items_center()
        .gap_2()
        .min_w_0()
        .child(
            div()
                .flex()
                .flex_none()
                .items_center()
                .gap(px(3.))
                .children(device.dots.iter().map(|status| look.dot(*status))),
        )
        .child(look.mono(text).min_w_0().truncate())
        .when(tally.blocked > 0, |row| {
            row.child(
                div()
                    .flex_none()
                    .px(px(5.))
                    .rounded(px(crate::config::corners::SMALL))
                    .bg(rgb(crate::config::mix(theme.background, blocked, 18)))
                    .text_color(rgb(blocked))
                    .text_size(look.small())
                    .child(format!("{} blocked", tally.blocked)),
            )
        })
}

/// A share over a thin bar, or a dash before the first sample.
fn meter(look: &Look, label: Option<String>, percent: Option<f32>, warn: f32) -> Div {
    let theme = look.theme;
    let Some(percent) = percent else {
        return look.mono("—");
    };
    let fill = system_load::severity(
        percent,
        warn,
        theme,
        crate::config::mix(theme.background, theme.foreground, 70),
    );
    div()
        .flex()
        .flex_col()
        .gap(px(3.))
        .child(
            look.mono(label.unwrap_or_else(|| format!("{percent:.0}%")))
                .text_color(rgb(theme.foreground)),
        )
        .child(
            div()
                .w(px(METER_WIDTH))
                .h(px(3.))
                .rounded_full()
                .bg(rgb(theme.active))
                .child(
                    div()
                        .h_full()
                        .rounded_full()
                        .w(px(METER_WIDTH * percent.clamp(0., 100.) / 100.))
                        .bg(rgb(fill)),
                ),
        )
}
