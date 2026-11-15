//! tabref attaches only to a browser it can vouch for. Something that is not
//! a Chrome of this user, listening on 9222 or on the port a launched
//! browser used to hold, is never sent a byte. The impostor test needs no
//! browser; the others are live (`TABREF_LIVE=1`).

#![cfg(target_os = "linux")]
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use std::net::TcpListener;
use std::os::unix::process::CommandExt;
use std::process::{Command, Stdio};
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use common::{Run, harness_home, live};
use tabref_cdp::{endpoint_from_port, find_chrome};
use tabref_core::Deadline;

/// A program listening on `port` that counts every connection it is sent.
struct Impostor {
    contacts: Arc<AtomicUsize>,
}

impl Impostor {
    fn on(port: u16) -> Option<Self> {
        let listener = TcpListener::bind(("127.0.0.1", port)).ok()?;
        let contacts = Arc::new(AtomicUsize::new(0));
        let seen = Arc::clone(&contacts);
        std::thread::spawn(move || {
            for stream in listener.incoming() {
                seen.fetch_add(1, Ordering::SeqCst);
                drop(stream);
            }
        });
        Some(Self { contacts })
    }

    fn contacts(&self) -> usize {
        self.contacts.load(Ordering::SeqCst)
    }
}

fn tabref(home: &std::path::Path, args: &[&str], in_process: bool) -> Run {
    let mut command = Command::new(env!("CARGO_BIN_EXE_tabref"));
    command
        .args(args)
        .env("TABREF_HOME", home)
        .env_remove("TABREF_CDP")
        .env_remove("TABREF_NO_DAEMON");
    if in_process {
        command.env("TABREF_NO_DAEMON", "1");
    }
    let output = command.output().unwrap();
    Run {
        ok: output.status.success(),
        stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
        stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
    }
}

fn stop_daemon(home: &std::path::Path) {
    let _ = tabref(home, &["daemon", "stop"], false);
}

#[test]
fn a_program_that_is_not_chrome_on_9222_is_refused_untouched() {
    let Some(impostor) = Impostor::on(9222) else {
        eprintln!("skipped: port 9222 is in use");
        return;
    };
    for in_process in [false, true] {
        let home = harness_home("owner-9222");
        let run = tabref(&home, &["--json", "tabs"], in_process);
        stop_daemon(&home);
        let _ = std::fs::remove_dir_all(&home);
        assert!(!run.ok, "{}", run.stdout);
        let error = &run.json()["error"];
        assert_eq!(
            error["details"]["kind"], "cdp_endpoint_unverified",
            "{error}"
        );
        let named = format!("(pid {})", std::process::id());
        assert!(
            error["message"].as_str().unwrap().contains(&named),
            "{error}"
        );
        assert!(
            error["suggestion"]
                .as_str()
                .unwrap()
                .starts_with("Run 'tabref open <url> --launch'"),
            "{error}"
        );
    }
    assert_eq!(impostor.contacts(), 0, "the impostor was sent a connection");
}

#[test]
fn a_named_port_another_program_holds_works_but_warns() {
    if !live() {
        return;
    }
    let tabref_home = common::harness("owner-named");
    let proxy = forward_to(tabref_home.browser.port);
    let endpoint = format!(
        "ws://127.0.0.1:{proxy}{}",
        tabref_home.browser.websocket_path
    );
    let json = tabref(
        &tabref_home.home,
        &["--cdp", &endpoint, "--json", "tabs"],
        true,
    );
    let text = tabref(&tabref_home.home, &["--cdp", &endpoint, "tabs"], true);
    assert!(
        json.ok && text.ok,
        "{}{}{}",
        json.stderr,
        text.stdout,
        text.stderr
    );
    let warnings = json.json()["data"]["warnings"].clone();
    let warning = warnings[0].as_str().unwrap_or_default().to_owned();
    assert!(
        warning.contains(&format!("port {proxy} is held by"))
            && warning.contains(&format!("(pid {})", std::process::id())),
        "{warnings}"
    );
    assert!(
        text.stdout.starts_with(&format!("warning: port {proxy}")),
        "{}",
        text.stdout
    );
}

#[test]
fn a_named_ipv6_endpoint_is_never_taken_for_checked() {
    if !live() {
        return;
    }
    let tabref_home = common::harness("owner-named-v6");
    let Some(proxy) = forward_on("[::1]:0", tabref_home.browser.port) else {
        eprintln!("skipped: no IPv6 loopback");
        return;
    };
    let endpoint = format!("ws://[::1]:{proxy}{}", tabref_home.browser.websocket_path);
    let mut runs = Vec::new();
    for _ in 0..2 {
        runs.push(tabref(
            &tabref_home.home,
            &["--cdp", &endpoint, "--json", "tabs"],
            false,
        ));
    }
    let failed = tabref(
        &tabref_home.home,
        &["--cdp", &endpoint, "--json", "click", "@s00000000:e1"],
        false,
    );
    stop_daemon(&tabref_home.home);
    for run in &runs {
        assert!(run.ok, "{}{}", run.stdout, run.stderr);
        let warning = run.json()["data"]["warnings"][0]
            .as_str()
            .unwrap_or_default()
            .to_owned();
        assert!(
            warning.contains(&format!("[::1]:{proxy} was not checked")),
            "{}",
            run.stdout
        );
    }
    assert!(!failed.ok);
    let error = &failed.json()["error"];
    assert!(
        error["details"]["warnings"][0]
            .as_str()
            .unwrap_or_default()
            .contains("was not checked"),
        "{error}"
    );
}

/// A forwarder on a fresh loopback port, held by this test process, that
/// relays every connection to `port`.
fn forward_to(port: u16) -> u16 {
    forward_on("127.0.0.1:0", port).unwrap()
}

/// [`forward_to`], listening on `address`; `None` when it cannot bind.
fn forward_on(address: &str, port: u16) -> Option<u16> {
    let listener = TcpListener::bind(address).ok()?;
    let own = listener.local_addr().unwrap().port();
    std::thread::spawn(move || {
        for client in listener.incoming().flatten() {
            let Ok(upstream) = std::net::TcpStream::connect(("127.0.0.1", port)) else {
                continue;
            };
            let (mut client_in, mut upstream_out) =
                (client.try_clone().unwrap(), upstream.try_clone().unwrap());
            let (mut upstream_in, mut client_out) = (upstream, client);
            std::thread::spawn(move || std::io::copy(&mut client_in, &mut upstream_out));
            std::thread::spawn(move || std::io::copy(&mut upstream_in, &mut client_out));
        }
    });
    Some(own)
}

#[test]
fn a_chrome_of_this_user_on_9222_is_used_without_being_named() {
    if !live() {
        return;
    }
    if TcpListener::bind(("127.0.0.1", 9222)).is_err() {
        eprintln!("skipped: port 9222 is in use");
        return;
    }
    let profile = harness_home("owner-chrome-profile");
    let mut browser = Command::new(find_chrome().unwrap())
        .args([
            format!("--user-data-dir={}", profile.display()),
            "--headless=new".to_owned(),
            "--no-first-run".to_owned(),
            "--remote-debugging-port=9222".to_owned(),
            "about:blank".to_owned(),
        ])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .process_group(0)
        .spawn()
        .unwrap();
    let up = (0..100).any(|_| {
        std::thread::sleep(Duration::from_millis(100));
        endpoint_from_port(9222, Deadline::after(500).unwrap()).is_ok()
    });
    let home = harness_home("owner-chrome");
    let run = tabref(
        &home,
        &["open", "data:text/html,<h1>Verified owner</h1>"],
        false,
    );
    stop_daemon(&home);
    kill_group(browser.id());
    let _ = browser.wait();
    let _ = std::fs::remove_dir_all(&profile);
    let _ = std::fs::remove_dir_all(&home);
    assert!(up, "Chrome never listened on 9222");
    assert!(run.ok, "{}{}", run.stdout, run.stderr);
    assert!(run.stdout.contains("Verified owner"), "{}", run.stdout);
}

#[test]
fn a_launched_browser_whose_port_was_taken_over_is_forgotten_untouched() {
    if !live() {
        return;
    }
    let home = harness_home("owner-launched");
    let opened = tabref(
        &home,
        &["open", "data:text/html,<h1>Mine</h1>", "--launch"],
        false,
    );
    assert!(opened.ok, "{}{}", opened.stdout, opened.stderr);
    let record: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(home.join("launched.json")).unwrap())
            .unwrap();
    let port = u16::try_from(record["port"].as_u64().unwrap()).unwrap();
    let pid = u32::try_from(record["pid"].as_u64().unwrap()).unwrap();
    kill_group(pid);
    let _ = std::fs::remove_dir_all(record["profile_dir"].as_str().unwrap());
    let mut impostor = None;
    for _ in 0..50 {
        impostor = Impostor::on(port);
        if impostor.is_some() {
            break;
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    let impostor = impostor.expect("the launched browser's port never came free");

    let snap = tabref(&home, &["snap"], false);
    let forgotten = !home.join("launched.json").exists();
    stop_daemon(&home);
    let _ = std::fs::remove_dir_all(&home);
    assert!(!snap.ok, "{}", snap.stdout);
    assert!(forgotten, "the launch record of a gone browser was kept");
    assert_eq!(impostor.contacts(), 0, "the impostor was sent a connection");
}

fn kill_group(pid: u32) {
    let _ = Command::new("kill")
        .args(["-KILL", &format!("-{pid}")])
        .status();
    std::thread::sleep(Duration::from_millis(300));
}
