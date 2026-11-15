//! Live runs of `type` (append text without clearing) and `double-click`,
//! as single commands and as `do` steps, through the actionability gate.
//! Opt-in:
//! `TABREF_LIVE=1 cargo test -p tabref --test live_type_click -- --test-threads=1`.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use common::{Harness, harness, live, serve};

const PAGE: &str = r#"<!doctype html><title>Typing</title>
<script>function out(text) { document.getElementById('out').textContent = text; }</script>
<label>Greeting <input id=greeting value="Hello"></label>
<label>Locked <input readonly value="fixed"></label>
<div contenteditable=true role=textbox aria-label="Notes" id=notes>Dear</div>
<button id=twice onclick="this.dataset.clicks = Number(this.dataset.clicks || 0) + 1" ondblclick="out('double ' + this.dataset.clicks)">Twice</button>
<button disabled ondblclick="out('off')">Off</button>
<p id=out>Nothing</p>"#;

fn site() -> Harness {
    let port = serve(&[("/typing", PAGE)]);
    let tabref = harness("type-click");
    tabref.ok(&["open", &format!("http://127.0.0.1:{port}/typing")]);
    tabref
}

fn js(tabref: &Harness, expression: &str) -> String {
    tabref.ok(&["js", expression]).stdout.trim().to_owned()
}

#[test]
fn type_appends_to_what_a_field_holds_and_reads_it_back() {
    if !live() {
        return;
    }
    let tabref = site();
    let typed = tabref.ok(&["type", "Greeting", " world"]);
    assert_eq!(
        typed.stdout.lines().next().unwrap(),
        r#"typed "Greeting" "Hello" -> "Hello world""#
    );
    assert_eq!(
        js(&tabref, "document.getElementById('greeting').value"),
        "Hello world"
    );
    tabref.ok(&["type", "Notes", " Ada"]);
    assert_eq!(
        js(&tabref, "document.getElementById('notes').textContent"),
        "Dear Ada"
    );
}

#[test]
fn a_type_step_appends_and_a_fill_step_still_replaces() {
    if !live() {
        return;
    }
    let tabref = site();
    let run = tabref.ok(&[
        "do",
        "type Greeting \"!\"; fill Greeting Hi; type Greeting there",
    ]);
    let lines: Vec<&str> = run.stdout.lines().collect();
    assert_eq!(
        lines[0], r#"ok type "Greeting" "Hello" -> "Hello!""#,
        "{}",
        run.stdout
    );
    assert_eq!(
        lines[1], r#"ok fill "Greeting" "Hello!" -> "Hi""#,
        "{}",
        run.stdout
    );
    assert!(
        lines[2].starts_with(r#"ok type "Greeting" "Hi" -> "Hithere""#),
        "{}",
        run.stdout
    );
}

#[test]
fn type_into_a_read_only_field_is_refused_by_the_gate() {
    if !live() {
        return;
    }
    let tabref = site();
    let run = tabref.run(&["type", "Locked", "x"]);
    assert!(
        run.stderr
            .starts_with("error ACTION_FAILED (element_readonly): "),
        "{}",
        run.stderr
    );
}

#[test]
fn double_click_fires_the_double_click_once_after_two_presses() {
    if !live() {
        return;
    }
    let tabref = site();
    let run = tabref.ok(&["double-click", "Twice"]);
    assert!(
        run.stdout.starts_with(r#"double-clicked "Twice""#),
        "{}",
        run.stdout
    );
    assert_eq!(
        js(&tabref, "document.getElementById('out').textContent"),
        "double 2"
    );
    let step = tabref.ok(&["do", "double-click Twice"]);
    assert!(
        step.stdout.starts_with(r#"ok double-click "Twice""#),
        "{}",
        step.stdout
    );
    assert_eq!(
        js(&tabref, "document.getElementById('out').textContent"),
        "double 4"
    );
}

#[test]
fn double_click_on_a_disabled_button_is_refused_by_the_gate() {
    if !live() {
        return;
    }
    let tabref = site();
    let run = tabref.run(&["double-click", "Off"]);
    assert!(
        run.stderr
            .starts_with("error ACTION_FAILED (element_disabled): "),
        "{}",
        run.stderr
    );
    assert_eq!(
        js(&tabref, "document.getElementById('out').textContent"),
        "Nothing"
    );
}
