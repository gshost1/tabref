//! The cold path: the first `open` through a daemon that does not run yet,
//! on a browser that has only just started, and a navigation the browser is
//! slow to start (a just-started browser holds its first page load until it
//! has read its profile; a slow server holds the response the same way).
//! Opt-in: `TABREF_LIVE=1 cargo test -p tabref --test live_cold_start`.

#![cfg(unix)]
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use std::io::{Read, Write};
use std::net::TcpListener;
use std::process::Command;
use std::thread;
use std::time::{Duration, Instant};

use common::{Harness, Run, live, serve};

const PAGE: &str = "<!doctype html><title>Cold</title><h1>Cold start</h1><button>Go</button>";

fn first_run(tabref: &Harness, args: &[&str]) -> Run {
    let output = Command::new(env!("CARGO_BIN_EXE_tabref"))
        .args(["--cdp", &tabref.browser.port.to_string()])
        .args(args)
        .env("TABREF_HOME", &tabref.home)
        .env_remove("TABREF_NO_DAEMON")
        .output()
        .unwrap();
    Run {
        ok: output.status.success(),
        stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
        stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
    }
}

fn stop_daemon(tabref: &Harness) {
    let _ = first_run(tabref, &["daemon", "stop"]);
}

/// Serves `PAGE` once `delay` has passed since each request arrived.
fn serve_after(delay: Duration) -> u16 {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(mut stream) = stream else { continue };
            thread::spawn(move || {
                let mut request = [0_u8; 4096];
                let _ = stream.read(&mut request);
                thread::sleep(delay);
                let response = format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: text/html\r\nContent-Length: {}\r\n\
                     Connection: close\r\n\r\n{PAGE}",
                    PAGE.len()
                );
                let _ = stream.write_all(response.as_bytes());
            });
        }
    });
    port
}

#[test]
fn the_first_open_on_a_just_started_browser_prints_the_page() {
    if !live() {
        return;
    }
    let port = serve(&[("/", PAGE)]);
    let url = format!("http://127.0.0.1:{port}/");
    for round in 0..3 {
        let tabref = common::harness(&format!("cold-{round}"));
        let started = Instant::now();
        let first = first_run(&tabref, &["open", &url]);
        let took = started.elapsed();
        stop_daemon(&tabref);
        assert!(first.ok, "round {round}: {}{}", first.stdout, first.stderr);
        assert!(first.stdout.contains("Cold start"), "{}", first.stdout);
        assert!(took < Duration::from_secs(9), "round {round}: {took:?}");
    }
}

#[test]
fn a_navigation_started_late_in_the_deadline_still_opens() {
    if !live() {
        return;
    }
    let tabref = common::harness("cold-slow");
    let url = format!(
        "http://127.0.0.1:{}/",
        serve_after(Duration::from_millis(6_500))
    );
    let opened = first_run(&tabref, &["--timeout", "8000", "open", &url]);
    stop_daemon(&tabref);
    assert!(opened.ok, "{}{}", opened.stdout, opened.stderr);
    assert!(opened.stdout.contains("Cold start"), "{}", opened.stdout);
}

#[test]
fn a_navigation_the_browser_never_starts_says_so() {
    if !live() {
        return;
    }
    let tabref = common::harness("cold-never");
    let url = format!("http://127.0.0.1:{}/", serve_after(Duration::from_secs(30)));
    let opened = first_run(&tabref, &["--json", "--timeout", "3000", "open", &url]);
    stop_daemon(&tabref);
    assert!(!opened.ok);
    let error = &opened.json()["error"];
    assert_eq!(
        error["details"]["kind"], "cdp_navigation_pending",
        "{error}"
    );
}
