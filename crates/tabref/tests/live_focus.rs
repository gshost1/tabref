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
<label id=ziplabel for=zip onclick="1">Zip code</label> <input id=zip aria-label="Postal">
<label>Jumpy <input id=jumpy onfocus="setTimeout(() => document.getElementById('city').focus(), 50)"></label>
<p id=out>Nothing</p>
<p id=seen>Unseen</p>
<script>
document.addEventListener('visibilitychange', () => { document.getElementById('seen').textContent = 'vis changed'; });
window.addEventListener('focus', () => { document.getElementById('seen').textContent = 'window focused'; });
</script>"#;

fn site() -> Harness {
    let port = serve(&[("/focus", PAGE)]);
    let tabref = harness("focus");
    tabref.ok(&["open", &format!("http://127.0.0.1:{port}/focus")]);
    tabref
}

/// The focus page in tab 0, behind a blank tab that has the front.
fn background_site() -> Harness {
    let tabref = site();
    tabref.ok(&["open", "--new-tab", "about:blank"]);
    in_back(
        &tabref,
        "document.getElementById('seen').textContent = 'Unseen'",
    );
    tabref
}

fn in_back(tabref: &Harness, expression: &str) -> String {
    tabref
        .ok(&["--tab", "0", "js", expression])
        .stdout
        .trim()
        .to_owned()
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

#[test]
fn focus_in_a_background_tab_leaves_it_in_the_background() {
    if !live() {
        return;
    }
    let tabref = background_site();
    assert_eq!(in_back(&tabref, "document.visibilityState"), "hidden");
    let focused = tabref.ok(&["--tab", "0", "focus", "Name"]);
    assert!(
        focused.stdout.contains("name focused"),
        "{}",
        focused.stdout
    );
    assert!(
        !focused.stdout.contains("vis changed"),
        "{}",
        focused.stdout
    );
    assert!(
        !focused.stdout.contains("window focused"),
        "{}",
        focused.stdout
    );
    assert_eq!(in_back(&tabref, "document.visibilityState"), "hidden");
    assert_eq!(in_back(&tabref, "document.hasFocus()"), "false");
}

#[test]
fn a_refused_focus_changes_nothing_in_the_page() {
    if !live() {
        return;
    }
    let tabref = background_site();
    let refused = tabref.run(&["--tab", "0", "focus", "Tap area"]);
    assert!(
        refused
            .stderr
            .starts_with("error ACTION_NOT_SUPPORTED (cdp_not_focusable): "),
        "{}",
        refused.stderr
    );
    assert_eq!(
        in_back(&tabref, "document.getElementById('seen').textContent"),
        "Unseen"
    );
    let pressed = tabref.run(&["--tab", "0", "press", "Enter", "Tap area"]);
    assert!(
        pressed
            .stderr
            .starts_with("error ACTION_NOT_SUPPORTED (cdp_not_focusable): "),
        "{}",
        pressed.stderr
    );
}

#[test]
fn a_label_focuses_its_control_and_a_move_away_is_caught() {
    if !live() {
        return;
    }
    let tabref = site();
    let label = tabref.ok(&["snap"]).ref_for("Zip code");
    tabref.ok(&["focus", &label]);
    assert_eq!(active(&tabref), "zip");
    let jumpy = tabref.ok(&["focus", "Jumpy"]);
    let first = jumpy.stdout.lines().next().unwrap();
    assert_eq!(
        first, "focused \"Jumpy\" (the page moved the focus away)",
        "{}",
        jumpy.stdout
    );
    assert_eq!(active(&tabref), "city");
}
