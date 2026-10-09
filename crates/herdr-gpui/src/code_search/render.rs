//! The picker paints ranked rows; ranking and input live elsewhere.

use super::{CodeSearch, Mode, Status, rank};
use crate::HerdrWindow;
use gpui::{prelude::*, *};

impl HerdrWindow {
    fn code_search_header(&self, code: &CodeSearch, cx: &mut Context<Self>) -> Div {
        let theme = &self.theme;
        let counted = code.index.as_ref().map(|index| match code.mode {
            Mode::Symbols => index.symbols().len(),
            Mode::Files => index.files().len(),
        });
        let summary = match (&code.status, counted) {
            (Status::Failed(_), _) => String::new(),
            (Status::Locating, _) | (Status::Indexing, None) => "Reading the checkout...".into(),
            (_, Some(count)) => {
                let shown = code.hits.len();
                let more = if shown == rank::MAX_HITS { "+" } else { "" };
                let partial = code
                    .index
                    .as_ref()
                    .filter(|index| index.truncated())
                    .map_or("", |_| " (checkout too large, some left out)");
                format!("{shown}{more} of {count}{partial}")
            }
            (Status::Ready, None) => String::new(),
        };
        let root = code
            .index
            .as_ref()
            .map(|index| index.root().display().to_string())
            .unwrap_or_default();
        div()
            .flex_none()
            .p(px(16.))
            .border_b_1()
            .border_color(rgb(theme.active))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(12.))
                    .child(
                        div()
                            .flex_none()
                            .text_size(px(self.config.ui.size * 1.35))
                            .font_weight(FontWeight::SEMIBOLD)
                            .child(match code.mode {
                                Mode::Symbols => "Go to Symbol",
                                Mode::Files => "Go to File",
                            }),
                    )
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .truncate()
                            .text_color(rgb(theme.muted))
                            .child(root),
                    ),
            )
            .child(div().pt(px(12.)).child(code.search.clone()))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(6.))
                    .pt(px(10.))
                    .children(Mode::ALL.into_iter().map(|mode| {
                        div()
                            .id(("code-search-mode", mode as usize))
                            .debug_selector(move || {
                                format!("code-search-mode-{}", mode.label().to_lowercase())
                            })
                            .px_2()
                            .py_1()
                            .rounded(px(crate::config::corners::CONTROL))
                            .cursor_pointer()
                            .when(mode == code.mode, |tab| tab.bg(rgb(theme.active)))
                            .hover(|tab| tab.bg(rgb(theme.active)))
                            .child(mode.label())
                            .on_click(cx.listener(move |this, _, window, cx| {
                                this.set_code_search_mode(mode, window, cx);
                            }))
                    }))
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .truncate()
                            .text_right()
                            .text_color(rgb(theme.muted))
                            .child(summary),
                    ),
            )
    }

    fn code_search_row(
        &self,
        code: &CodeSearch,
        row: usize,
        cx: &mut Context<Self>,
    ) -> Option<Stateful<Div>> {
        let hit = code.hits.get(row)?;
        let index = code.index.as_ref()?;
        let label: SharedString = rank::label(index, code.mode, hit.item).to_owned().into();
        let (detail, badge) = match code.mode {
            Mode::Symbols => {
                let symbol = index.symbols().get(hit.item)?;
                let file = index.files().get(symbol.file)?;
                (format!("{file}:{}", symbol.line), symbol.kind.label())
            }
            Mode::Files => (String::new(), ""),
        };
        let matched = HighlightStyle {
            color: Some(rgb(self.theme.primary()).into()),
            font_weight: Some(FontWeight::BOLD),
            ..Default::default()
        };
        let label = StyledText::new(label)
            .with_highlights(hit.highlights.iter().map(|range| (range.clone(), matched)));
        Some(
            div()
                .id(row)
                .debug_selector(move || format!("code-search-row-{row}"))
                .w_full()
                .h(px(self.config.ui.line_height() * 2. + 12.))
                .px(px(16.))
                .flex()
                .items_center()
                .gap(px(12.))
                .cursor_pointer()
                .when(row == code.selected, |div| div.bg(rgb(self.theme.active)))
                .hover(|s| s.bg(rgb(self.theme.active)))
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .flex()
                        .flex_col()
                        .child(div().truncate().child(label))
                        .when(!detail.is_empty(), |column| {
                            column.child(
                                div()
                                    .truncate()
                                    .text_color(rgb(self.theme.muted))
                                    .child(detail),
                            )
                        }),
                )
                .child(
                    div()
                        .flex_none()
                        .text_color(rgb(self.theme.muted))
                        .child(badge),
                )
                .on_click(cx.listener(move |this, event: &ClickEvent, window, cx| {
                    let viewer = event.modifiers().secondary();
                    this.activate_code_search(row, viewer, window, cx);
                })),
        )
    }

    pub(crate) fn render_code_search(&self, cx: &mut Context<Self>) -> Div {
        let Some(code) = &self.menu.code_search else {
            return div();
        };
        let theme = &self.theme;
        let empty = match &code.status {
            Status::Failed(error) => Some(error.clone()),
            Status::Locating | Status::Indexing if code.index.is_none() => None,
            _ if code.hits.is_empty() && code.rank_task.is_none() => Some(match code.mode {
                Mode::Symbols => "No matching symbols.".into(),
                Mode::Files => "No matching files.".into(),
            }),
            _ => None,
        };
        div()
            .flex()
            .flex_col()
            .size_full()
            .min_h_0()
            .child(self.code_search_header(code, cx))
            .when_some(empty, |panel, text| {
                panel.child(
                    div()
                        .debug_selector(|| "code-search-empty".into())
                        .flex_1()
                        .p(px(16.))
                        .text_color(rgb(theme.muted))
                        .child(text),
                )
            })
            .when(!code.hits.is_empty(), |panel| {
                panel.child(
                    uniform_list(
                        "code-search-results",
                        code.hits.len(),
                        cx.processor(|this, range: std::ops::Range<usize>, _, cx| {
                            let Some(code) = &this.menu.code_search else {
                                return Vec::new();
                            };
                            range
                                .filter_map(|row| this.code_search_row(code, row, cx))
                                .collect()
                        }),
                    )
                    .track_scroll(&code.scroll)
                    .flex_1()
                    .min_h_0(),
                )
            })
            .child(
                div()
                    .flex_none()
                    .px(px(16.))
                    .py(px(10.))
                    .border_t_1()
                    .border_color(rgb(theme.active))
                    .text_color(rgb(theme.muted))
                    .child(if cfg!(target_os = "macos") {
                        "↑ / ↓ navigate · Tab symbols/files · Enter editor · ⌘Enter viewer · Esc cancel"
                    } else {
                        "↑ / ↓ navigate · Tab symbols/files · Enter editor · Ctrl-Enter viewer · Esc cancel"
                    }),
            )
    }
}
