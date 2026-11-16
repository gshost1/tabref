//! Live runs of how an open listbox prints: options that are list items
//! (`<ul role=listbox><li role=option>`) show in the view like any other
//! control, and a click that opens a listbox names each option once, as
//! the new control it is, not again as an added text line.
//! Opt-in: `TABREF_LIVE=1 cargo test -p tabref --test live_listbox_view`.

#![cfg(unix)]
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use common::{harness, live, serve};

const LISTS: &str = r#"<!doctype html><title>Lists</title><h1>Lists</h1>
<ul role="listbox" aria-label="Pets"><li role="option">Cat</li><li role="option" aria-selected="true">Dog</li></ul>
<ul role="menu" aria-label="Actions"><li role="menuitem">Rename</li><li role="menuitem">Delete</li></ul>
<ul><li>Plain item</li></ul>"#;

const BUTTON: &str = include_str!("fixtures/aria/button.html");

#[test]
fn options_and_menu_items_that_are_list_items_are_printed() {
    if !live() {
        return;
    }
    let port = serve(&[("/l", LISTS)]);
    let tabref = harness("listbox-view");
    let opened = tabref.ok(&["open", &format!("http://127.0.0.1:{port}/l")]);
    for shown in [
        "option Cat]",
        "option Dog (selected)]",
        "menuitem Rename]",
        "menuitem Delete]",
        "Plain item",
    ] {
        assert!(opened.stdout.contains(shown), "{shown}:\n{}", opened.stdout);
    }
    let _ = tabref.run(&["daemon", "stop"]);
}

#[test]
fn a_click_that_opens_a_listbox_names_each_option_once() {
    if !live() {
        return;
    }
    let port = serve(&[("/b", BUTTON)]);
    let tabref = harness("listbox-click");
    let opened = tabref.ok(&["open", &format!("http://127.0.0.1:{port}/b")]);
    let colour = opened.ref_for("button Colour");
    let clicked = tabref.ok(&["click", &colour]);
    for name in ["Red", "Green", "Blue"] {
        let mentions = clicked
            .stdout
            .lines()
            .filter(|line| line.contains(name) && !line.starts_with("clicked"))
            .count();
        assert_eq!(mentions, 1, "{name}:\n{}", clicked.stdout);
    }
    assert!(
        clicked.stdout.contains("option Green"),
        "{}",
        clicked.stdout
    );
    let snapped = tabref.ok(&["snap"]);
    assert!(
        snapped.stdout.contains("option Green]"),
        "{}",
        snapped.stdout
    );
    let _ = tabref.run(&["daemon", "stop"]);
}
