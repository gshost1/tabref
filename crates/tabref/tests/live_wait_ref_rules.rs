//! Live runs of `wait --ref T --state S` against the rules the action gate
//! uses: a navigation during the wait ends it as `gone` or `STALE_REF`, an
//! `aria-hidden` or `inert` section hides what it holds, a `display:
//! contents` element is shown by what it draws, `enabled` needs the element
//! shown, a name and a ref agree on `gone`, and `value=X` is the text of an
//! element that is not a field. Opt-in:
//! `TABREF_LIVE=1 cargo test -p tabref --test live_wait_ref_rules -- --test-threads=1`.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use common::{Harness, harness, live, serve};

const PAGE: &str = r#"<!doctype html><title>Rules</title>
<div id=shade><button id=shaded>Shaded</button></div>
<div id=walled><button id=walled-in>Walled</button></div>
<button id=flat style="display:contents">Flat</button>
<button id=late>Late</button>
<button id=go value=x>Go</button>
<ol role=listbox aria-label=Steps><li id=step role=option>Step one</li></ol>"#;

const NEXT: &str = "<!doctype html><title>Next</title><p>Next page</p>";

fn site(name: &str) -> (Harness, String) {
    let port = serve(&[("/rules", PAGE), ("/next", NEXT)]);
    let tabref = harness(name);
    let address = format!("http://127.0.0.1:{port}/rules");
    tabref.ok(&["open", &address]);
    (tabref, address)
}

fn later(tabref: &Harness, script: &str, ms: u32) {
    tabref.ok(&["js", &format!("setTimeout(() => {{ {script} }}, {ms}); 1")]);
}

fn now(run: &common::Run) -> String {
    let json = run.json();
    json["data"]["now"]
        .as_str()
        .or_else(|| json["error"]["details"]["now"].as_str())
        .unwrap_or_default()
        .to_owned()
}

#[test]
fn a_navigation_during_the_wait_is_gone_or_a_stale_ref() {
    if !live() {
        return;
    }
    let (tabref, address) = site("wait-ref-navigate");
    let late = tabref.ok(&["snap"]).ref_for("Late");
    later(&tabref, "location.href = '/next';", 600);
    let gone = tabref.ok(&["--json", "wait", "--ref", &late, "--state", "gone"]);
    assert_eq!(now(&gone), "gone", "{}", gone.stdout);

    tabref.ok(&["open", &address]);
    let late = tabref.ok(&["snap"]).ref_for("Late");
    later(&tabref, "location.href = '/next';", 600);
    let run = tabref.run(&[
        "--json",
        "--timeout",
        "8000",
        "wait",
        "--ref",
        &late,
        "--state",
        "value=never",
    ]);
    let error = &run.json()["error"];
    assert_eq!(error["code"], "STALE_REF", "{error}");
}

#[test]
fn aria_hidden_inert_and_display_contents_follow_the_action_gate() {
    if !live() {
        return;
    }
    let (tabref, _) = site("wait-ref-gate");
    let snap = tabref.ok(&["snap"]);
    let shaded = snap.ref_for("Shaded");
    let walled = snap.ref_for("Walled");
    let flat = snap.ref_for("Flat");
    later(
        &tabref,
        "document.getElementById('shade').setAttribute('aria-hidden', 'true');",
        300,
    );
    let hidden = tabref.ok(&[
        "--json",
        "--timeout",
        "4000",
        "wait",
        "--ref",
        &shaded,
        "--state",
        "hidden",
    ]);
    assert_eq!(now(&hidden), "hidden", "{}", hidden.stdout);
    later(
        &tabref,
        "document.getElementById('walled').inert = true;",
        300,
    );
    tabref.ok(&[
        "--timeout",
        "4000",
        "wait",
        "--ref",
        &walled,
        "--state",
        "hidden",
    ]);
    let shown = tabref.ok(&[
        "--json",
        "--timeout",
        "2000",
        "wait",
        "--ref",
        &flat,
        "--state",
        "visible",
    ]);
    assert_eq!(now(&shown), "visible", "{}", shown.stdout);
}

#[test]
fn enabled_needs_the_element_shown() {
    if !live() {
        return;
    }
    let (tabref, _) = site("wait-ref-enabled");
    let late = tabref.ok(&["snap"]).ref_for("Late");
    tabref.ok(&[
        "js",
        "document.getElementById('late').style.display = 'none'; 1",
    ]);
    let run = tabref.run(&[
        "--json",
        "--timeout",
        "1500",
        "wait",
        "--ref",
        &late,
        "--state",
        "enabled",
    ]);
    let error = &run.json()["error"];
    assert_eq!(error["code"], "TIMEOUT", "{error}");
    assert_eq!(error["details"]["now"], "hidden", "{error}");
}

#[test]
fn a_name_and_a_ref_agree_on_gone() {
    if !live() {
        return;
    }
    let (tabref, _) = site("wait-ref-gone");
    let late = tabref.ok(&["snap"]).ref_for("Late");
    tabref.ok(&[
        "js",
        "document.getElementById('late').style.display = 'none'; 1",
    ]);
    let by_ref = tabref.ok(&[
        "--json",
        "--timeout",
        "2000",
        "wait",
        "--ref",
        &late,
        "--state",
        "gone",
    ]);
    assert_eq!(now(&by_ref), "hidden", "{}", by_ref.stdout);
    let by_name = tabref.ok(&[
        "--json",
        "--timeout",
        "2000",
        "wait",
        "--ref",
        "Late",
        "--state",
        "hidden",
    ]);
    assert_eq!(now(&by_name), "gone", "{}", by_name.stdout);
}

#[test]
fn a_value_is_the_text_of_an_element_that_is_not_a_field() {
    if !live() {
        return;
    }
    let (tabref, _) = site("wait-ref-value");
    tabref.ok(&[
        "--timeout",
        "2000",
        "wait",
        "--ref",
        "Go",
        "--state",
        "value=Go",
    ]);
    tabref.ok(&[
        "--timeout",
        "2000",
        "wait",
        "--ref",
        "Step one",
        "--state",
        "value=\"Step one\"",
    ]);
}
