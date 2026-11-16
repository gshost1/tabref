//! Live runs of `wait --ref T --state S` and the `expect T is S` step:
//! each state the page reaches later ends the wait at once, a state already
//! held returns at once, and a state never reached is a TIMEOUT that says
//! what the element is instead. Opt-in:
//! `TABREF_LIVE=1 cargo test -p tabref --test live_wait_ref -- --test-threads=1`.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use std::time::Instant;

use common::{Harness, harness, live, serve};

const PAGE: &str = r#"<!doctype html><title>States</title>
<label>Price <input id=price value="67.00"></label>
<button id=save disabled>Save</button>
<button id=banner>Dismiss banner</button>
<div id=panel hidden><button>Panel action</button></div>
<p id=out>Nothing</p>"#;

fn site() -> Harness {
    let port = serve(&[("/states", PAGE)]);
    let tabref = harness("wait-ref");
    tabref.ok(&["open", &format!("http://127.0.0.1:{port}/states")]);
    tabref
}

fn later(tabref: &Harness, script: &str) {
    tabref.ok(&["js", &format!("setTimeout(() => {{ {script} }}, 400); 1")]);
}

#[test]
fn each_state_the_page_reaches_later_ends_the_wait() {
    if !live() {
        return;
    }
    let tabref = site();
    later(&tabref, "document.getElementById('save').disabled = false;");
    let enabled = tabref.ok(&["wait", "--ref", "Save", "--state", "enabled"]);
    assert!(
        enabled.stdout.starts_with("ok after "),
        "{}",
        enabled.stdout
    );
    assert!(
        enabled.stdout.contains("to be enabled"),
        "{}",
        enabled.stdout
    );

    later(&tabref, "document.getElementById('price').value = '62.00';");
    tabref.ok(&["wait", "--ref", "Price", "--state", "value=62.00"]);

    let banner = tabref.ok(&["snap"]).ref_for("Dismiss banner");
    later(
        &tabref,
        "document.getElementById('banner').style.display = 'none';",
    );
    tabref.ok(&["wait", "--ref", &banner, "--state", "hidden"]);
    later(&tabref, "document.getElementById('banner').remove();");
    tabref.ok(&["wait", "--ref", &banner, "--state", "gone"]);

    let started = Instant::now();
    tabref.ok(&["wait", "--ref", "Price", "--state", "visible"]);
    assert!(started.elapsed().as_millis() < 3_000);
}

#[test]
fn a_state_never_reached_times_out_saying_what_the_element_is() {
    if !live() {
        return;
    }
    let tabref = site();
    let run = tabref.run(&[
        "--json",
        "--timeout",
        "1000",
        "wait",
        "--ref",
        "Save",
        "--state",
        "enabled",
    ]);
    let error = &run.json()["error"];
    assert_eq!(error["code"], "TIMEOUT", "{error}");
    assert_eq!(error["details"]["kind"], "wait_timeout", "{error}");
    assert_eq!(error["details"]["now"], "disabled", "{error}");
    let bad = tabref.run(&["wait", "--ref", "Save", "--state", "shiny"]);
    assert!(
        bad.stderr.starts_with("error INVALID_ARGS"),
        "{}",
        bad.stderr
    );
}

#[test]
fn an_expect_step_waits_for_an_element_state() {
    if !live() {
        return;
    }
    let tabref = site();
    let run = tabref.ok(&[
        "do",
        "js \"setTimeout(() => { document.getElementById('save').disabled = false; }, 300); 1\"; expect \"Save\" is enabled; click Save",
    ]);
    let lines: Vec<&str> = run.stdout.lines().collect();
    assert!(
        lines[1].starts_with("ok expect \"Save\" is enabled"),
        "{}",
        run.stdout
    );
    assert!(lines[2].starts_with("ok click \"Save\""), "{}", run.stdout);
}
