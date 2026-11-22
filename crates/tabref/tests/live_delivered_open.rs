//! An `open` whose page loaded stays a success when the snapshot after it
//! cannot be read: one note line says so, and the JSON says the same.
#![cfg(unix)]
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use common::{Harness, harness, live, serve};
use serde_json::Value;

const START: &str = "<!doctype html><title>Start</title><h1>Start</h1>";
const OTHER: &str = "<!doctype html><title>Other</title><h1>Other</h1>";
const HOP: &str = "<!doctype html><title>Hop</title><script>history.replaceState(null, '', '/landed')</script><h1>Hop</h1>";

fn unreadable(name: &str) -> (Harness, String) {
    let port = serve(&[("/", START), ("/other", OTHER), ("/hop", HOP)]);
    let tabref = harness(name);
    let base = format!("http://127.0.0.1:{port}");
    tabref.ok(&["open", &format!("{base}/")]);
    let counter = tabref.home.join("refs-next");
    std::fs::remove_file(&counter).unwrap();
    std::fs::create_dir(&counter).unwrap();
    (tabref, base)
}

fn note(text: &str) -> String {
    let line = text
        .lines()
        .find(|line| line.starts_with("note: could not read"))
        .unwrap_or_else(|| panic!("no unread note in:\n{text}"));
    assert!(
        line.starts_with("note: could not read the page ("),
        "{text}"
    );
    assert!(line.ends_with("; run tabref snap to see it"), "{text}");
    line.to_owned()
}

fn said(text: &str, data: &Value) {
    let line = note(text);
    assert_eq!(
        data["report_error"].as_str(),
        line.strip_prefix("note: "),
        "{data}"
    );
}

#[test]
fn an_unreadable_page_after_open_keeps_the_open() {
    if !live() {
        return;
    }
    let (tabref, base) = unreadable("delivered-open");
    let opened = tabref.ok(&["open", &format!("{base}/other")]);
    let first = opened.stdout.lines().next().unwrap();
    assert_eq!(
        first,
        format!("page \"Other\" {base}/other"),
        "{}",
        opened.stdout
    );
    note(&opened.stdout);
    let envelope = tabref.ok(&["--json", "open", "/"]).json();
    assert_eq!(envelope["ok"], true, "{envelope}");
    let data = &envelope["data"];
    assert_eq!(data["load_finished"], true, "{envelope}");
    assert_eq!(data["url"], format!("{base}/"), "{envelope}");
    assert_eq!(data["title"], "Start", "{envelope}");
    let said = data["report_error"].as_str().unwrap();
    assert!(said.starts_with("could not read the page ("), "{envelope}");
}

#[test]
fn an_unreadable_page_after_open_names_the_address_it_ended_at() {
    if !live() {
        return;
    }
    let (tabref, base) = unreadable("delivered-open-moved");
    let opened = tabref.ok(&["open", &format!("{base}/hop")]);
    let first = opened.stdout.lines().next().unwrap();
    assert_eq!(
        first,
        format!("page \"Hop\" {base}/landed"),
        "{}",
        opened.stdout
    );
    note(&opened.stdout);
    let envelope = tabref
        .ok(&["--json", "open", &format!("{base}/hop")])
        .json();
    let data = &envelope["data"];
    assert_eq!(data["url"], format!("{base}/landed"), "{envelope}");
    assert_eq!(data["title"], "Hop", "{envelope}");
}

#[test]
fn an_unreadable_page_after_open_in_a_new_tab_keeps_the_open() {
    if !live() {
        return;
    }
    let (tabref, base) = unreadable("delivered-open-new-tab");
    let opened = tabref.ok(&["open", "--new-tab", &format!("{base}/other")]);
    assert!(
        opened.stdout.starts_with("new tab [1]\npage \"Other\" "),
        "{}",
        opened.stdout
    );
    note(&opened.stdout);
}

#[test]
fn an_unreadable_page_after_a_last_open_step_keeps_the_batch() {
    if !live() {
        return;
    }
    let (tabref, _) = unreadable("delivered-open-do");
    let batch = tabref.ok(&["--json", "do", "js 1\nopen /other"]).json();
    assert_eq!(batch["ok"], true, "{batch}");
    let steps = batch["data"]["steps"].as_array().unwrap();
    assert_eq!(steps.len(), 2, "{batch}");
    let step = &steps[1];
    let text = step["text"].as_str().unwrap();
    assert!(text.starts_with("ok open"), "{batch}");
    said(text, &step["data"]);
    assert!(step["data"].get("page").is_none(), "{batch}");
}
