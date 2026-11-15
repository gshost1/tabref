//! Live end-to-end runs of the `tabref` binary against headless Chrome and
//! local fixture pages. Opt-in: `TABREF_LIVE=1 cargo test -p tabref --test live_cli`.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use common::{harness, live, serve};

const SHOP: &str = r#"<!doctype html><html><head><title>Shop</title></head><body>
<h1>Checkout</h1>
<a href="/done">Finish</a>
<a href="/done" target="_blank">Finish elsewhere</a>
<form onsubmit="event.preventDefault(); document.getElementById('out').textContent = 'Sent ' + document.getElementById('email').value;">
  <label>Email <input id="email" type="email"></label>
  <label><input type="checkbox"> Gift wrap</label>
  <label>Size <select><option>Small</option><option>Large</option></select></label>
  <button>Send</button>
</form>
<button id="greet" onclick="alert('Saved!')">Save</button>
<p id="out">Waiting</p>
<p>Total: 42 EUR</p>
</body></html>"#;

const DONE: &str = "<!doctype html><html><head><title>Done</title></head><body><h1>Thank you</h1><a href=\"/\">Back</a></body></html>";

const OTHER: &str =
    "<!doctype html><html><head><title>Other</title></head><body><h1>Other</h1></body></html>";

fn shop() -> (common::Harness, String) {
    let port = serve(&[("/", SHOP), ("/done", DONE), ("/other", OTHER)]);
    (harness("shop"), format!("http://127.0.0.1:{port}/"))
}

#[test]
fn open_prints_a_snapshot_and_actions_read_back() {
    if !live() {
        return;
    }
    let (tabref, url) = shop();
    let opened = tabref.ok(&["open", &url]);
    assert!(
        opened.stdout.starts_with("page \"Shop\""),
        "{}",
        opened.stdout
    );
    assert!(opened.stdout.contains("# Checkout"));

    let email = opened.ref_for("textbox Email");
    let filled = tabref.ok(&["fill", &email, "ada@example.test"]);
    assert!(
        filled.stdout.contains("-> \"ada@example.test\""),
        "{}",
        filled.stdout
    );

    let gift = opened.ref_for("checkbox Gift wrap");
    assert!(tabref.ok(&["check", &gift]).stdout.starts_with("checked"));
    assert!(
        tabref
            .ok(&["check", &gift])
            .stdout
            .contains("already checked")
    );

    let size = opened.ref_for("select Size");
    assert!(
        tabref
            .ok(&["select", &size, "Large"])
            .stdout
            .contains("-> \"Large\"")
    );

    let pressed = tabref.ok(&["press", "Enter", &email]);
    assert!(
        pressed.stdout.starts_with("pressed Enter"),
        "{}",
        pressed.stdout
    );
    let found = tabref.ok(&["find", "Sent ada"]);
    assert!(
        found.stdout.contains("Sent ada@example.test"),
        "{}",
        found.stdout
    );

    let read = tabref.ok(&["read"]);
    assert!(read.stdout.contains("Total: 42 EUR"));
    assert_eq!(tabref.ok(&["js", "document.title"]).stdout.trim(), "Shop");
}

#[test]
fn a_click_that_navigates_prints_the_new_page_and_old_refs_go_stale() {
    if !live() {
        return;
    }
    let (tabref, url) = shop();
    let opened = tabref.ok(&["open", &url]);
    let finish = opened.ref_for("Finish -> /done");
    let clicked = tabref.ok(&["click", &finish]);
    assert!(
        clicked.stdout.contains("page \"Done\""),
        "{}",
        clicked.stdout
    );
    assert!(clicked.stdout.contains("heading \"Thank you\""));

    let stale = tabref.run(&["click", &finish]);
    assert!(!stale.ok);
    assert!(
        stale.stderr.starts_with("error STALE_REF"),
        "{}",
        stale.stderr
    );

    let waited = tabref.ok(&["wait", "--text", "Thank you"]);
    assert!(waited.stdout.starts_with("ok after"), "{}", waited.stdout);
    let timed_out = tabref.run(&["--timeout", "600", "wait", "--text", "Never shown"]);
    assert!(
        timed_out.stderr.starts_with("error TIMEOUT"),
        "{}",
        timed_out.stderr
    );
}

#[test]
fn a_changed_element_is_stale_instead_of_clicked() {
    if !live() {
        return;
    }
    let (tabref, url) = shop();
    let opened = tabref.ok(&["open", &url]);
    let save = opened.ref_for("button Save");
    tabref.ok(&[
        "js",
        "document.getElementById('greet').textContent = 'Delete'",
    ]);
    let drift = tabref.run(&["--json", "click", &save]);
    let envelope = drift.json();
    assert_eq!(envelope["ok"], false);
    assert_eq!(envelope["error"]["code"], "STALE_REF");
    assert_eq!(envelope["error"]["details"]["kind"], "cdp_node_drift");
    assert_eq!(envelope["error"]["disposition"]["retry"], "safe");
}

#[test]
fn dialogs_are_accepted_and_new_tabs_become_current() {
    if !live() {
        return;
    }
    let (tabref, url) = shop();
    let opened = tabref.ok(&["open", &url]);
    let save = opened.ref_for("button Save");
    let alerted = tabref.ok(&["click", &save]);
    assert!(
        alerted.stdout.contains("dialog alert \"Saved!\" accepted"),
        "{}",
        alerted.stdout
    );

    let elsewhere = opened.ref_for("Finish elsewhere");
    let popped = tabref.ok(&["click", &elsewhere]);
    assert!(
        popped.stdout.contains("opened a new tab"),
        "{}",
        popped.stdout
    );
    let tabs = tabref.ok(&["tabs"]);
    assert!(
        tabs.stdout
            .lines()
            .any(|line| line.starts_with("*[") && line.contains("/done")),
        "{}",
        tabs.stdout
    );
    let closed = tabref.ok(&["close"]);
    assert!(closed.stdout.contains("\"Shop\""), "{}", closed.stdout);
}

#[test]
fn the_json_envelope_carries_snapshot_lines() {
    if !live() {
        return;
    }
    let (tabref, url) = shop();
    tabref.ok(&["open", &url]);
    let envelope = tabref.ok(&["--json", "snap", "-a"]).json();
    assert_eq!(envelope["ok"], true);
    assert_eq!(envelope["command"], "snap");
    let lines = envelope["data"]["lines"].as_array().unwrap();
    assert!(
        lines
            .iter()
            .any(|line| line.as_str().unwrap().contains("Total: 42 EUR"))
    );
    assert!(
        envelope["data"]["snapshot_id"]
            .as_str()
            .unwrap()
            .starts_with('s')
    );
}

#[test]
fn tab_indices_keep_naming_the_tabs_the_agent_listed() {
    if !live() {
        return;
    }
    let (tabref, url) = shop();
    tabref.ok(&["open", &url]);
    tabref.ok(&["js", "window.open('/done'); 1"]);
    let listed = tabref.ok(&["tabs"]);
    let second = listed
        .stdout
        .lines()
        .find(|line| line.contains("[1]"))
        .unwrap()
        .split_whitespace()
        .find(|word| word.starts_with("id="))
        .unwrap()
        .to_owned();
    tabref.ok(&["js", "window.open('/other'); 1"]);
    let reordered = tabref.ok(&["tabs"]);
    assert!(
        reordered
            .stdout
            .lines()
            .any(|line| line.contains("[1]") && line.contains(&second)),
        "{}",
        reordered.stdout
    );
    let third = reordered
        .stdout
        .lines()
        .find(|line| line.contains("[2]"))
        .unwrap();
    assert!(!third.contains(&second), "{}", reordered.stdout);
    let closed = tabref.ok(&["close", "1"]);
    assert!(!closed.stdout.contains(&second), "{}", closed.stdout);
    assert_eq!(
        closed
            .stdout
            .lines()
            .filter(|line| line.contains("id="))
            .count(),
        2
    );
}
