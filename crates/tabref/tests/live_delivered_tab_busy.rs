//! A click that opened a tab another daemon command holds was still
//! delivered: it is never reported as not delivered and safe to retry.
#![cfg(unix)]
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use std::process::{Child, Command, Output, Stdio};
use std::thread;
use std::time::{Duration, Instant};

use common::{Harness, harness, live, serve};
use serde_json::Value;
use tabref_cdp::{connect, endpoint_from_port, list_page_targets};
use tabref_core::Deadline;

const START: &str = r#"<!doctype html><title>Start</title><h1>Start</h1>
<button onclick="window.open('/other'); const t = Date.now(); while (Date.now() - t < 2500) {}">Pop</button>"#;

const OTHER: &str = "<!doctype html><title>Other</title><h1>Other</h1>";

fn command(tabref: &Harness, args: &[&str]) -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_tabref"));
    command
        .args(["--cdp", &tabref.browser.port.to_string()])
        .args(args)
        .env("TABREF_HOME", &tabref.home)
        .env_remove("TABREF_NO_DAEMON")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    command
}

fn page_tabs(tabref: &Harness) -> Vec<String> {
    let deadline = Deadline::after(10_000).unwrap();
    let endpoint = endpoint_from_port(tabref.browser.port, deadline).unwrap();
    let mut browser = connect(&endpoint, deadline).unwrap();
    let targets = list_page_targets(&mut browser, deadline).unwrap();
    targets.into_iter().map(|tab| tab.target_id).collect()
}

fn opened_tab(tabref: &Harness, before: &[String]) -> String {
    let started = Instant::now();
    while started.elapsed() < Duration::from_secs(5) {
        if let Some(tab) = page_tabs(tabref)
            .into_iter()
            .find(|tab| !before.contains(tab))
        {
            return tab;
        }
        thread::sleep(Duration::from_millis(20));
    }
    panic!("the click opened no tab");
}

/// Runs `args` while another call holds the tab the click opens.
fn with_new_tab_held(name: &str, args: &[&str]) -> Value {
    let port = serve(&[("/", START), ("/other", OTHER)]);
    let tabref = harness(name);
    let output = command(&tabref, &["open", &format!("http://127.0.0.1:{port}/")])
        .output()
        .unwrap();
    assert!(output.status.success(), "{output:?}");
    let before = page_tabs(&tabref);
    let acting = command(&tabref, args).spawn().unwrap();
    let tab = opened_tab(&tabref, &before);
    let mut holder: Child = command(
        &tabref,
        &[
            "--tab",
            &tab,
            "--timeout",
            "9000",
            "wait",
            "--text",
            "never shown",
        ],
    )
    .spawn()
    .unwrap();
    let output: Output = acting.wait_with_output().unwrap();
    let _ = holder.kill();
    let _ = holder.wait();
    let text = String::from_utf8_lossy(&output.stdout);
    serde_json::from_str(text.trim()).unwrap_or_else(|_| {
        panic!(
            "not JSON: {text}\n{}",
            String::from_utf8_lossy(&output.stderr)
        )
    })
}

fn delivered(envelope: &Value) {
    let error = &envelope["error"];
    assert_eq!(envelope["ok"], false, "{envelope}");
    assert_ne!(
        error["disposition"]["delivery"], "not_delivered",
        "{envelope}"
    );
    assert_ne!(error["disposition"]["retry"], "safe", "{envelope}");
    assert!(error.get("recovery").is_none(), "{envelope}");
    let message = error["message"].as_str().unwrap();
    assert!(message.contains("was delivered"), "{envelope}");
    let said = error["details"]["report_error"].as_str().unwrap();
    assert!(
        said.starts_with("could not read what the action set off ("),
        "{envelope}"
    );
    assert!(said.ends_with("; run tabref snap to see it"), "{envelope}");
}

#[test]
fn a_click_whose_new_tab_is_held_elsewhere_is_never_retry_safe() {
    if !live() {
        return;
    }
    let envelope = with_new_tab_held(
        "delivered-busy-click",
        &["--json", "--timeout", "5000", "click", "Pop"],
    );
    delivered(&envelope);
}

#[test]
fn a_do_step_whose_new_tab_is_held_elsewhere_is_never_retry_safe() {
    if !live() {
        return;
    }
    let envelope = with_new_tab_held(
        "delivered-busy-do",
        &["--json", "--timeout", "5000", "do", "click \"Pop\"\njs 1"],
    );
    delivered(&envelope);
    assert_eq!(envelope["error"]["details"]["failed_step"], 1, "{envelope}");
}
