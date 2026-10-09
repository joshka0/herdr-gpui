//! A new VS Code server address moves the VS Code tabs in the groups to it,
//! keeping their place and their folder, rather than closing them.
use super::*;
use crate::{browser::store::Place, code_server::Server, controls::Command};

fn refuses(_: &WebUrl) -> crate::Result<Server> {
    Err(crate::code_server::Error::TokenRefused.into())
}

#[gpui::test]
fn a_new_server_keeps_grouped_vs_code_tabs_where_they_are(cx: &mut gpui::TestAppContext) {
    let (view, cx) = window(cx);
    let (grouped, panel) = cx.update(|_, cx| {
        let tab_scope = scope(&view.read(cx).endpoints[0]);
        Store::update(cx, |store| {
            let grouped = store
                .open_code_tab(
                    tab_scope.clone(),
                    "w1",
                    url("http://127.0.0.1:8000/?folder=/x&tkn=old"),
                )
                .unwrap();
            store.move_code_tab(grouped, Place::CodeGroup);
            let panel = store
                .open_code_tab(tab_scope, "w2", url("http://127.0.0.1:8000/?folder=/y"))
                .unwrap();
            (grouped, panel)
        })
    });
    cx.update(|_, cx| {
        view.update(cx, |view, _| {
            view.config.code.url =
                Some(WebUrl::try_from("http://127.0.0.1:9000/?tkn=new").unwrap());
            view.browser.code_server.probe = refuses;
        })
    });
    cx.update(|window, cx| {
        view.update(cx, |view, cx| view.command(Command::ToggleCode, window, cx))
    });
    cx.run_until_parked();
    cx.update(|_, cx| {
        let store = cx.global::<Store>();
        let tab = store.get(grouped).unwrap();
        assert_eq!(tab.place, Place::CodeGroup);
        // On the new server, with its folder; the new token is added as the
        // page loads, and the old one is gone.
        assert_eq!(tab.location, Some(url("http://127.0.0.1:9000/?folder=/x")));
        // A panel tab still closes, to open anew on its workspace's folder.
        assert!(store.get(panel).is_none());
    });
}
