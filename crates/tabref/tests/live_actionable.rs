//! Live regressions for actions on elements that cannot take them yet: a
//! disabled, aria-disabled or fieldset-disabled control, a hidden one, and a
//! read-only field are refused before any input with the reason and the next
//! step, and one that becomes ready within a moment is waited for. Opt-in:
//! `TABREF_LIVE=1 cargo test -p tabref --test live_actionable -- --test-threads=1`.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use common::{Harness, harness, live, serve};

const GATES: &str = r#"<!doctype html><title>Gates</title>
<script>function out(text) { document.getElementById('out').textContent = text; }</script>
<label>Name <input id=name oninput="document.getElementById('save').disabled = !this.value"></label>
<button type=button id=save disabled onclick="out('saved')">Save</button>
<button type=button id=later disabled onclick="out('later clicked')">Later</button>
<button type=button id=peek onclick="out('peek clicked')">Peek</button>
<div role=button tabindex=0 aria-disabled=true onclick="out('archive clicked')">Archive</div>
<fieldset disabled><legend>Shipping</legend><button type=button onclick="out('ship clicked')">Ship</button></fieldset>
<label>Code <input readonly value=ABC></label>
<label>Size <select disabled onchange="out('size changed')"><option>Small</option><option>Medium</option></select></label>
<label><input type=checkbox disabled onchange="out('terms changed')"> Terms</label>
<p id=out>Nothing</p>"#;

fn site() -> Harness {
    let port = serve(&[("/gates", GATES)]);
    let tabref = harness("actionable");
    tabref.ok(&["open", &format!("http://127.0.0.1:{port}/gates")]);
    tabref
}

fn untouched(tabref: &Harness) {
    let out = tabref.ok(&["js", "document.getElementById('out').textContent"]);
    assert_eq!(out.stdout.trim(), "Nothing");
}

#[test]
fn a_disabled_button_is_refused_with_the_reason_and_is_not_clicked() {
    if !live() {
        return;
    }
    let tabref = site();
    let run = tabref.run(&["click", "Save"]);
    assert!(!run.ok, "{}", run.stdout);
    assert!(
        run.stderr.starts_with(
            r#"error ACTION_FAILED (element_disabled): button "Save" is disabled; wait for the page to enable it, or fill what it needs"#
        ),
        "{}",
        run.stderr
    );
    assert!(!run.stdout.contains("no visible change"), "{}", run.stdout);
    let json = tabref.run(&["--json", "click", "Save"]).json();
    assert_eq!(json["error"]["disposition"]["delivery"], "not_delivered");
    untouched(&tabref);
    let batch = tabref.ok(&["do", "fill Name Ada; click Save"]);
    assert!(batch.stdout.contains("saved"), "{}", batch.stdout);
}

#[test]
fn a_button_enabled_a_moment_later_is_waited_for_and_clicked() {
    if !live() {
        return;
    }
    let tabref = site();
    tabref.ok(&[
        "js",
        "setTimeout(() => { document.getElementById('later').disabled = false; }, 300); 1",
    ]);
    let run = tabref.ok(&["click", "Later"]);
    assert!(run.stdout.contains("later clicked"), "{}", run.stdout);
}

#[test]
fn a_hidden_element_is_waited_for_until_shown_and_refused_while_it_stays_hidden() {
    if !live() {
        return;
    }
    let tabref = site();
    let peek = tabref.ok(&["snap"]).ref_for("Peek");
    tabref.ok(&[
        "js",
        "const b = document.getElementById('peek'); b.style.display = 'none'; setTimeout(() => { b.style.display = ''; }, 300); 1",
    ]);
    let shown = tabref.ok(&["click", &peek]);
    assert!(shown.stdout.contains("peek clicked"), "{}", shown.stdout);
    tabref.ok(&[
        "js",
        "document.getElementById('peek').style.display = 'none'; document.getElementById('out').textContent = 'Nothing'; 1",
    ]);
    let hidden = tabref.run(&["click", &peek]);
    assert!(
        hidden
            .stderr
            .starts_with("error ACTION_FAILED (element_hidden): "),
        "{}",
        hidden.stderr
    );
    assert!(
        hidden.stderr.contains("is hidden (display:none)"),
        "{}",
        hidden.stderr
    );
    untouched(&tabref);
}

#[test]
fn aria_disabled_and_fieldset_disabled_controls_are_refused() {
    if !live() {
        return;
    }
    let tabref = site();
    let archive = tabref.run(&["click", "Archive"]);
    assert!(
        archive
            .stderr
            .starts_with("error ACTION_FAILED (element_disabled): "),
        "{}",
        archive.stderr
    );
    assert!(
        archive.stderr.contains("aria-disabled"),
        "{}",
        archive.stderr
    );
    let ship = tabref.run(&["do", "click Ship"]);
    assert!(
        ship.stderr
            .starts_with("error ACTION_FAILED (element_disabled): "),
        "{}",
        ship.stderr
    );
    assert!(ship.stderr.contains("disabled fieldset"), "{}", ship.stderr);
    let size = tabref.run(&["select", "Size", "Medium"]);
    assert!(
        size.stderr
            .starts_with("error ACTION_FAILED (element_disabled): "),
        "{}",
        size.stderr
    );
    let terms = tabref.run(&["check", "Terms"]);
    assert!(
        terms
            .stderr
            .starts_with("error ACTION_FAILED (element_disabled): "),
        "{}",
        terms.stderr
    );
    untouched(&tabref);
}

#[test]
fn a_read_only_field_is_not_filled() {
    if !live() {
        return;
    }
    let tabref = site();
    let run = tabref.run(&["fill", "Code", "XYZ"]);
    assert!(
        run.stderr
            .starts_with("error ACTION_FAILED (element_readonly): "),
        "{}",
        run.stderr
    );
    assert!(run.stderr.contains("is read-only"), "{}", run.stderr);
    let value = tabref.ok(&["js", "document.querySelector('[readonly]').value"]);
    assert_eq!(value.stdout.trim(), "ABC");
}
