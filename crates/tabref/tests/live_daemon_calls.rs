//! Live checks of how the daemon serves calls that overlap or go away: a
//! killed client frees its tab, calls on one tab wait their turn while calls
//! on two tabs run side by side, and a relative `open` path means the
//! caller's directory. Opt-in: `TABREF_LIVE=1 cargo test -p tabref --test live_daemon_calls`.

#![cfg(unix)]
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use std::path::Path;
use std::process::{Child, Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};

use common::{Harness, live, serve};
use tabref_cdp::{connect, create_background_tab, endpoint_from_port};
use tabref_core::Deadline;

const PAGE: &str = "<!doctype html><title>Calls</title><h1>Calls</h1><button>Go</button>";

fn command(tabref: &Harness, args: &[&str], cwd: Option<&Path>) -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_tabref"));
    command
        .args(["--cdp", &tabref.browser.port.to_string()])
        .args(args)
        .env("TABREF_HOME", &tabref.home)
        .env_remove("TABREF_NO_DAEMON")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    if let Some(cwd) = cwd {
        command.current_dir(cwd);
    }
    command
}

/// Runs a call to completion: whether it succeeded, its output, its time.
fn timed(tabref: &Harness, args: &[&str]) -> (bool, String, Duration) {
    let started = Instant::now();
    let output = command(tabref, args, None).output().unwrap();
    let text = String::from_utf8_lossy(&output.stdout).into_owned()
        + &String::from_utf8_lossy(&output.stderr);
    (output.status.success(), text, started.elapsed())
}

fn waiting(tabref: &Harness, extra: &[&str], timeout_ms: &str) -> Child {
    let mut args = extra.to_vec();
    args.extend(["--timeout", timeout_ms, "wait", "--text", "never shown"]);
    command(tabref, &args, None).spawn().unwrap()
}

fn opened(name: &str) -> Harness {
    let tabref = common::harness(name);
    let port = serve(&[("/", PAGE)]);
    let (ok, text, _) = timed(&tabref, &["open", &format!("http://127.0.0.1:{port}/")]);
    assert!(ok, "{text}");
    tabref
}

#[test]
fn a_killed_client_frees_its_tab_for_the_next_call() {
    if !live() {
        return;
    }
    let tabref = opened("calls-killed");
    let mut client = waiting(&tabref, &[], "6000");
    thread::sleep(Duration::from_millis(700));
    client.kill().unwrap();
    let _ = client.wait();

    let (ok, text, took) = timed(&tabref, &["--timeout", "3000", "snap"]);

    assert!(ok, "{text}");
    assert!(text.contains("heading \"Calls\""), "{text}");
    assert!(took < Duration::from_millis(1_500), "{took:?}");
}

#[test]
fn calls_on_one_tab_take_turns_and_calls_on_two_tabs_run_side_by_side() {
    if !live() {
        return;
    }
    let tabref = opened("calls-overlap");
    let mut first = waiting(&tabref, &[], "1500");
    thread::sleep(Duration::from_millis(300));
    let (ok, text, took) = timed(&tabref, &["snap"]);
    assert!(ok, "{text}");
    assert!(
        took >= Duration::from_millis(800),
        "same tab ran at once: {took:?}"
    );
    let _ = first.wait();

    let deadline = Deadline::after(10_000).unwrap();
    let endpoint = endpoint_from_port(tabref.browser.port, deadline).unwrap();
    let mut browser = connect(&endpoint, deadline).unwrap();
    let other = create_background_tab(&mut browser, "about:blank", deadline).unwrap();
    let mut second = waiting(&tabref, &["--tab", &other], "2000");
    thread::sleep(Duration::from_millis(300));
    let (ok, text, took) = timed(&tabref, &["--tab", "0", "snap"]);
    assert!(ok, "{text}");
    assert!(
        took < Duration::from_millis(1_000),
        "other tab waited: {took:?}"
    );
    let _ = second.wait();
}

#[test]
fn a_relative_open_path_means_the_callers_directory() {
    if !live() {
        return;
    }
    let tabref = common::harness("calls-relative");
    let dir = tabref.home.join("pages");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(
        dir.join("local.html"),
        "<!doctype html><title>Local file</title><h1>From disk</h1>",
    )
    .unwrap();
    let warm = command(&tabref, &["tabs"], None).output().unwrap();
    assert!(warm.status.success());

    let output = command(&tabref, &["open", "local.html"], Some(&dir))
        .output()
        .unwrap();
    let text = String::from_utf8_lossy(&output.stdout);

    assert!(output.status.success(), "{text}");
    assert!(text.contains("\"Local file\""), "{text}");
    assert!(text.contains("heading \"From disk\""), "{text}");
}
