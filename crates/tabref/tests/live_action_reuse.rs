//! Opt-in: TABREF_LIVE=1 cargo test -p tabref --test live_action_reuse.
#![cfg(unix)]
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;
#[path = "live_action_reuse/proxy.rs"]
mod proxy;

use std::process::Command;

use common::{Harness, Run, live, serve};
use serde_json::{Value, json};
use tabref_cdp::{attach_to_target, connect, list_page_targets};
use tabref_core::Deadline;
use tabref_page::{arm_capture_watch, check_capture_watch};

const LARGE: &str = include_str!("fixtures/action-reuse.html");

fn run(tabref: &Harness, proxy: &proxy::Proxy, args: &[&str], local: bool) -> Run {
    let mut command = Command::new(env!("CARGO_BIN_EXE_tabref"));
    command
        .args(["--cdp", &proxy.endpoint, "--json", "--timeout", "30000"])
        .args(args)
        .env("TABREF_HOME", &tabref.home)
        .env_remove("TABREF_NO_DAEMON");
    if local {
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

fn edit(tabref: &Harness, expression: &str) {
    let deadline = Deadline::after(30_000).unwrap();
    let mut connection = connect(&tabref.browser.endpoint().unwrap(), deadline).unwrap();
    let target = list_page_targets(&mut connection, deadline)
        .unwrap()
        .into_iter()
        .find(|target| target.url.starts_with("http://127.0.0.1:"))
        .unwrap();
    let page = attach_to_target(&mut connection, &target.target_id, deadline).unwrap();
    let reply = connection
        .call(
            "Runtime.evaluate",
            json!({ "expression": expression, "returnByValue": true }),
            Some(&page.session_id),
            deadline,
        )
        .unwrap();
    assert!(reply.get("exceptionDetails").is_none(), "{reply}");
}

fn site(name: &str) -> (Harness, proxy::Proxy, String) {
    let tabref = common::harness(name);
    let proxy = proxy::Proxy::new(&tabref.browser);
    let port = serve(&[("/", LARGE), ("/away", LARGE)]);
    let url = format!("http://127.0.0.1:{port}/");
    (tabref, proxy, url)
}

#[test]
fn an_unchanged_large_page_always_captures_for_resolve_and_report() {
    if !live() {
        return;
    }
    let (tabref, proxy, url) = site("action-reuse-still");
    run(&tabref, &proxy, &["open", &url], false);
    run(&tabref, &proxy, &["snap"], false);
    let before = proxy.captures();
    run(&tabref, &proxy, &["click", "Go"], false);
    assert_eq!(
        proxy.captures() - before,
        2,
        "action did not capture fresh for both resolve and report"
    );
    let before = proxy.captures();
    run(&tabref, &proxy, &["snap"], false);
    assert_eq!(
        proxy.captures() - before,
        1,
        "kept a capture after an action"
    );
    run(&tabref, &proxy, &["daemon", "stop"], false);
}

#[test]
fn changes_between_snap_and_click_force_a_fresh_resolve() {
    if !live() {
        return;
    }
    let (tabref, proxy, url) = site("action-reuse-changed");
    for change in [
        "document.getElementById('out').textContent='edited'",
        "location.href='/away'",
        "const f=document.createElement('iframe'); f.srcdoc='<button>Inner</button>'; document.body.append(f)",
    ] {
        run(&tabref, &proxy, &["open", &url], false);
        run(&tabref, &proxy, &["snap"], false);
        edit(&tabref, change);
        let before = proxy.captures();
        run(&tabref, &proxy, &["click", "Noop"], false);
        assert!(proxy.captures() > before, "reused after {change}");
    }
    run(&tabref, &proxy, &["daemon", "stop"], false);
}

fn report(mut value: Value) -> Value {
    let data = value["data"].as_object_mut().unwrap();
    data.remove("point");
    if let Some(controls) = data.get_mut("new").and_then(Value::as_array_mut) {
        for control in controls {
            let text = control.as_str().unwrap();
            let start = text.find(" button ").unwrap();
            *control = Value::String(format!("[{}", &text[start + 1..]));
        }
    }
    value["data"].clone()
}

#[test]
fn daemon_and_in_process_actions_pick_the_same_button_and_report_the_same_changes() {
    if !live() {
        return;
    }
    let (tabref, proxy, url) = site("action-reuse-report");
    let mut reports = Vec::new();
    for local in [false, true] {
        run(&tabref, &proxy, &["open", &url], local);
        run(&tabref, &proxy, &["snap"], local);
        let clicked = run(&tabref, &proxy, &["click", "Go"], local);
        assert!(clicked.stdout.contains("Undo"), "{}", clicked.stdout);
        assert_eq!(clicked.json()["data"]["target"], "\"Go\"");
        assert!(!clicked.stdout.contains("wrong"), "{}", clicked.stdout);
        reports.push(report(clicked.json()));
    }
    assert_eq!(reports[0], reports[1]);
    run(&tabref, &proxy, &["daemon", "stop"], false);
}

#[test]
fn empty_and_ambiguous_locators_still_fail_without_dispatch() {
    if !live() {
        return;
    }
    let (tabref, proxy, url) = site("action-reuse-errors");
    run(&tabref, &proxy, &["open", &url], false);
    for (target, code) in [
        ("Missing", "ELEMENT_NOT_FOUND"),
        ("Duplicate", "AMBIGUOUS_TARGET"),
    ] {
        let output = Command::new(env!("CARGO_BIN_EXE_tabref"))
            .args(["--cdp", &proxy.endpoint, "--json", "click", target])
            .env("TABREF_HOME", &tabref.home)
            .env_remove("TABREF_NO_DAEMON")
            .output()
            .unwrap();
        assert!(!output.status.success());
        let value: Value = serde_json::from_slice(&output.stdout).unwrap();
        assert_eq!(value["error"]["code"], code);
    }
    run(&tabref, &proxy, &["daemon", "stop"], false);
}

#[test]
fn large_pages_notice_stylesheet_changes_without_dom_mutations() {
    if !live() {
        return;
    }
    let (tabref, proxy, url) = site("action-reuse-css");
    run(&tabref, &proxy, &["open", &url], false);
    edit(
        &tabref,
        "const style=document.createElement('style'); style.textContent='#go { visibility:visible }'; document.head.append(style)",
    );
    let deadline = Deadline::after(30_000).unwrap();
    let mut connection = connect(&tabref.browser.endpoint().unwrap(), deadline).unwrap();
    let target = list_page_targets(&mut connection, deadline)
        .unwrap()
        .into_iter()
        .find(|target| target.url == url)
        .unwrap();
    let page = attach_to_target(&mut connection, &target.target_id, deadline).unwrap();
    for expression in [
        "document.styleSheets[0].insertRule('#go { visibility:hidden }', 1)",
        "document.styleSheets[0].cssRules[1].style.visibility='visible'",
    ] {
        let watch = arm_capture_watch(&mut connection, &page, "css", deadline).unwrap();
        assert!(!watch.blind);
        assert_eq!(
            check_capture_watch(&mut connection, &page, &watch, deadline),
            "same"
        );
        edit(&tabref, expression);
        let verdict = check_capture_watch(&mut connection, &page, &watch, deadline);
        assert!(
            matches!(verdict.as_str(), "state" | "mutated" | "blind"),
            "{verdict}"
        );
    }
    run(&tabref, &proxy, &["daemon", "stop"], false);
}
