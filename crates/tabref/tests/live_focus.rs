//! Live runs of `focus T` in a browser tabref launched, whose tabs are
//! told at attach that they hold the window focus and are never told
//! otherwise: the page's own focus handlers run, as a command and as a `do`
//! step, in a background tab too, which reads as visible and focused to
//! every command alike; what a focus showed stays shown for the
//! next command; a label focuses its control; and a page that moves the
//! focus away within about 300 ms is reported. Opt-in:
//! `TABREF_LIVE=1 cargo test -p tabref --test live_focus -- --test-threads=1`.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use std::time::{Duration, Instant};

use common::focus_page::{active, in_back, open_page, own_browser, own_site, read, to_back};
use common::live;

#[test]
fn focus_moves_the_keyboard_focus_and_the_page_sees_it() {
    if !live() {
        return;
    }
    let tabref = own_site("focus-own");
    let focused = tabref.ok(&["focus", "Name"]);
    let first = focused.stdout.lines().next().unwrap();
    assert_eq!(first, "focused \"Name\"", "{}", focused.stdout);
    assert!(
        focused.stdout.contains("name focused"),
        "{}",
        focused.stdout
    );
    assert_eq!(active(&tabref), "name");
    let step = tabref.ok(&["do", "focus City; press a"]);
    assert!(
        step.stdout.starts_with("ok focus \"City\""),
        "{}",
        step.stdout
    );
    assert_eq!(active(&tabref), "city");
    assert_eq!(read(&tabref, "document.getElementById('city').value"), "a");
}

#[test]
fn what_a_focus_showed_is_still_shown_to_the_next_command() {
    if !live() {
        return;
    }
    let tabref = own_site("focus-lasts");
    tabref.ok(&["focus", "Search"]);
    assert_eq!(active(&tabref), "search");
    assert_eq!(read(&tabref, "document.hasFocus()"), "true");
    assert_eq!(
        read(&tabref, "document.getElementById('list').hidden"),
        "false"
    );
    let snap = tabref.ok(&["snap"]);
    assert!(snap.stdout.contains("Suggestion one"), "{}", snap.stdout);
}

#[test]
fn focus_in_a_background_tab_runs_its_handlers_and_reads_the_same_before_and_after() {
    if !live() {
        return;
    }
    let tabref = own_site("focus-own-back");
    to_back(&tabref);
    assert_eq!(in_back(&tabref, "document.visibilityState"), "visible");
    assert_eq!(in_back(&tabref, "document.hasFocus()"), "true");
    let focused = tabref.ok(&["--tab", "0", "focus", "Name"]);
    assert!(
        focused.stdout.contains("name focused"),
        "{}",
        focused.stdout
    );
    assert!(
        !focused.stdout.contains("in the background"),
        "{}",
        focused.stdout
    );
    assert!(
        !focused.stdout.contains("vis changed"),
        "{}",
        focused.stdout
    );
    assert_eq!(in_back(&tabref, "document.visibilityState"), "visible");
    assert_eq!(in_back(&tabref, "document.hasFocus()"), "true");
}

#[test]
fn a_label_focuses_its_control_and_a_move_away_is_caught() {
    if !live() {
        return;
    }
    let tabref = own_site("focus-moves");
    let label = tabref.ok(&["snap"]).ref_for("Zip code");
    tabref.ok(&["focus", &label]);
    assert_eq!(active(&tabref), "zip");
    for name in ["Jumpy", "Late jump"] {
        let jumped = tabref.ok(&["focus", name]);
        let first = jumped.stdout.lines().next().unwrap();
        assert_eq!(
            first,
            format!("focused \"{name}\" (the page moved the focus away)"),
            "{}",
            jumped.stdout
        );
        assert_eq!(active(&tabref), "city");
    }
}

#[test]
fn a_focus_with_no_focus_listener_ends_with_the_settle() {
    if !live() {
        return;
    }
    let tabref = own_browser("focus-quiet");
    let port = open_page(&tabref);
    tabref.ok(&["open", &format!("http://127.0.0.1:{port}/quiet")]);
    tabref.ok(&["focus", "Other"]);
    let fastest = (0..3)
        .map(|_| {
            tabref.ok(&["focus", "Other"]);
            let started = Instant::now();
            tabref.ok(&["focus", "Plain"]);
            started.elapsed()
        })
        .min()
        .unwrap();
    assert!(fastest < Duration::from_millis(250), "{fastest:?}");
    assert_eq!(active(&tabref), "plain");
    let deep = tabref.ok(&["focus", "Deep jump"]);
    let first = deep.stdout.lines().next().unwrap();
    assert_eq!(
        first, "focused \"Deep jump\" (the page moved the focus away)",
        "{}",
        deep.stdout
    );
}

#[test]
fn a_tab_tabref_moves_away_from_goes_to_the_background() {
    if !live() {
        return;
    }
    let tabref = own_site("focus-left-behind");
    tabref.ok(&["focus", "Name"]);
    assert_eq!(read(&tabref, "document.hasFocus()"), "true");
    tabref.ok(&["open", "--new-tab", "about:blank"]);
    std::thread::sleep(Duration::from_millis(1_000));
    let ticks = in_back(&tabref, "JSON.stringify(window.ticks)");
    let ticks: serde_json::Value = serde_json::from_str(&ticks).unwrap();
    assert_eq!(ticks["wentHidden"], true, "{ticks}");
    assert!(
        ticks["hidden"].as_u64().unwrap() < 30,
        "a hidden tab runs throttled timers: {ticks}"
    );
}
