//! Neovim, reused: an editor pane that runs Neovim listens on a socket in
//! this app's private state folder, and later files for the same tab open
//! in it through `nvim --server … --remote`, as herdr-nvim's sidebar does,
//! instead of in another split. The socket is passed to the editor as an
//! argument, never typed as text a shell would read, and the files sent to
//! it travel as argv, so no path is ever parsed by a shell here.

use super::EditorTarget;
use std::{
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::atomic::{AtomicU64, Ordering},
    time::{Duration, Instant},
};

/// Unix socket paths are short: 104 bytes on macOS, 108 on Linux.
const MAX_SOCKET_BYTES: usize = 100;
/// How long one `nvim --server` call may take before it is given up on.
const DEADLINE: Duration = Duration::from_secs(5);

/// A fresh socket path for one editor pane, unique to this process. The
/// folder is made, private, by the line that starts the editor. `None` when
/// no state folder is known or the path would not fit a socket.
pub(crate) fn new_socket() -> Option<PathBuf> {
    static NEXT: AtomicU64 = AtomicU64::new(0);
    let dir = crate::preferences::state_dir()?.join("nvim");
    let name = format!(
        "{}-{}.sock",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    );
    let socket = dir.join(name);
    socket
        .to_str()
        .is_some_and(|text| text.len() <= MAX_SOCKET_BYTES && super::quotable(text))
        .then_some(socket)
}

/// Whether the first word of an editor command names Neovim.
pub(crate) fn is_nvim(command: &str) -> bool {
    command
        .split_whitespace()
        .next()
        .and_then(|program| program.rsplit('/').next())
        .is_some_and(|name| name.starts_with("nvim"))
}

/// Opens `target` in the Neovim listening on `socket`, at its line, and
/// centers it. Blocking: it runs on the background executor.
pub(crate) fn open(socket: &Path, target: &EditorTarget) -> crate::Result<()> {
    // Without a server `--remote` edits the file itself and never exits,
    // so make sure one is listening first.
    if !listening(socket) {
        return Err(crate::Error::EditorRemote);
    }
    // `--remote +LINE` would open a file named `+LINE`, so the line is set
    // with an expression once the file is open.
    run(nvim(socket).arg("--remote").arg(&target.path))?;
    if let Some(line) = target.line {
        let jump = format!("execute('call cursor({line}, 1) | normal! zz')");
        run(nvim(socket).arg("--remote-expr").arg(jump))?;
    }
    Ok(())
}

#[cfg(unix)]
fn listening(socket: &Path) -> bool {
    std::os::unix::net::UnixStream::connect(socket).is_ok()
}

/// Windows builds never start an editor (see `SUPPORTED`).
#[cfg(not(unix))]
fn listening(_: &Path) -> bool {
    false
}

fn nvim(socket: &Path) -> Command {
    let mut command = Command::new("nvim");
    command
        .env("PATH", crate::local_path::local_path())
        .args(["--headless", "--clean", "--server"])
        .arg(socket)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    command
}

/// Runs `command` to completion within [`DEADLINE`], killing it past that.
fn run(command: &mut Command) -> crate::Result<()> {
    let mut child = command.spawn().map_err(|_| crate::Error::EditorRemote)?;
    let deadline = Instant::now() + DEADLINE;
    loop {
        match child.try_wait() {
            Ok(Some(status)) if status.success() => return Ok(()),
            Ok(Some(_)) | Err(_) => return Err(crate::Error::EditorRemote),
            Ok(None) if Instant::now() >= deadline => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(crate::Error::EditorRemote);
            }
            Ok(None) => std::thread::sleep(Duration::from_millis(20)),
        }
    }
}
