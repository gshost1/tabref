//! Live regressions for `type` on the fields it must refuse or append to
//! correctly: a ref that is not a text field never writes into the focused
//! one, an email input appends at its end, a number input points at
//! `fill`, a truncated append is not called verified while a space an
//! editor stores as a non-breaking space is, and a long field prints only
//! its tail. Opt-in:
//! `TABREF_LIVE=1 cargo test -p tabref --test live_type_fields -- --test-threads=1`.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use common::{Harness, harness, live, serve};

const PAGE: &str = r#"<!doctype html><title>Fields</title>
<label>Other <input id=other value="mine"></label>
<label><input type=checkbox id=agree> Agree</label>
<button id=go>Go</button>
<label>Size <select id=size><option>S</option><option>M</option></select></label>
<a href="/elsewhere" id=away>Away</a>
<label>Email <input type=email id=email value="a@b"></label>
<label>Count <input type=number id=count value="12"></label>
<label>Short <input id=short maxlength=5 value="Hello"></label>
<div contenteditable=true role=textbox aria-label="Notes" id=notes>Dear</div>
<label>Story <textarea id=story>Once</textarea></label>
<label>Essay <textarea id=essay></textarea></label>
<script>document.getElementById('essay').value = 'x'.repeat(2990) + 'the long end';</script>"#;

fn site() -> Harness {
    let port = serve(&[("/fields", PAGE)]);
    let tabref = harness("type-fields");
    tabref.ok(&["open", &format!("http://127.0.0.1:{port}/fields")]);
    tabref
}

fn js(tabref: &Harness, expression: &str) -> String {
    tabref.ok(&["js", expression]).stdout.trim().to_owned()
}

#[test]
fn a_ref_that_is_not_a_text_field_is_refused_and_the_focused_field_keeps_its_text() {
    if !live() {
        return;
    }
    let tabref = site();
    let snap = tabref.ok(&["snap"]);
    for needle in ["checkbox Agree", "button Go", "select Size", "Away"] {
        let target = snap.ref_for(needle);
        tabref.ok(&["js", "document.getElementById('other').focus(); 1"]);
        let run = tabref.run(&["type", &target, "LEAK"]);
        assert!(
            run.stderr
                .starts_with("error ACTION_NOT_SUPPORTED (cdp_not_editable): "),
            "{needle}: {}{}",
            run.stdout,
            run.stderr
        );
        assert_eq!(
            js(&tabref, "document.getElementById('other').value"),
            "mine"
        );
    }
}

#[test]
fn an_email_input_appends_at_its_end_and_a_number_input_points_at_fill() {
    if !live() {
        return;
    }
    let tabref = site();
    tabref.ok(&["type", "Email", ".com"]);
    assert_eq!(
        js(&tabref, "document.getElementById('email').value"),
        "a@b.com"
    );
    let count = tabref.run(&["type", "Count", "3"]);
    assert!(
        count
            .stderr
            .starts_with("error ACTION_NOT_SUPPORTED (cdp_whole_value_input): type adds text to a text field; use fill for a number/date input"),
        "{}",
        count.stderr
    );
    assert_eq!(js(&tabref, "document.getElementById('count').value"), "12");
}

#[test]
fn only_an_append_that_landed_whole_is_verified() {
    if !live() {
        return;
    }
    let tabref = site();
    let short = tabref.ok(&["--json", "type", "Short", "o"]).json();
    assert_eq!(short["data"]["verified"], false, "{short}");
    let notes = tabref.ok(&["--json", "type", "Notes", " Ada"]).json();
    assert_eq!(notes["data"]["verified"], true, "{notes}");
    let story = tabref.ok(&["--json", "type", "Story", " a\n"]).json();
    assert_eq!(story["data"]["verified"], true, "{story}");
}

#[test]
fn a_long_field_prints_only_the_tail_of_its_text() {
    if !live() {
        return;
    }
    let tabref = site();
    let run = tabref.ok(&["type", "Essay", "!"]);
    let first = run.stdout.lines().next().unwrap();
    assert!(first.len() < 200, "{first}");
    assert!(first.contains("the long end!\""), "{first}");
    assert!(first.contains('…'), "{first}");
    assert!(
        js(
            &tabref,
            "document.getElementById('essay').value.endsWith('the long end!')"
        ) == "true"
    );
}
