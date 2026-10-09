//! The VS Code panel when the app starts VS Code itself. The launcher is a
//! stand-in that runs nothing; its reports are set by hand.
use super::*;
use crate::{
    code_server::{
        Launcher, Server,
        launcher::Cli,
        supervisor::{Address, Report, Status},
    },
    controls::Command,
};
use std::num::NonZeroU16;

const COMMIT: &str = "2a59476c9bfcb90b3ddc372c36762471b7dfad1c";

fn answers(_: &WebUrl) -> crate::Result<Server> {
    Ok(Server::from_version(COMMIT).unwrap())
}

/// The fixture window with VS Code found, as a running app would have it.
fn found(cx: &mut gpui::TestAppContext) -> (Entity<HerdrWindow>, &mut VisualTestContext) {
    let (view, cx) = window(cx);
    cx.update(|_, cx| {
        Launcher::fixture(cx, Cli::Found("/Applications/code-tunnel".into()));
        view.update(cx, |view, _| view.browser.code_server.probe = answers);
    });
    draw(cx);
    (view, cx)
}

fn run(view: &Entity<HerdrWindow>, cx: &mut VisualTestContext, command: Command) {
    cx.update(|window, cx| view.update(cx, |view, cx| view.command(command, window, cx)));
    draw(cx);
}

fn tick(view: &Entity<HerdrWindow>, cx: &mut VisualTestContext) {
    cx.update(|window, cx| view.update(cx, |view, cx| view.ensure_code_page(window, cx)));
    cx.run_until_parked();
    draw(cx);
}

fn accept(view: &Entity<HerdrWindow>, cx: &mut VisualTestContext) {
    view.update(cx, |view, _| view.config.code.license_accepted = true);
}

fn set_report(cx: &mut VisualTestContext, status: Status, port: Option<u16>) {
    cx.update(|_, cx| {
        Launcher::set_report(
            cx,
            Some(Report {
                revision: 1,
                status,
                address: port.map(|port| Address {
                    port: NonZeroU16::new(port).unwrap(),
                    url: WebUrl::try_from(format!("http://127.0.0.1:{port}/?tkn=secret").as_str())
                        .unwrap(),
                }),
            }),
        );
    });
}

fn running(cx: &mut VisualTestContext) -> bool {
    cx.update(|_, cx| Launcher::running(cx))
}

#[gpui::test]
fn a_found_vs_code_asks_for_its_license_before_starting(cx: &mut gpui::TestAppContext) {
    let (view, cx) = found(cx);
    // No address is set, yet the panel is offered.
    assert!(cx.debug_bounds("toggle-code").is_some());
    run(&view, cx, Command::ToggleCode);
    assert!(cx.debug_bounds("code-consent").is_some());
    assert!(cx.debug_bounds("code-accept").is_some());
    tick(&view, cx);
    assert!(
        !running(cx),
        "nothing starts before the license is accepted"
    );

    let button = cx.debug_bounds("code-accept").unwrap().center();
    cx.simulate_click(button, gpui::Modifiers::default());
    draw(cx);
    view.read_with(cx, |view, _| assert!(view.config.code.license_accepted));
    tick(&view, cx);
    assert!(running(cx));
    assert!(cx.debug_bounds("code-consent").is_none());
    assert!(cx.debug_bounds("code-placeholder").is_some(), "starting");
}

#[gpui::test]
fn vs_code_starts_only_once_a_page_needs_it(cx: &mut gpui::TestAppContext) {
    let (view, cx) = found(cx);
    accept(&view, cx);
    tick(&view, cx);
    tick(&view, cx);
    assert!(!running(cx), "no panel, no page, no server");
    run(&view, cx, Command::ToggleCode);
    assert!(running(cx));
    assert!(cx.debug_bounds("code-placeholder").is_some());
    assert!(cx.debug_bounds("code-unreachable").is_none());
}

#[gpui::test]
fn a_started_server_opens_the_page_on_its_own_address(cx: &mut gpui::TestAppContext) {
    let (view, cx) = found(cx);
    accept(&view, cx);
    run(&view, cx, Command::ToggleCode);
    // Its address is made, but VS Code has not answered yet.
    set_report(cx, Status::Starting, Some(51234));
    tick(&view, cx);
    let tab = |view: &Entity<HerdrWindow>, cx: &mut VisualTestContext| {
        cx.update(|_, cx| {
            let tab_scope = scope(&view.read(cx).endpoints[0]);
            cx.global::<Store>().code_tab(&tab_scope, "w0").cloned()
        })
    };
    assert!(tab(&view, cx).is_none());
    assert!(cx.debug_bounds("code-placeholder").is_some());

    set_report(cx, Status::Ready, Some(51234));
    tick(&view, cx);
    // The server answered; the next tick opens the tab.
    tick(&view, cx);
    let opened = tab(&view, cx).unwrap();
    assert_eq!(
        opened.location,
        Some(url("http://127.0.0.1:51234/?tkn=secret"))
    );
}

#[gpui::test]
fn a_server_that_stopped_says_why_in_the_panel(cx: &mut gpui::TestAppContext) {
    let (view, cx) = found(cx);
    accept(&view, cx);
    run(&view, cx, Command::ToggleCode);
    let error = crate::code_server::Error::PortTaken { port: 51234 };
    set_report(
        cx,
        Status::Failed {
            error: Arc::new(error.into()),
            retry: std::time::Instant::now(),
        },
        Some(51234),
    );
    tick(&view, cx);
    assert!(cx.debug_bounds("code-unreachable").is_some());
    assert!(cx.debug_bounds("code-placeholder").is_none());
}

#[gpui::test]
fn an_address_keeps_the_server_the_user_runs(cx: &mut gpui::TestAppContext) {
    let (view, cx) = found(cx);
    accept(&view, cx);
    view.update(cx, |view, _| {
        view.config.code.url = Some(WebUrl::try_from("http://127.0.0.1:8000/?tkn=x").unwrap());
    });
    run(&view, cx, Command::ToggleCode);
    tick(&view, cx);
    tick(&view, cx);
    assert!(!running(cx), "an older config's address is used as before");
    let opened = cx.update(|_, cx| {
        let tab_scope = scope(&view.read(cx).endpoints[0]);
        cx.global::<Store>().code_tab(&tab_scope, "w0").cloned()
    });
    assert_eq!(
        opened.unwrap().location,
        Some(url("http://127.0.0.1:8000/?tkn=x"))
    );
}

#[gpui::test]
fn starting_without_vs_code_installed_says_so(cx: &mut gpui::TestAppContext) {
    let (view, cx) = window(cx);
    cx.update(|_, cx| Launcher::fixture(cx, Cli::Missing));
    view.update(cx, |view, _| {
        view.config.code.mode = Some(crate::config::CodeMode::Start);
        view.config.code.license_accepted = true;
    });
    draw(cx);
    assert!(cx.debug_bounds("toggle-code").is_some());
    run(&view, cx, Command::ToggleCode);
    tick(&view, cx);
    assert!(!running(cx));
    assert!(cx.debug_bounds("code-placeholder").is_some());
}
