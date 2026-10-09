//! The setup script of a worktree created on another host. Scripts run
//! through the connection the window shows, so the setup waits until the
//! window has switched to that host and lists the new workspace, then starts
//! as a local creation's does, asking for trust the same way.

use crate::{
    HerdrWindow,
    teleport::NewCheckout,
    window::Flash,
    worktree_scripts::{Checkout, Launch, ScriptKind},
};
use gpui::Context;
use std::time::{Duration, Instant};

/// How long a setup waits for its host and workspace, as a follow does.
const WAIT: Duration = Duration::from_secs(30);

/// A setup script waiting for the window to reach its checkout.
pub(crate) struct Setup {
    pub(crate) endpoint_id: String,
    pub(crate) workspace_id: String,
    pub(crate) repo: String,
    pub(crate) checkout: NewCheckout,
    pub(crate) until: Instant,
}

impl Setup {
    pub(crate) fn new(
        endpoint_id: String,
        workspace_id: String,
        repo: String,
        checkout: NewCheckout,
        now: Instant,
    ) -> Self {
        Self {
            endpoint_id,
            workspace_id,
            repo,
            checkout,
            until: now + WAIT,
        }
    }
}

impl HerdrWindow {
    /// Start a waiting setup once its host is shown and lists its workspace,
    /// and no other script is starting. Gives up after [`WAIT`].
    pub(crate) fn poll_dispatch_setup(&mut self, now: Instant, cx: &mut Context<Self>) {
        let Some(setup) = &self.dispatch_setup else {
            return;
        };
        if now >= setup.until {
            let host = setup.endpoint_id.clone();
            self.dispatch_setup = None;
            tracing::warn!(%host, "the setup script's host was never shown");
            self.show_flash(
                Flash::warning("The new worktree's setup script did not start"),
                cx,
            );
            return;
        }
        let shown = self.endpoints[self.selected_endpoint].id == setup.endpoint_id
            && self.live.status.is_connected();
        let Some(snapshot) = self.live.snapshot.as_ref().filter(|_| shown) else {
            return;
        };
        let listed = snapshot
            .workspaces
            .iter()
            .any(|workspace| workspace.workspace_id == setup.workspace_id);
        if !listed || self.worktree_script.is_some() {
            return;
        }
        let boot = snapshot.boot_id.clone();
        let Some(setup) = self.dispatch_setup.take() else {
            return;
        };
        let launch = Launch {
            kind: ScriptKind::Setup,
            endpoint: (
                self.selection_epoch,
                self.endpoints[self.selected_endpoint].generation,
            ),
            endpoint_id: setup.endpoint_id,
            boot,
            workspace: setup.workspace_id,
            repo: setup.repo,
            repo_key: setup.checkout.repo_key,
            checkout: Some(Checkout {
                path: setup.checkout.path,
                root: setup.checkout.root,
            }),
            force: false,
            requested: false,
        };
        if let Err(error) = self.start_worktree_script(launch, cx) {
            self.local_error = Some(format!("The setup script did not start: {error}"));
            cx.notify();
        }
    }
}
