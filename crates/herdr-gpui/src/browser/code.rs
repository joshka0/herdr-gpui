//! The VS Code panel beside a workspace's editor groups: one page per
//! workspace, served by `code serve-web`, opened on the address `[code] url`
//! names and then wherever it goes.
//! The page is a browser tab placed in the panel rather than a strip, so it
//! is saved and dropped with its workspace like any other tab. Each
//! workspace shows or hides its panel on its own, and a hidden page stays
//! alive, keeping its state.
//!
//! A page that cannot load says nothing back through the web view, so the
//! window first asks the server whether it is there, and creates pages only
//! once it has answered lately. Until then the panel says why it is empty.
#[cfg(any(target_os = "macos", windows))]
use super::{Location, TabId, store::Place};
use super::{Scope, Store, WebUrl, view::store};
use crate::{
    HerdrWindow,
    code_server::{self, Server},
    window::Flash,
};
use gpui::{prelude::*, *};
use std::time::{Duration, Instant};

/// The query parameter that carries `code serve-web`'s connection token.
#[cfg(any(target_os = "macos", windows, test))]
const TOKEN: &str = "tkn";

/// How long an answer is trusted when a page is created, and how long an
/// unreachable server is left before it is asked again.
const RETRY: Duration = Duration::from_secs(5);

/// What the window knows of the configured VS Code server.
pub(crate) struct CodeServer {
    /// The address the state is for; a new one starts over.
    url: Option<WebUrl>,
    pub(super) state: Reach,
    /// Fences out the answer to a question asked about an older address.
    generation: u64,
    /// How the server is asked; tests answer for it.
    pub(super) probe: fn(&WebUrl) -> crate::Result<Server>,
}

impl Default for CodeServer {
    fn default() -> Self {
        Self {
            url: None,
            state: Reach::Unknown,
            generation: 0,
            probe: code_server::probe,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub(super) enum Reach {
    Unknown,
    Asking,
    Ready { server: Server, at: Instant },
    Failed { message: SharedString, at: Instant },
}

impl HerdrWindow {
    /// Whether the focused workspace shows its VS Code panel.
    pub(crate) fn shown_code(&self) -> bool {
        self.browser_key()
            .and_then(|key| self.browser.layouts.get(&key))
            .is_some_and(|layout| layout.code)
    }

    /// Shows or hides the focused workspace's panel. A hidden page keeps
    /// running, but no longer holds the keyboard.
    pub(crate) fn toggle_code(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(layout) = self.ensure_layout() else {
            self.show_flash(Flash::warning("Open a workspace first"), cx);
            return;
        };
        layout.code = !layout.code;
        if layout.code {
            self.open_code_page(true, window, cx);
        } else {
            #[cfg(any(target_os = "macos", windows))]
            if let Some(tab) = self
                .browser_key()
                .and_then(|(scope, workspace)| store(cx)?.code_tab(&scope, &workspace))
            {
                self.browser.pages.blur(tab.id, cx);
            }
            window.focus(&self.focus, cx);
        }
        cx.notify();
    }

    /// Closes this window's panel pages, keeping their tabs, and forgets why
    /// any could not be created, so each is created anew.
    #[cfg(any(target_os = "macos", windows))]
    fn close_code_pages(&mut self, cx: &App) {
        let Some(store) = store(cx) else {
            return;
        };
        let code = |id: TabId| store.get(id).is_some_and(|tab| tab.place == Place::Code);
        let pages: Vec<TabId> = self.browser.pages.ids().filter(|id| code(*id)).collect();
        for id in pages {
            self.browser.pages.close(id);
        }
        self.browser.failed.retain(|id, _| !code(*id));
    }

    /// The focused workspace's panel page, when the panel shows and the
    /// page exists, for the window to present.
    #[cfg(any(target_os = "macos", windows))]
    pub(super) fn code_page(&self, cx: &App) -> Option<TabId> {
        if !self.shown_code() {
            return None;
        }
        let (scope, workspace) = self.browser_key()?;
        let id = store(cx)?.code_tab(&scope, &workspace)?.id;
        self.browser.pages.contains(id).then_some(id)
    }

    /// Runs on every window tick: a shown panel gets its page, such as one
    /// restored at startup or that of a workspace just switched to.
    pub(super) fn ensure_code_page(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.open_code_page(false, window, cx);
    }

    /// Opens the focused workspace's panel tab, and creates its page, if
    /// the panel shows and its server answers. Creating a page starts the
    /// platform's web content processes, so this runs from input and ticks,
    /// never from render. `report` says why a tab could not open.
    fn open_code_page(&mut self, report: bool, window: &mut Window, cx: &mut Context<Self>) {
        if !super::EMBEDDED || !self.shown_code() {
            return;
        }
        let (Some((scope, workspace)), Some(url)) =
            (self.browser_key(), self.config.code.url.clone())
        else {
            return;
        };
        self.follow_code_address(&url, cx);
        if self.code_page_settled(&scope, &workspace, cx) || !self.code_server_ready(&url, cx) {
            return;
        }
        #[cfg(not(any(target_os = "macos", windows)))]
        let _ = (report, window, scope, workspace);
        #[cfg(any(target_os = "macos", windows))]
        {
            let opened = match store(cx).and_then(|store| store.code_tab(&scope, &workspace)) {
                Some(tab) => Some(tab.id),
                None => Store::update(cx, |store| {
                    store.open_code_tab(scope, &workspace, Location::Web { url: url.clone() })
                }),
            };
            let Some(id) = opened else {
                if report {
                    self.show_flash(Flash::warning("Too many browser tabs are open"), cx);
                }
                return;
            };
            let Some(mut tab) = store(cx).and_then(|store| store.get(id)).cloned() else {
                return;
            };
            if let Some(Location::Web { url: page }) = &tab.location {
                tab.location = Some(Location::Web {
                    url: with_token(page, &url),
                });
            }
            if let Err(error) = self.browser.pages.ensure(&tab, window, cx) {
                tracing::warn!(%error, "Cannot create the VS Code page");
                self.browser.failed.insert(id, error.to_string().into());
            }
            cx.notify();
        }
    }

    /// Whether the workspace's panel page exists, so VS Code reconnects to
    /// its server on its own, or could not be created and is not retried
    /// every tick. Either way the server need not be asked.
    fn code_page_settled(&self, scope: &Scope, workspace: &str, cx: &App) -> bool {
        let Some(id) = store(cx)
            .and_then(|store| store.code_tab(scope, workspace))
            .map(|tab| tab.id)
        else {
            return false;
        };
        #[cfg(any(target_os = "macos", windows))]
        if self.browser.pages.contains(id) {
            return true;
        }
        self.browser.failed.contains_key(&id)
    }

    /// Starts over when `url` is a new address: it closes the tabs still on
    /// another server, so they reopen on this one, and the pages of the
    /// rest, so they reopen with its token.
    fn follow_code_address(&mut self, url: &WebUrl, cx: &mut Context<Self>) {
        if self.browser.code_server.url.as_ref() != Some(url) {
            let server = &mut self.browser.code_server;
            server.url = Some(url.clone());
            server.state = Reach::Unknown;
            server.generation += 1;
            let origin = url.origin();
            let gone = Store::update(cx, |store| store.close_code_tabs_off(&origin));
            if !gone.is_empty() {
                self.forget_browser_tabs(|id| gone.contains(&id));
            }
            #[cfg(any(target_os = "macos", windows))]
            self.close_code_pages(cx);
        }
    }

    /// Whether the server at `url` has answered lately, asking it when
    /// nothing is known, it last answered a while ago, or it last failed a
    /// while ago. A page created against a server that has since stopped
    /// would stay blank, so an old answer is not trusted.
    fn code_server_ready(&mut self, url: &WebUrl, cx: &mut Context<Self>) -> bool {
        match &self.browser.code_server.state {
            Reach::Ready { at, .. } if at.elapsed() < RETRY => return true,
            Reach::Asking => return false,
            Reach::Failed { at, .. } if at.elapsed() < RETRY => return false,
            Reach::Unknown | Reach::Ready { .. } | Reach::Failed { .. } => {}
        }
        let server = &mut self.browser.code_server;
        server.state = Reach::Asking;
        let (generation, probe, url) = (server.generation, server.probe, url.clone());
        let answer = cx.background_executor().spawn(async move { probe(&url) });
        cx.spawn(async move |this, cx| {
            let answer = answer.await;
            let _ = this.update(cx, |this, cx| {
                let server = &mut this.browser.code_server;
                if server.generation != generation {
                    return;
                }
                server.state = match answer {
                    Ok(server) => Reach::Ready {
                        server,
                        at: Instant::now(),
                    },
                    Err(error) => {
                        tracing::info!(%error, "The VS Code server did not answer");
                        Reach::Failed {
                            message: error.to_string().into(),
                            at: Instant::now(),
                        }
                    }
                };
                cx.notify();
            });
        })
        .detach();
        cx.notify();
        false
    }
}

/// The address a panel page loads: where its tab last was, carrying the
/// token of the configured address `configured`, in place of any it had.
/// The server takes the token, sets its cookie, and redirects to the same
/// path and query without it, so a page keeps its folder across a restart
/// or a new token. An address on another server is left as it is.
#[cfg(any(target_os = "macos", windows, test))]
fn with_token(page: &WebUrl, configured: &WebUrl) -> WebUrl {
    // The raw query text, so the rest of the page's address stays exactly
    // as the server wrote it.
    let is_token = |pair: &&str| pair.split('=').next() == Some(TOKEN);
    let Some(token) = configured
        .0
        .query()
        .and_then(|query| query.split('&').find(is_token))
    else {
        return page.clone();
    };
    if page.origin() != configured.origin() {
        return page.clone();
    }
    let query: Vec<&str> = page
        .0
        .query()
        .unwrap_or_default()
        .split('&')
        .filter(|pair| !pair.is_empty() && !is_token(pair))
        .chain([token])
        .collect();
    let mut url = page.0.clone();
    url.set_query(Some(&query.join("&")));
    WebUrl::try_from(url.as_str()).unwrap_or_else(|_| page.clone())
}

#[cfg(test)]
mod tests;
