//! Opt-in: TABREF_LIVE=1 cargo test -p tabref --test live_capture_safety.
#![cfg(unix)]
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use std::process::Command;

use common::{Harness, Run};
use serde_json::{Value, json};
use tabref_cdp::{attach_to_target, connect, list_page_targets};
use tabref_core::Deadline;

const ANIMATED: &str = r#"<button id=a onclick="picked='a'">Remove draft</button>
<button id=b style="visibility:hidden" onclick="picked='b'">Remove</button>"#;
const ANIMATE: &str = "b.animate([{visibility:'visible'},{visibility:'visible'}],{duration:1,fill:'forwards'}).finished.then(()=>true)";
const VALIDITY: &str = r#"<style>form:valid .fix{display:none}</style><form>
<input id=field aria-label=Name><button type=button onclick="picked='a'">Send now</button>
<button class=fix type=button onclick="picked='b'">Send</button></form>"#;
const INVALID: &str = "field.setCustomValidity('bad')";
const INTERNALS: &str = r#"<style>x-action{display:block;width:120px;height:30px}</style>
<x-action id=a tabindex=0 onclick="picked='a'"></x-action>
<x-action id=b tabindex=0 onclick="picked='b'"></x-action><script>
customElements.define('x-action',class extends HTMLElement {
constructor(){super();this.ins=this.attachInternals();this.ins.role='button';}});
a.ins.ariaLabel='Delete';b.ins.ariaLabel='Keep';</script>"#;
const SWAP: &str = "a.ins.ariaLabel='Keep';b.ins.ariaLabel='Delete'";
const STATES: &str = r#"<style>button{width:160px;height:30px}.fix::before{content:''}x-state:state(fixed)~.fix::before{content:'Do not '}</style>
<x-state id=state></x-state><button onclick="picked='a'">Delete draft</button>
<button class=fix onclick="picked='b'">Delete</button><script>
customElements.define('x-state',class extends HTMLElement {
constructor(){super();this.ins=this.attachInternals();this.ins.states.add('fixed');}});</script>"#;

fn run(harness: &Harness, args: &[&str], fresh: bool) -> Run {
    let mut command = Command::new(env!("CARGO_BIN_EXE_tabref"));
    command
        .args([
            "--cdp",
            &harness.browser.port.to_string(),
            "--timeout",
            "30000",
        ])
        .args(args)
        .env("TABREF_HOME", &harness.home)
        .env_remove("TABREF_NO_DAEMON");
    if fresh {
        command.env("TABREF_NO_DAEMON", "1");
    }
    let output = command.output().unwrap();
    let result = Run {
        ok: output.status.success(),
        stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
        stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
    };
    assert!(result.ok, "{args:?}: {}{}", result.stdout, result.stderr);
    result
}

fn evaluate(harness: &Harness, expression: &str) -> Value {
    let deadline = Deadline::after(30_000).unwrap();
    let mut connection = connect(&harness.browser.endpoint().unwrap(), deadline).unwrap();
    let target = list_page_targets(&mut connection, deadline)
        .unwrap()
        .into_iter()
        .find(|target| target.url.starts_with("http://127.0.0.1:"))
        .unwrap();
    let page = attach_to_target(&mut connection, &target.target_id, deadline).unwrap();
    let reply = connection
        .call(
            "Runtime.evaluate",
            json!({"expression":expression,"awaitPromise":true,"returnByValue":true}),
            Some(&page.session_id),
            deadline,
        )
        .unwrap();
    assert!(reply.get("exceptionDetails").is_none(), "{reply}");
    reply["result"]["value"].clone()
}

fn setup(html: &str, large: bool, edit: &str) -> Harness {
    let harness = common::harness("capture-safety");
    let html = format!(
        "<!doctype html><html><head><title>Safety</title></head><body>{html}{}{}</body></html>",
        if large {
            "<span></span>".repeat(1_500)
        } else {
            String::new()
        },
        "<script>window.picked='';</script>"
    );
    let port = common::serve(&[("/", &html)]);
    run(
        &harness,
        &["open", &format!("http://127.0.0.1:{port}/")],
        false,
    );
    run(&harness, &["snap"], false);
    evaluate(&harness, edit);
    harness
}

fn compare_click(html: &str, large: bool, edit: &str, locator: &str) {
    let harness = setup(html, large, edit);
    run(&harness, &["click", locator], false);
    let reused = evaluate(&harness, "picked");
    evaluate(&harness, "picked=''");
    run(&harness, &["click", locator], true);
    let fresh = evaluate(&harness, "picked");
    run(&harness, &["daemon", "stop"], false);
    assert_eq!(fresh, "b", "fresh baseline did not pick the exact match");
    assert_eq!(reused, fresh, "reuse selected a different element");
}

fn compare_view(html: &str, large: bool, edit: &str, command: &str, needle: &str) {
    let harness = setup(html, large, edit);
    let reused = run(&harness, &[command], false);
    let fresh = run(&harness, &[command], true);
    run(&harness, &["daemon", "stop"], false);
    if command == "read" {
        assert_eq!(reused.stdout, fresh.stdout, "reuse printed a stale read");
    } else {
        assert!(
            fresh.stdout.contains(needle),
            "fresh baseline: {}",
            fresh.stdout
        );
        assert!(
            reused.stdout.contains(needle),
            "reuse printed a stale snap: {}",
            reused.stdout
        );
    }
}

#[test]
fn finished_fill_animation_click_matches_fresh() {
    if common::live() {
        compare_click(ANIMATED, true, ANIMATE, "Remove");
    }
}

#[test]
fn finished_fill_animation_snap_matches_fresh() {
    if common::live() {
        compare_view(ANIMATED, true, ANIMATE, "snap", "button Remove]");
    }
}

#[test]
fn finished_fill_animation_read_matches_fresh() {
    if common::live() {
        compare_view(ANIMATED, true, ANIMATE, "read", "");
    }
}

#[test]
fn custom_validity_click_matches_fresh() {
    if common::live() {
        compare_click(VALIDITY, true, INVALID, "Send");
    }
}

#[test]
fn custom_validity_snap_matches_fresh() {
    if common::live() {
        compare_view(VALIDITY, true, INVALID, "snap", "button Send]");
    }
}

#[test]
fn custom_validity_read_matches_fresh() {
    if common::live() {
        compare_view(VALIDITY, true, INVALID, "read", "");
    }
}

#[test]
fn internals_labels_click_matches_fresh() {
    if common::live() {
        compare_click(INTERNALS, false, SWAP, "Delete");
    }
}

#[test]
fn internals_labels_snap_matches_fresh() {
    if !common::live() {
        return;
    }
    let harness = setup(INTERNALS, false, SWAP);
    let reused = run(&harness, &["snap"], false);
    let fresh = run(&harness, &["snap"], true);
    run(&harness, &["daemon", "stop"], false);
    let labels = |run: &Run| {
        run.stdout.find("button Delete").unwrap() < run.stdout.find("button Keep").unwrap()
    };
    assert_eq!(
        labels(&reused),
        labels(&fresh),
        "internals labels were stale"
    );
}

#[test]
fn internals_states_click_matches_fresh() {
    if common::live() {
        compare_click(STATES, false, "state.ins.states.delete('fixed')", "Delete");
    }
}

#[test]
fn internals_states_snap_matches_fresh() {
    if common::live() {
        compare_view(
            STATES,
            false,
            "state.ins.states.delete('fixed')",
            "snap",
            "button Delete]",
        );
    }
}

#[test]
fn a_batch_never_reuses_the_pre_batch_capture() {
    if !common::live() {
        return;
    }
    let html = format!(
        "{VALIDITY}<button id=toggle onmousedown=\"event.preventDefault()\" onclick=\"field.setCustomValidity('bad')\">Toggle</button>"
    );
    let harness = setup(&html, true, "true");
    run(&harness, &["hover", "Toggle"], false);
    run(&harness, &["snap"], false);
    let report = run(&harness, &["do", "click Toggle; click Send"], false);
    let picked = evaluate(&harness, "picked");
    run(&harness, &["daemon", "stop"], false);
    assert_eq!(
        picked, "b",
        "batch reused a pre-action capture: {}",
        report.stdout
    );
}

#[test]
fn wait_for_uses_the_current_validity_state() {
    if !common::live() {
        return;
    }
    let harness = setup(VALIDITY, true, INVALID);
    let reused = run(&harness, &["wait", "--text", "Send"], false);
    let fresh = run(&harness, &["wait", "--text", "Send"], true);
    run(&harness, &["daemon", "stop"], false);
    assert!(fresh.stdout.contains("button \"Send\""), "{}", fresh.stdout);
    assert!(
        reused.stdout.contains("button \"Send\""),
        "stale wait: {}",
        reused.stdout
    );
}
