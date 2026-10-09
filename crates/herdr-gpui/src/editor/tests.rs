#![allow(clippy::unwrap_used)]
use super::*;

#[cfg(unix)]
fn target(path: &str, line: Option<u32>) -> EditorTarget {
    EditorTarget {
        path: PathBuf::from(path),
        line,
    }
}

/// Runs a typed `line` the way a pane's shell would, with `editor` (a
/// program name) as `$EDITOR`, and returns the arguments that editor got.
#[cfg(unix)]
fn typed(line: &str, editor: &str, configured: Option<&str>) -> Vec<String> {
    use std::os::unix::fs::PermissionsExt;
    let dir = tempfile::tempdir().unwrap();
    let out = dir.path().join("args");
    let program = dir.path().join(editor);
    std::fs::write(
        &program,
        format!(
            "#!/bin/sh\nfor a in \"$@\"; do printf '%s\\n' \"$a\"; done > '{}'\n",
            out.display()
        ),
    )
    .unwrap();
    std::fs::set_permissions(&program, std::fs::Permissions::from_mode(0o755)).unwrap();
    // A configured command names its program; point it at the fake one.
    let line = configured.map_or_else(
        || line.to_owned(),
        |name| line.replace(&format!(" {name} "), &format!(" {} ", program.display())),
    );
    let status = std::process::Command::new("sh")
        .arg("-c")
        .arg(&line)
        .env_remove("VISUAL")
        .env("EDITOR", &program)
        .status()
        .unwrap();
    assert!(status.success(), "{line}");
    std::fs::read_to_string(out)
        .unwrap()
        .lines()
        .map(str::to_owned)
        .collect()
}

#[cfg(unix)]
#[test]
fn the_typed_line_runs_the_panes_editor_at_the_line() {
    let file = "/w/it is $(here)`x`;y.rs";
    let line = command_line(&target(file, Some(12)), None, None).unwrap();
    assert!(line.starts_with(" exec sh -c '"), "{line}");
    assert_eq!(typed(&line, "vim", None), ["+12", file]);
    // No line opens at the top, never `+` alone, which vi reads as the end.
    let line = command_line(&target("/w/a.rs", Some(0)), None, None).unwrap();
    assert_eq!(typed(&line, "vim", None), ["+1", "/w/a.rs"]);
}

#[cfg(unix)]
#[test]
fn a_neovim_listens_on_the_socket_in_a_private_folder() {
    let dir = tempfile::tempdir().unwrap();
    let socket = dir.path().join("private/7.sock");
    let line = command_line(&target("/w/a.rs", Some(3)), None, Some(&socket)).unwrap();
    assert_eq!(
        typed(&line, "nvim", None),
        ["--listen", socket.to_str().unwrap(), "+3", "/w/a.rs"]
    );
    let mode = std::os::unix::fs::PermissionsExt::mode(
        &std::fs::metadata(dir.path().join("private"))
            .unwrap()
            .permissions(),
    );
    assert_eq!(mode & 0o777, 0o700);
    // Another editor is never handed the socket.
    assert_eq!(typed(&line, "vim", None), ["+3", "/w/a.rs"]);
    // A configured Neovim listens too; a template runs as written.
    let command = |text: &str| EditorCommand::try_from(text.to_owned()).unwrap();
    let line = command_line(
        &target("/w/a.rs", Some(3)),
        Some(&command("nvim")),
        Some(&socket),
    )
    .unwrap();
    assert_eq!(
        typed(&line, "nvim", Some("nvim")),
        ["--listen", socket.to_str().unwrap(), "+3", "/w/a.rs"]
    );
    let line = command_line(
        &target("/w/a.rs", Some(3)),
        Some(&command("hx {file}:{line}")),
        Some(&socket),
    )
    .unwrap();
    assert_eq!(typed(&line, "hx", Some("hx")), ["/w/a.rs:3"]);
}

#[test]
fn neovim_is_named_by_the_commands_first_word() {
    for (command, nvim) in [
        ("nvim", true),
        ("/opt/homebrew/bin/nvim -p", true),
        ("nvim-qt", true),
        ("vim", false),
        ("hx nvim", false),
    ] {
        assert_eq!(nvim::is_nvim(command), nvim, "{command}");
    }
}

#[cfg(unix)]
#[test]
fn paths_a_shell_could_reinterpret_are_refused() {
    for path in [
        "/w/it's.rs",
        "/w/a\\b.rs",
        "/w/a\nrm -rf ~",
        "/w/\u{1b}[31m.rs",
        "relative/a.rs",
        "",
    ] {
        assert!(
            matches!(
                command_line(&target(path, Some(1)), None, None),
                Err(crate::Error::EditorPath)
            ),
            "{path:?}"
        );
    }
}

#[test]
fn commands_that_would_break_the_quoting_are_rejected() {
    for command in [
        "",
        "  ",
        "vim -c 'set x'",
        "a\\b",
        "vi\tm",
        &"v".repeat(1025),
    ] {
        assert!(
            matches!(
                EditorCommand::try_from(command.to_owned()),
                Err(crate::Error::EditorCommand)
            ),
            "{command:?}"
        );
    }
}

#[test]
fn the_split_response_names_the_new_pane() {
    let ok = json!({"result": {"type": "pane_info", "pane": {"pane_id": "w1:p9"}}});
    assert_eq!(created_pane(&ok).unwrap(), "w1:p9");
    assert!(matches!(
        created_pane(&json!({"result": {"pane": {"pane_id": ""}}})),
        Err(crate::Error::EditorResponse)
    ));
    assert!(matches!(
        created_pane(&json!({"error": {"message": "no such pane"}})),
        Err(crate::Error::DaemonResponse(_))
    ));
}

#[test]
fn media_files_are_left_to_the_system() {
    let dir = tempfile::tempdir().unwrap();
    for (name, editable) in [
        ("main.rs", true),
        ("Makefile", true),
        ("run.sh", true),
        ("shot.PNG", false),
        ("doc.pdf", false),
    ] {
        let path = dir.path().join(name);
        std::fs::write(&path, "x").unwrap();
        let metadata = std::fs::metadata(&path).unwrap();
        assert_eq!(EditorTarget::editable(&path, &metadata), editable, "{name}");
    }
    let metadata = std::fs::metadata(dir.path()).unwrap();
    assert!(!EditorTarget::editable(dir.path(), &metadata));
}

// The editor starts from a Unix shell; see `SUPPORTED`.
#[cfg(unix)]
mod flow {
    // Not a glob: the parent's imports would shadow `#[test]`.
    use super::{EditorTarget, PathBuf, command_line, json};
    use crate::{
        NavigationTarget, sidebar::layout_tests::fixture_window, window::MockPeer,
        worktree_scripts::tests::connect,
    };
    use herdr_client::protocol::{ClientMessage, ClientPaneInputEvent};

    #[gpui::test]
    fn a_file_opens_in_a_split_beside_its_pane_with_the_editor_typed(
        cx: &mut gpui::TestAppContext,
    ) {
        let mut peer = MockPeer::advertising(&["pane.split"]);
        let (view, cx) = cx.add_window_view(fixture_window);
        let target = EditorTarget {
            path: PathBuf::from("/repo/src/main.rs"),
            line: Some(42),
        };
        cx.update(|_, cx| {
            view.update(cx, |view, cx| {
                connect(view, &peer);
                view.open_in_editor(&target, Some("w1:p1"), cx);
                assert!(view.editor_open.is_some());
                // One editor opens at a time.
                view.flash = None;
                view.open_in_editor(&target, Some("w1:p1"), cx);
                let (flash, _) = view.flash.as_ref().unwrap();
                assert_eq!(flash.text.as_ref(), crate::Error::EditorBusy.to_string());
            })
        });
        let split = peer.request();
        assert_eq!(split["method"], "pane.split");
        assert_eq!(
            split["params"],
            json!({"target_pane_id": "w1:p1", "direction": "right", "focus": true, "cwd": "/repo"})
        );
        let id = split["id"].as_str().unwrap().to_owned();
        let created =
            json!({"id": id, "result": {"type": "pane_info", "pane": {"pane_id": "w1:p7"}}});
        peer.respond("boot-v1", &id, &created);
        let socket = view.read_with(cx, |view, _| {
            view.editor_open.as_ref().unwrap().socket.clone()
        });
        cx.update(|_, cx| {
            view.update(cx, |view, cx| {
                // Another request's answer is not this one.
                view.live.editor_response = Some(("other".into(), Some(Ok(json!(null)))));
                view.poll_editor_open(cx);
                assert!(view.editor_open.is_some());
                view.live.editor_response = Some((id.clone(), Some(Ok(created.clone()))));
                view.poll_editor_open(cx);
                assert!(view.editor_open.is_none());
                assert!(view.live.editor_response.is_none());
                assert_eq!(
                    view.pending_navigation,
                    Some(NavigationTarget::Pane("w1:p7".into()))
                );
            })
        });
        // The window may report its theme first.
        let input = std::iter::repeat_with(|| peer.receive())
            .find(|message| matches!(message, ClientMessage::ClientShellPaneInput { .. }))
            .unwrap();
        assert_eq!(
            input,
            ClientMessage::ClientShellPaneInput {
                pane_id: "w1:p7".into(),
                events: vec![
                    ClientPaneInputEvent::TextCommit(
                        command_line(&target, None, socket.as_deref()).unwrap()
                    ),
                    crate::menu::enter_key(),
                ],
            }
        );

        // The next file for that tab goes to the same editor while its pane
        // lives. Here no Neovim answers on the socket, so the pane is
        // forgotten and the file opens in a new split instead.
        cx.update(|_, cx| {
            view.update(cx, |view, cx| {
                assert_eq!(view.editor_panes.len(), usize::from(socket.is_some()));
                let snapshot = std::sync::Arc::make_mut(view.live.snapshot.as_mut().unwrap());
                let mut editor = snapshot.panes[0].clone();
                editor.pane_id = "w1:p7".into();
                snapshot.panes.push(editor);
                view.open_in_editor(&target, Some("w1:p1"), cx);
                // Nothing is split while the Neovim is asked.
                assert_eq!(view.editor_open.is_none(), socket.is_some());
            })
        });
        cx.run_until_parked();
        view.read_with(cx, |view, _| {
            assert!(view.editor_panes.is_empty());
            assert!(view.editor_open.is_some());
        });
        assert_eq!(peer.request()["method"], "pane.split");
    }

    #[gpui::test]
    fn a_reply_after_a_reconnect_types_nothing(cx: &mut gpui::TestAppContext) {
        let mut peer = MockPeer::advertising(&["pane.split"]);
        let (view, cx) = cx.add_window_view(fixture_window);
        let target = EditorTarget {
            path: PathBuf::from("/repo/a.rs"),
            line: None,
        };
        cx.update(|_, cx| {
            view.update(cx, |view, cx| {
                connect(view, &peer);
                view.open_in_editor(&target, None, cx);
                assert!(view.editor_open.is_some());
                view.selection_epoch += 1;
                view.poll_editor_open(cx);
                assert!(view.editor_open.is_none());
            })
        });
        assert_eq!(peer.request()["params"]["target_pane_id"], "w1:p1");
    }
}

/// Against a real Neovim; run with
/// `cargo test -p herdr-gpui -- --ignored editor::tests::a_listening`.
#[cfg(unix)]
#[test]
#[ignore = "needs nvim on PATH"]
fn a_listening_neovim_opens_files_at_their_line() {
    let dir = tempfile::tempdir().unwrap();
    let socket = dir.path().join("n.sock");
    let file = dir.path().join("a file.txt");
    std::fs::write(&file, "a\nb\nc\nd\ne\n").unwrap();
    let mut server = std::process::Command::new("nvim")
        .args(["--headless", "--clean", "--listen"])
        .arg(&socket)
        .spawn()
        .unwrap();
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
    while !socket.exists() && std::time::Instant::now() < deadline {
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
    let opened = nvim::open(
        &socket,
        &EditorTarget {
            path: file,
            line: Some(4),
        },
    );
    let at = std::process::Command::new("nvim")
        .args(["--headless", "--clean", "--server"])
        .arg(&socket)
        .args(["--remote-expr", "expand('%:t') . ':' . line('.')"])
        .output()
        .unwrap();
    let _ = server.kill();
    let _ = server.wait();
    opened.unwrap();
    assert_eq!(String::from_utf8_lossy(&at.stdout).trim(), "a file.txt:4");
    // Once it has gone, it says so rather than hanging.
    assert!(matches!(
        nvim::open(
            &socket,
            &EditorTarget {
                path: "/w/a".into(),
                line: None
            }
        ),
        Err(crate::Error::EditorRemote)
    ));
}
