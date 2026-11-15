//! Live runs of `focus T`: the element takes the keyboard focus, which the
//! page's own focus handlers see, as a command and as a `do` step; a
//! disabled field is refused by the actionability gate, and an element that
//! cannot take focus (a clickable div) is refused. Opt-in:
//! `TABREF_LIVE=1 cargo test -p tabref --test live_focus -- --test-threads=1`.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use common::{Harness, harness, live, serve};

const PAGE: &str = r#"<!doctype html><title>Focus</title>
<label>Name <input id=name onfocus="document.getElementById('out').textContent = 'name focused'"></label>
<label>City <input id=city></label>
<label>Locked <input id=locked disabled></label>
<div id=tap onclick="document.title = 'tapped'">Tap area</div>
<p id=out>Nothing</p>"#;

fn site() -> Harness {
    let port = serve(&[("/focus", PAGE)]);
    let tabref = harness("focus");
    tabref.ok(&["open", &format!("http://127.0.0.1:{port}/focus")]);
    tabref
}

fn active(tabref: &Harness) -> String {
    tabref
        .ok(&["js", "document.activeElement.id"])
        .stdout
        .trim()
        .to_owned()
}

#[test]
fn focus_moves_the_keyboard_focus_and_the_page_sees_it() {
    if !live() {
        return;
    }
    let tabref = site();
    let focused = tabref.ok(&["focus", "Name"]);
    assert!(
        focused.stdout.starts_with("focused \"Name\""),
        "{}",
        focused.stdout
    );
    assert!(
        focused.stdout.contains("name focused"),
        "{}",
        focused.stdout
    );
    assert_eq!(active(&tabref), "name");
    let step = tabref.ok(&["do", "focus City; press a"]);
    assert!(
        step.stdout.starts_with("ok focus \"City\""),
        "{}",
        step.stdout
    );
    assert_eq!(active(&tabref), "city");
    let typed = tabref.ok(&["js", "document.getElementById('city').value"]);
    assert_eq!(typed.stdout.trim(), "a");
}

#[test]
fn focus_on_a_disabled_field_or_plain_text_is_refused() {
    if !live() {
        return;
    }
    let tabref = site();
    let locked = tabref.run(&["focus", "Locked"]);
    assert!(
        locked
            .stderr
            .starts_with("error ACTION_FAILED (element_disabled): "),
        "{}",
        locked.stderr
    );
    let plain = tabref.run(&["focus", "Tap area"]);
    assert!(
        plain
            .stderr
            .starts_with("error ACTION_NOT_SUPPORTED (cdp_not_focusable): "),
        "{}",
        plain.stderr
    );
    assert_ne!(active(&tabref), "tap");
}
