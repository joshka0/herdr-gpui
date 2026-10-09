#![allow(clippy::unwrap_used)]
use super::*;

fn target(path: &str, line: Option<u32>) -> EditorTarget {
    EditorTarget {
        path: PathBuf::from(path),
        line,
    }
}

#[cfg(unix)]
#[test]
fn the_typed_line_quotes_the_path_and_uses_the_panes_editor() {
    assert_eq!(
        command_line(&target("/w/src/main.rs", Some(12)), None).unwrap(),
        r#" exec sh -c 'exec ${VISUAL:-${EDITOR:-vi}} "+$1" "$0"' '/w/src/main.rs' 12"#
    );
    // No line opens at the top, never `+` alone, which vi reads as the end.
    assert!(
        command_line(&target("/w/a b.rs", None), None)
            .unwrap()
            .ends_with("'/w/a b.rs' 1")
    );
    assert!(
        command_line(&target("/w/a.rs", Some(0)), None)
            .unwrap()
            .ends_with(" 1")
    );
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
                command_line(&target(path, Some(1)), None),
                Err(crate::Error::EditorPath)
            ),
            "{path:?}"
        );
    }
    // Shell syntax without quotes stays inside the single-quoted word.
    assert!(command_line(&target("/w/$(touch x)`y`;z.rs", None), None).is_ok());
}

#[cfg(unix)]
#[test]
fn a_configured_command_places_the_file_and_line() {
    let command = |text: &str| EditorCommand::try_from(text.to_owned()).unwrap();
    assert_eq!(
        command_line(&target("/w/a.rs", Some(3)), Some(&command("nvim"))).unwrap(),
        r#" exec sh -c 'exec nvim "+$1" "$0"' '/w/a.rs' 3"#
    );
    assert_eq!(
        command_line(
            &target("/w/a.rs", Some(3)),
            Some(&command(" hx {file}:{line} "))
        )
        .unwrap(),
        r#" exec sh -c 'exec hx "$0":"$1"' '/w/a.rs' 3"#
    );
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

mod flow {
    // Not a glob: the parent's imports would shadow `#[test]`.
    use super::{EditorTarget, PathBuf, command_line, json};
    use crate::{
        NavigationTarget, sidebar::layout_tests::fixture_window, window::MockPeer,
        worktree_scripts::tests::connect,
    };
    use herdr_client::protocol::{ClientMessage, ClientPaneInputEvent};

    #[cfg(unix)]
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
                    ClientPaneInputEvent::TextCommit(command_line(&target, None).unwrap()),
                    crate::menu::enter_key(),
                ],
            }
        );
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
