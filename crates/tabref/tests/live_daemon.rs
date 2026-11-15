//! Live runs of the daemon path against headless Chrome: output identical to
//! an in-process run, status and stop, idle exit, a rebuilt binary replacing
//! its daemon, and a browser that dies and comes back on the same port.
//! Opt-in: `TABREF_LIVE=1 cargo test -p tabref --test live_daemon`.

#![cfg(unix)]
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use std::net::TcpListener;
use std::os::unix::process::CommandExt;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant, SystemTime};

use common::{Harness, Run, live, serve};
use tabref_cdp::{LaunchedBrowser, endpoint_from_port, find_chrome};
use tabref_core::Deadline;

const PAGE: &str = r#"<!doctype html><html><head><title>Daemon</title></head><body>
<h1>Settings</h1><label>Name <input id="name"></label>
<button onclick="this.textContent='Saved'">Save</button>
<iframe title="inner" srcdoc="<button>Inner</button>"></iframe>
</body></html>"#;

fn run(tabref: &Harness, args: &[&str], env: &[(&str, &str)]) -> Run {
    let mut command = Command::new(env!("CARGO_BIN_EXE_tabref"));
    command
        .args(["--cdp", &tabref.browser.port.to_string()])
        .args(args)
        .env("TABREF_HOME", &tabref.home)
        .env_remove("TABREF_NO_DAEMON");
    for (key, value) in env {
        command.env(key, value);
    }
    let output = command.output().unwrap();
    Run {
        ok: output.status.success(),
        stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
        stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
    }
}

/// The output with every snapshot id (`@s<id>:`) blanked, wherever it
/// stands, so two snapshots of the same page compare equal.
fn without_snapshot_ids(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(at) = rest.find("@s") {
        out.push_str(&rest[..at]);
        let tail = &rest[at + 2..];
        let id_len = tail
            .find(|c: char| !c.is_ascii_alphanumeric())
            .unwrap_or(tail.len());
        if id_len > 0 && tail[id_len..].starts_with(':') {
            out.push_str("@S:");
            rest = &tail[id_len + 1..];
        } else {
            out.push_str("@s");
            rest = tail;
        }
    }
    out.push_str(rest);
    out
}

/// A browser killed when the test ends, even by a failed assertion, so it
/// never keeps the machine's Chrome slot.
struct Killed(LaunchedBrowser);

impl Drop for Killed {
    fn drop(&mut self) {
        kill(&self.0);
    }
}

fn daemon_pid(tabref: &Harness) -> Option<u64> {
    let status = run(tabref, &["--json", "daemon", "status"], &[]).json();
    status["data"]["pid"].as_u64()
}

fn page(tabref: &Harness) -> String {
    let port = serve(&[("/", PAGE)]);
    let url = format!("http://127.0.0.1:{port}/");
    assert!(run(tabref, &["open", &url], &[]).ok);
    url
}

#[test]
fn the_daemon_prints_what_an_in_process_run_prints() {
    if !live() {
        return;
    }
    let tabref = common::harness("daemon-same");
    page(&tabref);
    let in_process = [("TABREF_NO_DAEMON", "1")];
    for args in [
        &["snap"][..],
        &["snap", "--all"],
        &["--json", "tabs"],
        &["find", "Inner"],
    ] {
        let daemon = run(&tabref, args, &[]);
        let local = run(&tabref, args, &in_process);
        assert!(
            daemon.ok && local.ok,
            "{args:?}: {}{}",
            daemon.stderr,
            local.stderr
        );
        assert_eq!(
            without_snapshot_ids(&daemon.stdout),
            without_snapshot_ids(&local.stdout),
            "{args:?}"
        );
    }
    let missing = run(&tabref, &["click", "@s00000000:e1"], &[]);
    let missing_local = run(&tabref, &["click", "@s00000000:e1"], &in_process);
    assert!(!missing.ok);
    assert_eq!(missing.stderr, missing_local.stderr);
    assert!(daemon_pid(&tabref).is_some());
}

#[test]
fn status_stop_and_idle_exit() {
    if !live() {
        return;
    }
    let tabref = common::harness("daemon-life");
    assert!(daemon_pid(&tabref).is_none());
    page(&tabref);
    assert!(daemon_pid(&tabref).is_some());
    let stopped = run(&tabref, &["daemon", "stop"], &[]);
    assert!(stopped.stdout.contains("stopped"), "{}", stopped.stdout);
    assert!(daemon_pid(&tabref).is_none());

    let idle = [("TABREF_DAEMON_IDLE_MS", "300")];
    assert!(run(&tabref, &["snap"], &idle).ok);
    let started = Instant::now();
    while daemon_pid(&tabref).is_some() {
        assert!(
            started.elapsed() < Duration::from_secs(5),
            "daemon never idled out"
        );
        std::thread::sleep(Duration::from_millis(100));
    }
}

#[test]
fn a_rebuilt_binary_replaces_the_running_daemon() {
    if !live() {
        return;
    }
    let tabref = common::harness("daemon-build");
    page(&tabref);
    let first = daemon_pid(&tabref).unwrap();
    let binary = std::fs::File::open(env!("CARGO_BIN_EXE_tabref")).unwrap();
    binary
        .set_modified(SystemTime::now() + Duration::from_secs(1))
        .unwrap();
    drop(binary);

    let snap = run(&tabref, &["snap"], &[]);
    assert!(snap.ok, "{}", snap.stderr);
    assert!(snap.stdout.contains("Save"), "{}", snap.stdout);
    let second = daemon_pid(&tabref).unwrap();
    assert_ne!(first, second);
}

#[test]
fn a_browser_that_dies_is_a_clear_error_and_the_next_call_reconnects() {
    if !live() {
        return;
    }
    let port = free_port();
    let tabref = Harness {
        browser: chrome_on(port),
        home: common::harness_home("daemon-crash"),
    };
    page(&tabref);
    kill(&tabref.browser);

    let started = Instant::now();
    let failed = run(&tabref, &["--timeout", "3000", "snap"], &[]);
    assert!(!failed.ok);
    assert!(started.elapsed() < Duration::from_secs(6));
    assert!(failed.stderr.starts_with("error "), "{}", failed.stderr);

    let _restarted = Killed(chrome_on(port));
    page(&tabref);
    let snap = run(&tabref, &["snap"], &[]);
    assert!(snap.ok, "{}", snap.stderr);
    assert!(snap.stdout.contains("Settings"), "{}", snap.stdout);
}

fn free_port() -> u16 {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    listener.local_addr().unwrap().port()
}

/// A headless Chrome on a fixed port, so a restarted browser comes back at
/// the endpoint the daemon serves. Chrome writes no `DevToolsActivePort`
/// for a fixed port, so readiness is its `/json/version` answering.
fn chrome_on(port: u16) -> LaunchedBrowser {
    let profile_dir = common::harness_home("daemon-profile");
    let extra_args: Vec<String> = std::env::var("TABREF_CHROME_ARGS")
        .map(|args| args.split_whitespace().map(str::to_owned).collect())
        .unwrap_or_default();
    let mut child = Command::new(find_chrome().unwrap())
        .args([
            "--headless=new",
            "--no-first-run",
            "--no-default-browser-check",
            &format!("--remote-debugging-port={port}"),
            &format!("--user-data-dir={}", profile_dir.display()),
        ])
        .args(extra_args)
        .arg("about:blank")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .process_group(0)
        .spawn()
        .unwrap();
    let pid = child.id();
    std::thread::spawn(move || child.wait());
    let started = Instant::now();
    while endpoint_from_port(port, Deadline::after(500).unwrap()).is_err() {
        assert!(
            started.elapsed() < Duration::from_secs(30),
            "Chrome never answered"
        );
        std::thread::sleep(Duration::from_millis(50));
    }
    LaunchedBrowser {
        pid,
        port,
        websocket_path: String::new(),
        profile_dir,
        headless: true,
        started: None,
        profile_use: tabref_cdp::ProfileUse::THROWAWAY,
    }
}

fn kill(browser: &LaunchedBrowser) {
    let _ = Command::new("kill")
        .args(["-KILL", &format!("-{}", browser.pid)])
        .status();
    let _ = std::fs::remove_dir_all(&browser.profile_dir);
    std::thread::sleep(Duration::from_millis(300));
}
