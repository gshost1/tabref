//! Live runs of `open --new-tab`: the address opens in a new tab that
//! becomes the current one, a relative address resolves against the page
//! it was given from, the tab it came from stays as it was, and the `do`
//! step does the same. Opt-in:
//! `TABREF_LIVE=1 cargo test -p tabref --test live_open_tab -- --test-threads=1`.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

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
