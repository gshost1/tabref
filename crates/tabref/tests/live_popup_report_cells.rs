#![cfg(unix)]
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;
use common::{harness, live, serve};

#[test]
fn closing_an_unnamed_menu_is_not_hidden_by_another_unnamed_menu() {
    if !live() {
        return;
    }
    let port = serve(&[(
        "/",
        r#"<!doctype html>
<ul role=menu><li role=menuitem>Static action</li></ul>
<ul role=menu id=actions><li role=menuitem>Rename</li><li role=menuitem>Delete</li><li role=menuitem>Archive</li></ul>
<button onclick="document.getElementById('actions').hidden=true">Close actions</button>"#,
    )]);
    let tabref = harness("unnamed-menu-cells");
    tabref.ok(&["open", &format!("http://127.0.0.1:{port}/")]);
    let text = tabref.ok(&["click", "Close actions"]).stdout;
    assert!(text.contains("- menu closed (3 items)"), "{text}");
    assert!(!text.contains("- Rename"), "{text}");
}
