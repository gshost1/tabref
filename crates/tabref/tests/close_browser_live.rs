//! `tabref close --browser` on a real Chrome: after `open --launch`
//! through the daemon it closes the browser (it was asked, not killed),
//! removes the throwaway profile and the record, and no later command is
//! served by a connection to it. On a `--profile` the close lets the
//! profile keep what the page stored. Live (`TABREF_LIVE=1`).

#![cfg(target_os = "linux")]
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;
#[path = "common/profile_home.rs"]
mod profile_home;

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use common::{harness_home, live, serve};
use profile_home::{Dirs, launched_pid, stop_browser, tabref};

const PAGE: &str = "<!doctype html><title>Keep</title><h1>Keep</h1>";

fn profile_of(home: &Path) -> Option<PathBuf> {
    let text = std::fs::read_to_string(home.join("launched.json")).ok()?;
    let record: serde_json::Value = serde_json::from_str(&text).ok()?;
    record["profile_dir"].as_str().map(PathBuf::from)
}

fn gone(pid: u32) -> bool {
    let proc = PathBuf::from(format!("/proc/{pid}"));
    let started = Instant::now();
    while proc.exists() && started.elapsed() < Duration::from_secs(10) {
        std::thread::sleep(Duration::from_millis(50));
    }
    !proc.exists()
}

#[test]
fn close_browser_quits_the_launched_browser_and_removes_its_throwaway_profile() {
    if !live() {
        return;
    }
    let home = harness_home("close-live");
    let config = home.with_extension("config");
    let dirs = Dirs {
        homes: vec![home.clone()],
    };
    let opened = tabref(
        &home,
        &config,
        &["--timeout", "30000", "open", "about:blank", "--launch"],
        false,
    );
    let pid = launched_pid(&home);
    let profile = profile_of(&home);
    let closed = tabref(&home, &config, &["--json", "close", "--browser"], false);
    let ended = pid.is_some_and(gone);
    if !ended {
        if let Some(pid) = pid {
            stop_browser(pid);
        }
    }
    let after = tabref(&home, &config, &["--json", "tabs"], false);
    let recorded = home.join("launched.json").exists();
    drop(dirs);

    assert!(opened.ok, "{}{}", opened.stdout, opened.stderr);
    assert!(closed.ok, "{}{}", closed.stdout, closed.stderr);
    assert!(ended, "the launched browser is still running");
    assert_eq!(closed.json()["data"]["graceful"], true, "{}", closed.stdout);
    let profile = profile.unwrap();
    assert!(
        !profile.exists(),
        "the throwaway profile {} was left",
        profile.display()
    );
    assert!(!recorded, "the launch record was kept");
    assert!(
        !after.ok,
        "a command was served after the browser quit: {}",
        after.stdout
    );
}

#[test]
fn close_browser_on_a_profile_keeps_what_the_page_stored() {
    if !live() {
        return;
    }
    let home = harness_home("close-live-p");
    let config = home.with_extension("config");
    let profiles = harness_home("close-live-p-dir");
    let dirs = Dirs {
        homes: vec![home.clone(), profiles.clone()],
    };
    let profile = profiles.join("work");
    let url = format!("http://127.0.0.1:{}/", serve(&[("/", PAGE)]));
    let launch = [
        "--timeout",
        "30000",
        "open",
        url.as_str(),
        "--launch",
        "--profile",
        profile.to_str().unwrap(),
    ];
    let opened = tabref(&home, &config, &launch, false);
    let stored = tabref(
        &home,
        &config,
        &["js", "localStorage.setItem('kept', 'yes'); 1"],
        false,
    );
    let pid = launched_pid(&home);
    let closed = tabref(&home, &config, &["--json", "close", "--browser"], false);
    let ended = pid.is_some_and(gone);
    let kept = profile.join(".tabref-profile").exists();
    let relaunched = tabref(&home, &config, &launch, false);
    let read = tabref(
        &home,
        &config,
        &["js", "localStorage.getItem('kept')"],
        false,
    );
    let _ = tabref(&home, &config, &["close", "--browser"], false);
    for pid in [pid, launched_pid(&home)].into_iter().flatten() {
        stop_browser(pid);
    }
    drop(dirs);

    assert!(opened.ok && stored.ok, "{}{}", opened.stderr, stored.stderr);
    assert!(closed.ok, "{}{}", closed.stdout, closed.stderr);
    assert!(ended, "the browser on the profile is still running");
    assert_eq!(closed.json()["data"]["profile_kept"], true);
    assert!(kept, "the profile was removed");
    assert!(relaunched.ok, "{}{}", relaunched.stdout, relaunched.stderr);
    assert!(
        read.stdout.contains("yes"),
        "the profile lost what the page stored: {}{}",
        read.stdout,
        read.stderr
    );
}

#[test]
fn rewriting_devtools_active_port_refuses_close_and_keeps_live_chrome_running() {
    if !live() {
        return;
    }
    let home = harness_home("close-live-mismatch");
    let config = home.with_extension("config");
    let profiles = harness_home("close-live-mismatch-dir");
    let _dirs = Dirs {
        homes: vec![home.clone(), profiles.clone()],
    };
    let profile = profiles.join("work");
    let launched = profile_home::launch_on(&home, &config, &profile);
    assert!(launched.ok, "{}{}", launched.stdout, launched.stderr);
    let pid = launched_pid(&home).unwrap();
    let active_port = profile.join("DevToolsActivePort");
    let original = std::fs::read_to_string(&active_port).unwrap();
    let port = original.lines().next().unwrap();
    std::fs::write(
        &active_port,
        format!("{port}\n/devtools/browser/another-run\n"),
    )
    .unwrap();
    let refused = tabref(&home, &config, &["--json", "close", "--browser"], true);
    assert!(!refused.ok, "{}", refused.stdout);
    assert_eq!(
        refused.json()["error"]["details"]["kind"],
        "cdp_launch_unverified"
    );
    assert_eq!(launched_pid(&home), Some(pid));

    std::fs::write(&active_port, original).unwrap();
    let tabs = tabref(&home, &config, &["--json", "tabs"], true);
    assert!(tabs.ok, "{}{}", tabs.stdout, tabs.stderr);
    assert_eq!(launched_pid(&home), Some(pid));
    let closed = tabref(&home, &config, &["--json", "close", "--browser"], true);
    assert!(closed.ok, "{}{}", closed.stdout, closed.stderr);
    assert_eq!(closed.json()["data"]["graceful"], true);
}
