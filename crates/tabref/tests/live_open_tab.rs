//! Live runs of `open --new-tab`: the address opens in a new tab that
//! becomes the current one, a relative address resolves against the page
//! it was given from, the tab it came from stays as it was, and the `do`
//! step does the same. A failed load in the new tab says that tab is now
//! current and is not offered as a retry, a relative address with no page
//! to resolve against is refused before any tab opens, and a step whose
//! address is missing or a flag is refused. Opt-in:
//! `TABREF_LIVE=1 cargo test -p tabref --test live_open_tab -- --test-threads=1`.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use std::io::Read;
use std::net::TcpListener;
use std::thread;
use std::time::Duration;

use common::{Harness, harness, live, serve};

const LIST: &str = "<!doctype html><title>List</title><h1>All items</h1>";
const ITEM: &str = "<!doctype html><title>Item</title><h1>One item</h1><p id=likes>No likes</p><button onclick=\"document.getElementById('likes').textContent = 'Item liked'\">Like</button>";
const HELP: &str = "<!doctype html><title>Help</title><h1>Help page</h1>";

fn site() -> (Harness, String) {
    let port = serve(&[("/shop/list", LIST), ("/shop/item", ITEM), ("/help", HELP)]);
    (harness("open-tab"), format!("http://127.0.0.1:{port}"))
}

fn tab_lines(tabref: &Harness) -> Vec<String> {
    tabref
        .ok(&["tabs"])
        .stdout
        .lines()
        .map(str::to_owned)
        .collect()
}

#[test]
fn open_new_tab_opens_a_relative_address_in_a_new_current_tab() {
    if !live() {
        return;
    }
    let (tabref, base) = site();
    tabref.ok(&["open", &format!("{base}/shop/list")]);
    let opened = tabref.ok(&["open", "--new-tab", "./item"]);
    assert!(
        opened.stdout.starts_with("new tab [1]\n"),
        "{}",
        opened.stdout
    );
    assert!(opened.stdout.contains("One item"), "{}", opened.stdout);
    let tabs = tab_lines(&tabref);
    assert_eq!(tabs.len(), 2, "{tabs:?}");
    assert!(tabs[0].starts_with(" [0] \"List\""), "{tabs:?}");
    assert!(tabs[1].starts_with("*[1] \"Item\""), "{tabs:?}");
    assert!(tabs[1].contains("/shop/item"), "{tabs:?}");
    let clicked = tabref.ok(&["click", "Like"]);
    assert!(clicked.stdout.contains("Item liked"), "{}", clicked.stdout);
}

#[test]
fn an_open_step_with_new_tab_opens_its_own_tab() {
    if !live() {
        return;
    }
    let (tabref, base) = site();
    tabref.ok(&["open", &format!("{base}/shop/list")]);
    let run = tabref.ok(&["do", "open --new-tab /help; expect text \"Help page\""]);
    assert!(
        run.stdout
            .starts_with("ok open /help \"Help\" (new tab [1])"),
        "{}",
        run.stdout
    );
    let tabs = tab_lines(&tabref);
    assert_eq!(tabs.len(), 2, "{tabs:?}");
    assert!(tabs[1].starts_with("*[1] \"Help\""), "{tabs:?}");
}

/// A port that accepts connections and never answers them.
fn silent_port() -> u16 {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(mut stream) = stream else { continue };
            thread::spawn(move || {
                let mut request = [0_u8; 4096];
                let _ = stream.read(&mut request);
                thread::sleep(Duration::from_secs(60));
            });
        }
    });
    port
}

#[test]
fn a_load_that_fails_in_the_new_tab_says_that_tab_is_now_current() {
    if !live() {
        return;
    }
    let (tabref, base) = site();
    tabref.ok(&["open", &format!("{base}/shop/list")]);
    let url = format!("http://127.0.0.1:{}/", silent_port());
    let failed = tabref.run(&["--json", "--timeout", "3000", "open", "--new-tab", &url]);
    let error = &failed.json()["error"];
    assert_eq!(error["details"]["new_tab_index"], 1, "{error}");
    assert_ne!(error["details"]["retryable"], true, "{error}");
    let hint = error["suggestion"].as_str().unwrap_or_default();
    assert!(hint.contains("new tab [1] is now current"), "{error}");
    assert!(hint.contains("without --new-tab"), "{error}");
    let step = tabref.run(&["--timeout", "3000", "do", &format!("open --new-tab {url}")]);
    assert!(
        step.stderr.contains("new tab [2] is now current"),
        "{}",
        step.stderr
    );
}

#[test]
fn a_relative_new_tab_with_no_page_to_resolve_against_opens_nothing() {
    if !live() {
        return;
    }
    let (tabref, _) = site();
    let run = tabref.run(&["open", "--new-tab", "/help"]);
    assert!(
        run.stderr.starts_with(
            "error INVALID_ARGS: a relative address needs a current page; give a full URL"
        ),
        "{}",
        run.stderr
    );
    assert_eq!(tab_lines(&tabref).len(), 1);
}

#[test]
fn an_open_step_takes_the_flag_on_either_side_and_refuses_a_flag_as_address() {
    if !live() {
        return;
    }
    let (tabref, base) = site();
    tabref.ok(&["open", &format!("{base}/shop/list")]);
    let missing = tabref.run(&["do", "open --new-tab"]);
    assert!(
        missing.stderr.starts_with("error INVALID_ARGS"),
        "{}{}",
        missing.stdout,
        missing.stderr
    );
    assert_eq!(tab_lines(&tabref).len(), 1);
    let after = tabref.ok(&["do", "open /help --new-tab"]);
    assert!(
        after
            .stdout
            .starts_with("ok open /help \"Help\" (new tab [1])"),
        "{}",
        after.stdout
    );
}

#[test]
fn a_full_address_opens_a_new_tab_while_another_command_works_in_the_current_one() {
    if !live() {
        return;
    }
    let (tabref, base) = site();
    tabref.ok(&["open", &format!("{base}/shop/list")]);
    let busy = std::process::Command::new(env!("CARGO_BIN_EXE_tabref"))
        .args([
            "--cdp",
            &tabref.browser.port.to_string(),
            "--timeout",
            "6000",
        ])
        .args(["wait", "--text", "never shown"])
        .env("TABREF_HOME", &tabref.home)
        .spawn()
        .unwrap();
    thread::sleep(Duration::from_millis(800));
    let opened = tabref.run(&[
        "--timeout",
        "4000",
        "open",
        "--new-tab",
        &format!("{base}/help"),
    ]);
    let mut busy = busy;
    let _ = busy.wait();
    assert!(opened.ok, "{}{}", opened.stdout, opened.stderr);
    assert!(
        opened.stdout.starts_with("new tab [1]\n"),
        "{}",
        opened.stdout
    );
}
