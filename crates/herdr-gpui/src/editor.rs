//! Opening a file at a line in the user's terminal editor, in a new pane
//! beside the one the user was working in, the way an IDE jumps to a
//! definition. The daemon owns the pane and the editor runs in it like any
//! other program, so closing the editor closes its pane.
//!
//! The endpoint API has no method that starts a command in a pane, so the
//! editor starts from the new pane's shell: `pane.split` opens it in the
//! source pane's directory, then one line is typed to run the editor. Paths
//! come from terminal output, which is untrusted, so the typed line holds
//! them only as single-quoted words without a quote, backslash, or control
//! character: POSIX shells, fish, and nushell all read such a word literally.
//! Anything else opens in the system's default application instead.

use crate::{HerdrWindow, NavigationTarget};
use gpui::Context;
use herdr_client::{Method, protocol::ClientPaneInputEvent};
use serde::Deserialize;
use serde_json::{Value, json};
use std::path::{Path, PathBuf};

/// Whether this build starts the editor: the typed line runs POSIX `sh`,
/// which the shells of a Windows pane do not have.
pub(crate) const SUPPORTED: bool = cfg!(unix);

/// The longest command template accepted from the config.
const MAX_COMMAND_BYTES: usize = 1024;

/// Files a terminal editor has no use for, left to the system's viewer.
const MEDIA: [&str; 13] = [
    "bmp", "gif", "heic", "ico", "jpeg", "jpg", "mov", "mp3", "mp4", "pdf", "png", "svg", "webp",
];

/// Whether `text` reads the same as a single-quoted word in every shell the
/// typed line may reach.
fn quotable(text: &str) -> bool {
    !text.is_empty()
        && text.len() <= 4096
        && !text
            .chars()
            .any(|c| c == '\'' || c == '\\' || c.is_control())
}

/// The `editor_command` setting: how the editor is started, with `{file}`
/// and `{line}` standing for the file and the line to open it at. Without
/// either placeholder, `+{line} {file}` follows the command, which vi, Vim,
/// Neovim, Emacs, nano, micro, and Kakoune all understand.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
#[serde(try_from = "String")]
pub(crate) struct EditorCommand(String);

impl TryFrom<String> for EditorCommand {
    type Error = crate::Error;

    fn try_from(command: String) -> crate::Result<Self> {
        let command = command.trim();
        // The template is typed inside single quotes too.
        if command.len() > MAX_COMMAND_BYTES || !quotable(command) {
            return Err(crate::Error::EditorCommand);
        }
        Ok(Self(command.to_owned()))
    }
}

impl EditorCommand {
    /// The `sh` script that runs the editor, reading the file from `$0` and
    /// the line from `$1`.
    fn script(&self) -> String {
        let command = &self.0;
        if command.contains("{file}") || command.contains("{line}") {
            command
                .replace("{file}", "\"$0\"")
                .replace("{line}", "\"$1\"")
        } else {
            format!("{command} \"+$1\" \"$0\"")
        }
    }
}

/// A file to open, at a line when one is known.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct EditorTarget {
    /// Absolute, and checked to exist on this machine.
    pub(crate) path: PathBuf,
    pub(crate) line: Option<u32>,
}

impl EditorTarget {
    /// Whether a terminal editor is the place for `path`: a regular file
    /// that is not an image, a video, or a PDF.
    pub(crate) fn editable(path: &Path, metadata: &std::fs::Metadata) -> bool {
        let media = path
            .extension()
            .map(|extension| extension.to_string_lossy().to_ascii_lowercase())
            .is_some_and(|extension| MEDIA.contains(&extension.as_str()));
        metadata.is_file() && !media
    }
}

/// The line typed into the new pane's shell. A leading space keeps it out of
/// the history of shells that honor that, and `exec` hands the pane to the
/// editor, so quitting the editor closes the pane. Without a configured
/// command, the pane's own `$VISUAL` or `$EDITOR` is used, then `vi`.
pub(crate) fn command_line(
    target: &EditorTarget,
    command: Option<&EditorCommand>,
) -> crate::Result<String> {
    if !SUPPORTED {
        return Err(crate::Error::EditorUnsupported);
    }
    let path = target
        .path
        .to_str()
        .filter(|path| quotable(path) && target.path.is_absolute())
        .ok_or(crate::Error::EditorPath)?;
    let line = target.line.unwrap_or(1).max(1);
    let script = command.map_or_else(
        || r#"${VISUAL:-${EDITOR:-vi}} "+$1" "$0""#.to_owned(),
        EditorCommand::script,
    );
    Ok(format!(" exec sh -c 'exec {script}' '{path}' {line}"))
}

/// One editor pane being opened: the split it waits on and what to type
/// into the pane the split makes. Fenced like a worktree script, so a reply
/// from a replaced connection never types into anything.
pub(crate) struct Job {
    endpoint: (u64, u64),
    endpoint_id: String,
    boot: String,
    request: String,
    line: String,
}

/// The new pane a `pane.split` response names.
fn created_pane(response: &Value) -> crate::Result<String> {
    if let Some(error) = response.get("error").filter(|error| !error.is_null()) {
        return Err(crate::Error::DaemonResponse(error.clone()));
    }
    response["result"]["pane"]["pane_id"]
        .as_str()
        .filter(|id| !id.is_empty())
        .map(str::to_owned)
        .ok_or(crate::Error::EditorResponse)
}

impl HerdrWindow {
    /// Opens `target` in the editor, in a pane split to the right of
    /// `beside`, or of the focused pane. Reports a failure as a flash.
    pub(crate) fn open_in_editor(
        &mut self,
        target: &EditorTarget,
        beside: Option<&str>,
        cx: &mut Context<Self>,
    ) {
        if let Err(error) = self.start_editor(target, beside) {
            self.show_flash(crate::window::Flash::warning(error.to_string()), cx);
        }
        cx.notify();
    }

    fn start_editor(&mut self, target: &EditorTarget, beside: Option<&str>) -> crate::Result<()> {
        if self.editor_open.is_some() && self.editor_current() {
            return Err(crate::Error::EditorBusy);
        }
        let line = command_line(target, self.config.editor_command.as_ref())?;
        if self.selected_is_remote() {
            return Err(crate::Error::EditorNoPane);
        }
        let snapshot = self
            .live
            .snapshot
            .as_deref()
            .filter(|_| self.live.status.is_connected())
            .ok_or(crate::Error::NotConnected)?;
        let pane = beside
            .or(snapshot.focused_pane_id.as_deref())
            .and_then(|id| snapshot.panes.iter().find(|pane| pane.pane_id == id))
            .ok_or(crate::Error::EditorNoPane)?;
        let cwd = pane
            .foreground_cwd
            .clone()
            .or_else(|| pane.cwd.clone())
            .or_else(|| {
                target
                    .path
                    .parent()
                    .and_then(Path::to_str)
                    .map(str::to_owned)
            });
        let mut params = json!({
            "target_pane_id": pane.pane_id,
            "direction": "right",
            "focus": true,
        });
        if let Some(cwd) = cwd {
            params["cwd"] = json!(cwd);
        }
        let boot = snapshot.boot_id.clone();
        let endpoint = &self.endpoints[self.selected_endpoint];
        let request = endpoint
            .connection
            .request_editor(&boot, Method::PaneSplit, params)?;
        self.editor_open = Some(Job {
            endpoint: (self.selection_epoch, endpoint.generation),
            endpoint_id: endpoint.id.clone(),
            boot,
            request,
            line,
        });
        Ok(())
    }

    fn editor_current(&self) -> bool {
        self.editor_open.as_ref().is_some_and(|job| {
            job.endpoint
                == (
                    self.selection_epoch,
                    self.endpoints[self.selected_endpoint].generation,
                )
                && self.live.status.is_connected()
                && self
                    .live
                    .snapshot
                    .as_ref()
                    .is_some_and(|snapshot| snapshot.boot_id == job.boot)
        })
    }

    /// Runs every window tick: once the split answers, types the editor's
    /// line into the new pane and brings its tab forward.
    pub(crate) fn poll_editor_open(&mut self, cx: &mut Context<Self>) {
        let Some(request) = self.editor_open.as_ref().map(|job| job.request.clone()) else {
            return;
        };
        if !self.editor_current() {
            self.editor_open = None;
            return;
        }
        let Some((id, Some(result))) = &self.live.editor_response else {
            return;
        };
        if *id != request {
            return;
        }
        let result = result.clone();
        self.live.editor_response = None;
        if let Ok(mut inbox) = self.endpoints[self.selected_endpoint]
            .connection
            .inbox
            .try_lock()
            && inbox
                .editor_response
                .as_ref()
                .is_some_and(|(id, _)| *id == request)
        {
            inbox.editor_response = None;
        }
        let Some(job) = self.editor_open.take() else {
            return;
        };
        let typed = result
            .map_err(crate::Error::EditorRequest)
            .and_then(|response| created_pane(&response))
            .and_then(|pane| {
                let handle = self.endpoints[self.selected_endpoint]
                    .connection
                    .handle
                    .as_ref()
                    .ok_or(crate::Error::NotConnected)?;
                handle.send_input(
                    &job.boot,
                    &pane,
                    [
                        ClientPaneInputEvent::TextCommit(job.line.clone()),
                        crate::menu::enter_key(),
                    ],
                )?;
                Ok(pane)
            });
        match typed {
            Ok(pane) => {
                self.navigate_endpoint(&job.endpoint_id, NavigationTarget::Pane(&pane), cx);
            }
            Err(error) => {
                self.show_flash(
                    crate::window::Flash::warning(format!("The editor did not open: {error}")),
                    cx,
                );
            }
        }
    }
}

#[cfg(test)]
mod tests;
