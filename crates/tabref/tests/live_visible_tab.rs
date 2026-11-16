//! With no `--tab` and no remembered tab, a command works in the tab that
//! is showing, not the first one: in headless Chrome the tab opened last is
//! `visible` and the others `hidden`. A tab whose page is stuck (it never
//! answers) does not hold the choice up. Live (`TABREF_LIVE=1`).

#![cfg(target_os = "linux")]
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;
#[path = "common/profile_home.rs"]
mod profile_home;

use std::path::Path;

use common::{harness_home, live, serve};
use profile_home::{Dirs, launched_pid, stop_browser, tabref};

/// Forgets the tab the last command used (`tab-<port>`), keeping the order.
fn forget_remembered_tab(home: &Path) {
    for entry in std::fs::read_dir(home).unwrap().flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        if name.starts_with("tab-") && !name.starts_with("tab-order-") {
            std::fs::remove_file(entry.path()).unwrap();
        }
    }
}

#[test]
fn with_no_tab_remembered_the_showing_tab_is_used_rather_than_the_first() {
    if !live() {
        return;
    }
    let home = harness_home("visible-tab");
    let config = home.with_extension("config");
    let dirs = Dirs {
        homes: vec![home.clone()],
    };
    let run = |args: &[&str]| tabref(&home, &config, args, true);
    let opened = [
        run(&[
            "--timeout",
            "30000",
            "open",
            "data:text/html,<title>First</title>",
            "--launch",
        ]),
        run(&["open", "data:text/html,<title>Second</title>", "--new-tab"]),
        run(&["open", "data:text/html,<title>Shown</title>", "--new-tab"]),
    ];
    let state = run(&["--tab", "0", "js", "document.visibilityState"]);
    forget_remembered_tab(&home);
    let title = run(&["js", "document.title"]);
    let tabs = run(&["tabs"]);
    if let Some(pid) = launched_pid(&home) {
        stop_browser(pid);
    }
    drop(dirs);

    for step in &opened {
        assert!(step.ok, "{}{}", step.stdout, step.stderr);
    }
    assert!(
        state.stdout.contains("hidden"),
        "the first tab is not hidden here, so this test cannot tell: {}",
        state.stdout
    );
    assert!(title.ok, "{}{}", title.stdout, title.stderr);
    assert!(
        title.stdout.contains("Shown"),
        "the command used another tab than the showing one: {}",
        title.stdout
    );
    assert!(
        tabs.stdout.contains("*[2] \"Shown\""),
        "the showing tab was not remembered: {}",
        tabs.stdout
    );
}

const HANG: &str = "<!doctype html><title>Hang</title>\
    <script>setTimeout(() => { for (;;) {} }, 800)</script>";
const SHOWN: &str = "<!doctype html><title>Shown</title>";

#[test]
fn a_tab_that_never_answers_does_not_hold_up_the_choice() {
    if !live() {
        return;
    }
    let home = harness_home("visible-hang");
    let config = home.with_extension("config");
    let dirs = Dirs {
        homes: vec![home.clone()],
    };
    let port = serve(&[("/hang", HANG), ("/shown", SHOWN)]);
    let run = |args: &[&str]| tabref(&home, &config, args, true);
    let hang = format!("http://127.0.0.1:{port}/hang");
    let shown = format!("http://localhost:{port}/shown");
    let opened = [
        run(&["--timeout", "30000", "open", &hang, "--launch"]),
        run(&["open", "data:text/html,<title>Second</title>", "--new-tab"]),
        run(&["open", &shown, "--new-tab"]),
    ];
    std::thread::sleep(std::time::Duration::from_millis(1_500));
    forget_remembered_tab(&home);
    let started = std::time::Instant::now();
    let title = run(&["js", "document.title"]);
    let took = started.elapsed();
    if let Some(pid) = launched_pid(&home) {
        stop_browser(pid);
    }
    drop(dirs);

    for step in &opened {
        assert!(step.ok, "{}{}", step.stdout, step.stderr);
    }
    assert!(
        title.stdout.contains("Shown"),
        "{}{}",
        title.stdout,
        title.stderr
    );
    assert!(
        took < std::time::Duration::from_millis(1_200),
        "a tab that never answers held the choice up: {took:?}"
    );
}
