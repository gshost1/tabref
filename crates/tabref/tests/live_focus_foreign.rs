//! Live runs of `focus T` in a browser tabref did not launch (`--cdp`):
//! tabref never tells its tabs they hold the window focus, so `focus`
//! moves the keyboard focus, says when the tab is in the background, and
//! leaves nothing changed behind, even when its client is killed during the
//! command. Refusals (a disabled field, plain text, a link with no address,
//! a label whose control is hidden) come before anything in the page
//! changes. Opt-in:
//! `TABREF_LIVE=1 cargo test -p tabref --test live_focus_foreign -- --test-threads=1`.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use std::process::{Command, Stdio};
use std::time::Duration;

use common::focus_page::{active, foreign_site, open_page, read};
use common::harness;
use common::live;

const BACKGROUND: &str = "(the tab is in the background: the page may not run its focus handlers)";

#[test]
fn focus_says_the_tab_is_in_the_background_and_changes_nothing_else() {
    if !live() {
        return;
    }
    let tabref = foreign_site("focus-foreign");
    let focused = tabref.ok(&["focus", "Name"]);
    let first = focused.stdout.lines().next().unwrap();
    assert!(
        first.starts_with(&format!("focused \"Name\" {BACKGROUND}")),
        "{}",
        focused.stdout
    );
    assert_eq!(active(&tabref), "name");
    assert_eq!(read(&tabref, "document.hasFocus()"), "false");
    assert_eq!(
        read(&tabref, "document.getElementById('seen').textContent"),
        "Unseen"
    );
}

#[test]
fn a_refusal_comes_before_anything_in_the_page_changes() {
    if !live() {
        return;
    }
    let tabref = foreign_site("focus-refusals");
    let refusals = [
        ("Locked", "error ACTION_FAILED (element_disabled): "),
        (
            "Tap area",
            "error ACTION_NOT_SUPPORTED (cdp_not_focusable): ",
        ),
        (
            "Bare link",
            "error ACTION_NOT_SUPPORTED (cdp_not_focusable): ",
        ),
        ("Hidden box", "error ACTION_FAILED (element_hidden): "),
    ];
    for (name, refusal) in refusals {
        let refused = tabref.run(&["--timeout", "4000", "focus", name]);
        assert!(
            refused.stderr.starts_with(refusal),
            "{name}: {}",
            refused.stderr
        );
        assert_eq!(
            read(&tabref, "document.getElementById('seen').textContent"),
            "Unseen",
            "{name}"
        );
    }
    assert_eq!(read(&tabref, "document.activeElement.localName"), "body");
    for name in ["Bare link", "Bad index"] {
        let refused = tabref.run(&["--json", "focus", name]).json();
        assert_eq!(
            refused["error"]["platform_detail"], "not focusable",
            "{name} is refused by tabref before DOM.focus: {refused}"
        );
    }
}

#[test]
fn an_svg_link_and_a_host_that_delegates_focus_take_the_focus() {
    if !live() {
        return;
    }
    let tabref = foreign_site("focus-svg-shadow");
    tabref.ok(&["focus", "Svg link"]);
    assert_eq!(active(&tabref), "svglink");
    tabref.ok(&["focus", "Delegated"]);
    assert_eq!(active(&tabref), "xfield");
    assert_eq!(
        read(
            &tabref,
            "document.activeElement.shadowRoot.activeElement.id"
        ),
        "inner"
    );
}

#[test]
fn a_killed_focus_leaves_the_tab_as_it_was() {
    if !live() {
        return;
    }
    let tabref = foreign_site("focus-killed");
    let mut client = Command::new(env!("CARGO_BIN_EXE_tabref"))
        .args(["--cdp", &tabref.browser.port.to_string()])
        .args(["--timeout", "30000", "focus", "Slow"])
        .env("TABREF_HOME", &tabref.home)
        .env("TABREF_REFS", "long")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    std::thread::sleep(Duration::from_millis(800));
    let _ = Command::new("kill")
        .args(["-KILL", &client.id().to_string()])
        .status();
    let _ = client.wait();
    std::thread::sleep(Duration::from_millis(3000));
    assert_eq!(read(&tabref, "document.hasFocus()"), "false");
    assert_eq!(
        read(&tabref, "document.getElementById('seen').textContent"),
        "Unseen"
    );
}

#[test]
fn a_stale_launch_record_on_the_same_port_does_not_make_it_tabrefs_browser() {
    if !live() {
        return;
    }
    let tabref = harness("focus-stale");
    let mut record = serde_json::to_value(&tabref.browser).unwrap();
    record["pid"] = serde_json::json!(999_999);
    record["started"] = serde_json::Value::Null;
    record["profile_dir"] = serde_json::json!(tabref.home.join("gone-profile"));
    std::fs::write(tabref.home.join("launched.json"), record.to_string()).unwrap();
    open_page(&tabref);
    assert_eq!(read(&tabref, "document.hasFocus()"), "false");
    tabref.ok(&["focus", "City"]);
    assert_eq!(read(&tabref, "document.hasFocus()"), "false");
    assert_eq!(
        read(&tabref, "document.getElementById('seen').textContent"),
        "Unseen"
    );
}

#[test]
fn a_host_that_delegates_focus_finds_a_control_it_can_focus_or_is_refused() {
    if !live() {
        return;
    }
    let tabref = foreign_site("focus-delegates");
    tabref.ok(&["focus", "Nested"]);
    assert_eq!(
        read(
            &tabref,
            "document.activeElement.shadowRoot.activeElement.shadowRoot.activeElement.id"
        ),
        "inner"
    );
    tabref.ok(&["focus", "Skips hidden"]);
    assert_eq!(
        read(
            &tabref,
            "document.activeElement.shadowRoot.activeElement.id"
        ),
        "real"
    );
    let empty = tabref.run(&["--json", "focus", "Empty host"]).json();
    assert_eq!(
        empty["error"]["platform_detail"], "not focusable",
        "refused by tabref before DOM.focus: {empty}"
    );
    tabref.ok(&["focus", "Bad tab host"]);
    assert_eq!(
        read(
            &tabref,
            "document.activeElement.shadowRoot.activeElement.id"
        ),
        "good"
    );
    tabref.ok(&["focus", "Veiled host"]);
    assert_eq!(
        read(
            &tabref,
            "document.activeElement.shadowRoot.activeElement.id"
        ),
        "plain"
    );
    tabref.ok(&["focus", "Closed host"]);
    assert_eq!(active(&tabref), "closed");
    tabref.ok(&["focus", "Tabbed host"]);
    assert_eq!(
        read(
            &tabref,
            "document.activeElement.shadowRoot.activeElement.id"
        ),
        "tabbed-inner"
    );
    let shut = tabref.run(&["--json", "focus", "Shut host"]).json();
    assert_eq!(
        shut["error"]["platform_detail"], "not focusable",
        "a host that neither delegates nor has a tabindex is refused: {shut}"
    );
    tabref.ok(&["focus", "Own host"]);
    assert_eq!(active(&tabref), "own");
    assert_eq!(
        read(
            &tabref,
            "String(document.activeElement.shadowRoot.activeElement)"
        ),
        "null"
    );
}
