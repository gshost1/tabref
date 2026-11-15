//! `tabref connect --off` ends every use of the user's browser, including
//! a command already running: a `do` batch in the daemon, or one run
//! in-process in another shell. And `--off` works whatever `TABREF_CDP`
//! says. The live tests need Chrome (`TABREF_LIVE=1`).

#![cfg(target_os = "linux")]
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;
#[path = "common/user_home.rs"]
mod user_home;

use std::io::{Read, Write};
use std::net::TcpListener;
use std::process::Stdio;
use std::time::Duration;

use common::live;
use user_home::{UserHome, kill, list_pages, start_user_chrome};

/// Serves a page once `delay` has passed since each request arrived.
fn slow_page(delay: Duration) -> u16 {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    std::thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(mut stream) = stream else { continue };
            std::thread::spawn(move || {
                let mut request = [0_u8; 4096];
                let _ = stream.read(&mut request);
                std::thread::sleep(delay);
                let page = "<!doctype html><title>Slow</title><h1>Slow</h1>";
                let _ = stream.write_all(
                    format!(
                        "HTTP/1.1 200 OK\r\nContent-Type: text/html\r\nContent-Length: {}\r\n\r\n{page}",
                        page.len()
                    )
                    .as_bytes(),
                );
            });
        }
    });
    port
}

/// Runs a two-step `do` batch in the user's browser (a slow page, then
/// `AFTER-OFF`), turns the opt-in off while the first step runs, and returns
/// what `--off` printed, whether the batch succeeded, and the pages the
/// user's browser ended with.
fn off_during_a_batch(name: &str, env: &[(&str, &str)]) -> (String, bool, String) {
    let home = UserHome::new(name);
    let browser = start_user_chrome(&home).expect("the user's Chrome never came up");
    let connected = home.tabref(&["connect", "--user-chrome"]);
    assert!(connected.ok, "{}{}", connected.stdout, connected.stderr);
    let slow = slow_page(Duration::from_secs(3));
    let mut batch = home
        .command(&["do"], env)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    batch
        .stdin
        .take()
        .unwrap()
        .write_all(
            format!("open http://127.0.0.1:{slow}/\nopen data:text/html,<h1>AFTER-OFF</h1>\n")
                .as_bytes(),
        )
        .unwrap();
    std::thread::sleep(Duration::from_millis(1_200));
    let off = home.tabref(&["connect", "--off"]);
    let finished = batch.wait_with_output().unwrap();
    std::thread::sleep(Duration::from_millis(500));
    let pages = list_pages(&home.profile());
    kill(browser);
    assert!(off.ok, "{}{}", off.stdout, off.stderr);
    (off.stdout, finished.status.success(), pages)
}

#[test]
fn a_batch_running_in_the_daemon_stops_at_off() {
    if !live() {
        return;
    }
    let (off, batch_ok, pages) = off_during_a_batch("uc-off-daemon", &[]);
    assert!(off.contains("was stopped and has exited"), "{off}");
    assert!(!batch_ok, "the batch ran to the end after --off");
    assert!(
        !pages.contains("AFTER-OFF"),
        "the batch kept driving the user's browser: {pages}"
    );
}

#[test]
fn a_batch_running_in_another_shell_stops_at_off() {
    if !live() {
        return;
    }
    let (off, batch_ok, pages) = off_during_a_batch("uc-off-shell", &[("TABREF_NO_DAEMON", "1")]);
    assert!(off.contains("stopped using your Google Chrome"), "{off}");
    assert!(!batch_ok, "the batch ran to the end after --off");
    assert!(
        !pages.contains("AFTER-OFF"),
        "the batch kept driving the user's browser: {pages}"
    );
}

#[test]
fn off_works_with_an_endpoint_in_the_environment() {
    let home = UserHome::new("uc-off-cdp");
    let off = home.tabref_with(&["connect", "--off"], &[("TABREF_CDP", "9")]);
    assert!(off.ok, "{}{}", off.stdout, off.stderr);
    assert!(
        off.stdout.contains("daemon was not running"),
        "{}",
        off.stdout
    );
}
