//! A delivered click or key press stays delivered when a read after it fails.
#![cfg(unix)]
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use common::{Harness, harness, live, serve};
use serde_json::Value;

const START: &str = r#"<!doctype html><title>Start</title><h1>Start</h1>
<button onclick="window.open('/other')">Pop</button>
<a href="/other">Leave</a>
<button onclick="location.reload()">Reload</button>
<button onclick="setTimeout(() => { location = '/gone' }, 20)">Later</button>
<input aria-label="Search" onkeydown="if (event.key === 'Enter') location = '/other'">"#;

const OTHER: &str = "<!doctype html><title>Other</title><h1>Other</h1>";

const GONE: &str = "<!doctype html><title>Gone</title><script>window.close()</script>";

fn site(name: &str) -> (Harness, String) {
    let port = serve(&[("/", START), ("/other", OTHER), ("/gone", GONE)]);
    let tabref = harness(name);
    let base = format!("http://127.0.0.1:{port}");
    tabref.ok(&["open", &format!("{base}/")]);
    (tabref, base)
}

fn unreadable(name: &str) -> Harness {
    let (tabref, _) = site(name);
    let counter = tabref.home.join("refs-next");
    std::fs::remove_file(&counter).unwrap();
    std::fs::create_dir(&counter).unwrap();
    tabref
}

fn note(text: &str, what: &str) -> String {
    let line = text
        .lines()
        .find(|line| line.starts_with("note: could not read"))
        .unwrap_or_else(|| panic!("no unread note in:\n{text}"));
    assert!(
        line.starts_with(&format!("note: could not read {what} (")),
        "{text}"
    );
    assert!(line.ends_with("; run tabref snap to see it"), "{text}");
    line.to_owned()
}

fn only_step(batch: &Value) -> &Value {
    assert_eq!(batch["ok"], true, "{batch}");
    let steps = batch["data"]["steps"].as_array().unwrap();
    assert_eq!(steps.len(), 1, "{batch}");
    &steps[0]
}

fn reported(step: &Value, what: &str) {
    let text = step["text"].as_str().unwrap();
    let line = note(text, what);
    assert_eq!(
        step["data"]["report_error"].as_str(),
        line.strip_prefix("note: "),
        "{step}"
    );
    assert!(step["data"].get("page").is_none(), "{step}");
}

#[test]
fn an_unreadable_new_tab_keeps_the_click_delivered() {
    if !live() {
        return;
    }
    let tabref = unreadable("delivered-new-tab");
    let clicked = tabref.ok(&["click", "Pop"]);
    assert!(
        clicked.stdout.starts_with("clicked \"Pop\"\n"),
        "{}",
        clicked.stdout
    );
    assert!(
        clicked
            .stdout
            .contains("opened a new tab, now the current tab"),
        "{}",
        clicked.stdout
    );
    note(&clicked.stdout, "the new tab");
    let tabref = unreadable("delivered-new-tab-do");
    let batch = tabref.ok(&["--json", "do", "click \"Pop\""]).json();
    let step = only_step(&batch);
    assert!(
        step["text"]
            .as_str()
            .unwrap()
            .starts_with("ok click \"Pop\""),
        "{batch}"
    );
    assert!(step["data"]["new_tab"].is_string(), "{batch}");
    reported(step, "the new tab");
}

#[test]
fn an_unreadable_page_after_a_navigation_keeps_the_action_delivered() {
    if !live() {
        return;
    }
    let tabref = unreadable("delivered-navigation");
    let clicked = tabref.ok(&["click", "Leave"]);
    assert!(
        clicked.stdout.starts_with("clicked \"Leave\""),
        "{}",
        clicked.stdout
    );
    note(&clicked.stdout, "the page");
    let tabref = unreadable("delivered-navigation-press");
    let pressed = tabref.ok(&["press", "Enter", "Search"]);
    assert!(
        pressed.stdout.starts_with("pressed Enter in"),
        "{}",
        pressed.stdout
    );
    note(&pressed.stdout, "the page");
    let tabref = unreadable("delivered-navigation-do");
    let batch = tabref.ok(&["--json", "do", "click \"Leave\""]).json();
    reported(only_step(&batch), "the page");
    let tabref = unreadable("delivered-navigation-continues");
    let batch = tabref
        .ok(&["--json", "do", "click \"Leave\"\njs document.title"])
        .json();
    let steps = batch["data"]["steps"].as_array().unwrap();
    assert_eq!(steps.len(), 2, "{batch}");
    assert_eq!(steps[1]["data"]["value"], "Other", "{batch}");
}

#[test]
fn an_unreadable_page_after_a_reload_keeps_the_click_delivered() {
    if !live() {
        return;
    }
    let tabref = unreadable("delivered-reload");
    let clicked = tabref.ok(&["click", "Reload"]);
    assert!(
        clicked.stdout.starts_with("clicked \"Reload\""),
        "{}",
        clicked.stdout
    );
    note(&clicked.stdout, "the page");
    let tabref = unreadable("delivered-reload-do");
    let batch = tabref.ok(&["--json", "do", "click \"Reload\""]).json();
    reported(only_step(&batch), "the page");
}

#[test]
fn a_late_navigation_that_closes_the_tab_keeps_the_step_delivered_and_the_batch_going() {
    if !live() {
        return;
    }
    let (tabref, base) = site("delivered-late");
    tabref.ok(&["js", "window.open('/'); 1"]);
    tabref.ok(&["tab", "1"]);
    let batch = tabref.run(&[
        "--json",
        "do",
        &format!("click \"Later\"\nopen --new-tab {base}/other"),
    ]);
    let batch = batch.json();
    assert_eq!(batch["ok"], true, "{batch}");
    let steps = batch["data"]["steps"].as_array().unwrap();
    assert_eq!(steps.len(), 2, "{batch}");
    assert!(
        steps[0]["text"]
            .as_str()
            .unwrap()
            .starts_with("ok click \"Later\""),
        "{batch}"
    );
    reported(&steps[0], "where the step led");
    assert!(
        steps[1]["text"].as_str().unwrap().starts_with("ok open"),
        "{batch}"
    );
}
