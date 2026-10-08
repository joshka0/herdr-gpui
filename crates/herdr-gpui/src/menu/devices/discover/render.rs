//! The Nearby devices list under the SSH target field: a sliding bar while
//! sources search, then what they found, minus devices already saved.

use super::{Discovery, Source, Suggestion};
use crate::{
    HerdrWindow,
    progress::{self, Progress},
};
use gpui::{prelude::*, *};

impl HerdrWindow {
    pub(in crate::menu::devices) fn render_device_suggestions(
        &self,
        discovery: &Discovery,
        cx: &mut Context<Self>,
    ) -> Div {
        let theme = &self.theme;
        let visible: Vec<&Suggestion> = discovery
            .suggestions
            .iter()
            .filter(|suggestion| {
                !self.endpoints.iter().any(|endpoint| {
                    endpoint
                        .saved_ssh()
                        .is_some_and(|(target, _)| suggestion.saved_as(target))
                })
            })
            .collect();
        let status = if discovery.searching() {
            format!("Searching {}…", sources(&discovery.pending))
        } else {
            match visible.len() {
                0 => "Search finished. No devices found.".into(),
                1 => "Search finished. Found 1 device.".into(),
                count => format!("Search finished. Found {count} devices."),
            }
        };
        let mut header = div()
            .flex()
            .items_center()
            .gap(px(8.))
            .child(div().flex_none().child("Nearby devices"))
            .child(
                div()
                    .debug_selector(|| "device-discovery-status".into())
                    .flex_1()
                    .min_w_0()
                    .truncate()
                    .text_color(rgb(theme.muted))
                    .child(status),
            );
        if !discovery.searching() {
            header = header.child(
                div()
                    .id("device-discovery-again")
                    .debug_selector(|| "device-discovery-again".into())
                    .flex_none()
                    .px_2()
                    .py_1()
                    .cursor_pointer()
                    .rounded(px(crate::config::corners::CONTROL))
                    .hover(|s| s.bg(rgb(theme.active)))
                    .child("Search again")
                    .on_click(cx.listener(|this, _, _, cx| this.start_device_discovery(cx))),
            );
        }
        let mut view = div()
            .debug_selector(|| "device-discovery".into())
            .flex_none()
            .flex()
            .flex_col()
            .gap(px(6.))
            .child(header);
        if discovery.searching() {
            view = view.child(progress::bar(
                "device-discovery-progress",
                Progress::Busy,
                crate::menu::accent(theme).into(),
                rgb(theme.active).into(),
            ));
        }
        for (index, suggestion) in visible.into_iter().enumerate() {
            let target = suggestion.target.clone();
            let name = suggestion.name.clone();
            view = view.child(
                div()
                    .id(("device-suggestion", index))
                    .debug_selector(move || format!("device-suggestion-{index}"))
                    .flex()
                    .items_center()
                    .gap(px(8.))
                    .px(px(8.))
                    .py(px(6.))
                    .rounded(px(crate::config::corners::CONTROL))
                    .cursor_pointer()
                    .hover(|s| s.bg(rgb(theme.active)))
                    .child(
                        div()
                            .flex_none()
                            .max_w(px(220.))
                            .truncate()
                            .child(name.clone()),
                    )
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .truncate()
                            .text_color(rgb(theme.subtext()))
                            .child(suggestion.target.clone()),
                    )
                    .child(
                        div()
                            .flex_none()
                            .text_color(rgb(theme.muted))
                            .child(sources(&suggestion.sources)),
                    )
                    .on_click(cx.listener(move |this, _, window, cx| {
                        this.use_device_suggestion(&target, &name, window, cx);
                    })),
            );
        }
        for (source, error) in &discovery.failures {
            view = view.child(
                div()
                    .text_color(rgb(theme.muted))
                    .child(format!("{}: {error}", source.name())),
            );
        }
        view
    }

    /// Fill the form from a suggestion. A label the user typed is kept; one an
    /// earlier suggestion filled in, and not edited since, is replaced.
    pub(in crate::menu::devices) fn use_device_suggestion(
        &mut self,
        target: &str,
        name: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(setup) = &mut self.menu.device_setup else {
            return;
        };
        if !matches!(setup.step, super::super::add_device::Step::Form) {
            return;
        }
        let [target_field, label_field, _] = setup.fields.clone();
        target_field.update(cx, |field, cx| field.set_text_selected(target, cx));
        let label = label_field.read(cx).text().trim();
        if label.is_empty() || setup.suggested_label.as_deref() == Some(label) {
            // Recorded first, so the field's change event sees its own fill.
            setup.suggested_label = Some(name.to_owned());
            label_field.update(cx, |field, cx| field.set_text_selected(name, cx));
        }
        window.focus(&target_field.read(cx).focus.clone(), cx);
        self.menu.error = None;
        cx.notify();
    }
}

/// `Bonjour`, `Tailscale and Bonjour`, `SSH config, Tailscale, and Bonjour`.
pub(super) fn sources(sources: &[Source]) -> String {
    let names: Vec<&str> = sources.iter().map(|source| source.name()).collect();
    match names.as_slice() {
        [] => String::new(),
        [one] => (*one).to_owned(),
        [first, second] => format!("{first} and {second}"),
        [rest @ .., last] => format!("{}, and {last}", rest.join(", ")),
    }
}
