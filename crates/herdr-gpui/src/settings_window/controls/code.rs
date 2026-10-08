//! The Code page: the address of the `code serve-web` server the VS Code
//! panel shows, and a check that it answers.
use super::*;
use crate::{
    browser::WebUrl,
    code_server::{self, Server},
};

/// The page's state: its address field and the last connection test.
pub(in crate::settings_window) struct CodeSettings {
    /// Made when the page first shows, which is when a window is at hand
    /// for its blur subscription.
    field: Option<UrlField>,
    test: Test,
    /// Fences out a test answer once the address has changed since.
    generation: u64,
    /// How the server is asked; tests answer for it.
    pub(in crate::settings_window) probe: fn(&WebUrl) -> crate::Result<Server>,
    #[cfg(test)]
    pub(in crate::settings_window) io: Option<CodeIo>,
}

impl Default for CodeSettings {
    fn default() -> Self {
        Self {
            field: None,
            test: Test::Idle,
            generation: 0,
            probe: code_server::probe,
            #[cfg(test)]
            io: None,
        }
    }
}

struct UrlField {
    input: Entity<SearchInput>,
    /// The text the page last put in the field. Any other text is an edit
    /// in progress, which a reload must not replace.
    shown: String,
    /// The text is not an address, so it was not saved.
    invalid: bool,
    _blur: Subscription,
}

impl UrlField {
    /// Puts `text` in the field, selected, as the page's own.
    fn show(&mut self, text: &str, cx: &mut App) {
        self.shown = text.to_owned();
        self.input
            .update(cx, |input, cx| input.set_text_selected(text, cx));
    }
}

#[derive(Clone, Debug, PartialEq)]
enum Test {
    Idle,
    Testing,
    Passed(Server),
    Failed(SharedString),
}

#[cfg(test)]
#[derive(Clone)]
pub(in crate::settings_window) struct CodeIo {
    pub(in crate::settings_window) write:
        std::sync::Arc<dyn Fn(Option<WebUrl>) -> crate::Result<()> + Send + Sync>,
    pub(in crate::settings_window) load: fn() -> crate::Result<crate::settings_window::Loaded>,
}

impl SettingsWindow {
    /// The saved address, as the field shows it.
    fn saved_code_url(&self) -> &str {
        self.config.code.url.as_ref().map_or("", WebUrl::as_str)
    }

    /// Makes the address field the first time the page shows.
    pub(in crate::settings_window) fn open_code_page(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.code.field.is_some() {
            self.sync_code_field(cx);
            return;
        }
        let input = cx.new(SearchInput::new);
        let saved = self.saved_code_url().to_owned();
        input.update(cx, |input, cx| {
            input.set_text_selected(&saved, cx);
            input.set_placeholder("http://127.0.0.1:8000/?tkn=\u{2026}", cx);
            input.set_appearance(self.config.ui.clone(), self.theme.clone(), cx);
        });
        let focus = input.read(cx).focus.clone();
        let blur = cx.on_blur(&focus, window, |this, _, cx| {
            this.finish_code_edit(true, cx);
        });
        self.code.field = Some(UrlField {
            input,
            shown: saved,
            invalid: false,
            _blur: blur,
        });
        cx.notify();
    }

    pub(in crate::settings_window) fn refresh_code_appearance(&mut self, cx: &mut Context<Self>) {
        if let Some(field) = &self.code.field {
            field.input.update(cx, |input, cx| {
                input.set_appearance(self.config.ui.clone(), self.theme.clone(), cx);
            });
        }
    }

    /// Shows the saved address in the field, unless it is being edited:
    /// a reload, after a save or a change to the file, keeps an edit that
    /// is in progress.
    pub(in crate::settings_window) fn sync_code_field(&mut self, cx: &mut Context<Self>) {
        let saved = self.saved_code_url().to_owned();
        let Some(field) = &mut self.code.field else {
            return;
        };
        let editing = field.invalid || field.input.read(cx).text() != field.shown;
        if editing || field.shown == saved {
            return;
        }
        field.show(&saved, cx);
    }

    /// Saves the field's address when `save`, else puts the saved one back.
    /// Returns false while the field holds what is not an address, which
    /// is kept for the user to fix.
    pub(in crate::settings_window) fn finish_code_edit(
        &mut self,
        save: bool,
        cx: &mut Context<Self>,
    ) -> bool {
        let saved = self.saved_code_url().to_owned();
        let Some(field) = &mut self.code.field else {
            return true;
        };
        let input = field.input.read(cx);
        if input.is_composing() {
            return false;
        }
        let text = input.text().trim().to_owned();
        if !save || text == saved {
            field.invalid = false;
            field.show(&saved, cx);
            cx.notify();
            return true;
        }
        let url = if text.is_empty() {
            None
        } else {
            match WebUrl::from_typed(&text) {
                Ok(url) => Some(url),
                Err(_) => {
                    field.invalid = true;
                    cx.notify();
                    return false;
                }
            }
        };
        field.invalid = false;
        field.show(url.as_ref().map_or("", WebUrl::as_str), cx);
        self.code.test = Test::Idle;
        self.code.generation += 1;
        self.save_code_url(url, cx);
        cx.notify();
        true
    }

    fn save_code_url(&mut self, url: Option<WebUrl>, cx: &mut Context<Self>) {
        #[cfg(test)]
        if let Some(io) = self.code.io.clone() {
            self.save_with(move || (io.write)(url), io.load, false, cx);
            return;
        }
        self.save_native(move || Config::save_code_url(url), cx);
    }

    fn code_url_key(&mut self, event: &KeyDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        if !matches!(event.keystroke.key.as_str(), "enter" | "escape") {
            return;
        }
        cx.stop_propagation();
        window.prevent_default();
        if self.finish_code_edit(event.keystroke.key == "enter", cx) {
            window.focus(&self.focus, cx);
        }
    }

    /// The address a test asks: the field's, when it holds one, else the
    /// saved one.
    fn code_test_url(&self, cx: &App) -> Option<WebUrl> {
        self.code
            .field
            .as_ref()
            .and_then(|field| WebUrl::from_typed(field.input.read(cx).text()).ok())
            .or_else(|| self.config.code.url.clone())
    }

    /// Asks the server whether it answers, off the UI thread.
    pub(in crate::settings_window) fn test_code_connection(&mut self, cx: &mut Context<Self>) {
        let Some(url) = self.code_test_url(cx) else {
            return;
        };
        if self.code.test == Test::Testing {
            return;
        }
        self.code.generation += 1;
        self.code.test = Test::Testing;
        let (generation, probe) = (self.code.generation, self.code.probe);
        let answer = cx.background_executor().spawn(async move { probe(&url) });
        cx.spawn(async move |this, cx| {
            let answer = answer.await;
            let _ = this.update(cx, |this, cx| {
                if this.code.generation != generation {
                    return;
                }
                this.code.test = match answer {
                    Ok(server) => Test::Passed(server),
                    Err(error) => Test::Failed(error.to_string().into()),
                };
                cx.notify();
            });
        })
        .detach();
        cx.notify();
    }

    pub(in crate::settings_window) fn render_code_controls(
        &self,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Div {
        let field = match &self.code.field {
            Some(field) => div()
                .debug_selector(|| "settings-code-url".into())
                .min_w_0()
                .on_key_down(cx.listener(Self::code_url_key))
                .when(field.invalid, |row| {
                    row.border_b_1().border_color(rgb(self.theme.primary()))
                })
                .child(field.input.clone()),
            None => div(),
        };
        let invalid = self.code.field.as_ref().is_some_and(|field| field.invalid);
        let testing = self.code.test == Test::Testing;
        let can_test = !testing && self.code_test_url(cx).is_some();
        let button = self
            .control_choice(
                "settings-code-test",
                if testing {
                    "Testing\u{2026}"
                } else {
                    "Test connection"
                },
                false,
                can_test,
            )
            .debug_selector(|| "settings-code-test".into())
            .flex_none()
            .when(can_test, |button| {
                button.on_click(cx.listener(|this, _, _, cx| this.test_code_connection(cx)))
            });
        let card = self
            .control_card("Server")
            .child(self.control_note(
                "Start a server with `code serve-web`, then paste the address it prints, with \
                 its ?tkn= token.",
            ))
            .child(field)
            .when(invalid, |card| {
                card.child(self.control_note(
                    "Enter an http or https address, such as http://127.0.0.1:8000/?tkn=\u{2026}",
                ))
            })
            .child(
                div()
                    .flex()
                    .flex_wrap()
                    .items_center()
                    .gap(px(12.))
                    .child(button)
                    .children(self.render_code_test_result()),
            );
        div().flex().flex_col().gap(px(24.)).child(card)
    }

    /// What the last test found: an icon, what it means, and what the
    /// server said of itself.
    fn render_code_test_result(&self) -> Option<Div> {
        let theme = &self.theme;
        let (icon, color, text, detail): (_, _, SharedString, Option<SharedString>) = match &self
            .code
            .test
        {
            Test::Idle | Test::Testing => return None,
            Test::Passed(server) => (
                "icons/pass.svg",
                theme.palette[2],
                "Connected to VS Code".into(),
                Some(format!("commit {}", server.short_commit()).into()),
            ),
            Test::Failed(message) => ("icons/error.svg", theme.palette[1], message.clone(), None),
        };
        Some(
            div()
                .debug_selector(|| "settings-code-result".into())
                .flex()
                .items_center()
                .gap(px(8.))
                .min_w_0()
                .child(
                    svg()
                        .path(icon)
                        .flex_none()
                        .size(px(16.))
                        .text_color(rgb(theme.ink(color))),
                )
                .child(div().min_w_0().child(text))
                .children(detail.map(|detail| div().text_color(rgb(theme.muted)).child(detail))),
        )
    }
}

#[cfg(test)]
mod tests;
