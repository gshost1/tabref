//! A `DevToolsActivePort` an exited browser left behind can name the port
//! another browser now debugs on (seen on the Mac: Chrome's file from weeks
//! before said 9222, which Ego was using). tabref must never take that port
//! for the exited browser's profile. Here the "user's" Chrome profile has
//! only the stale file, and a live Chrome on the Chromium profile plays the
//! other browser. Live (`TABREF_LIVE=1`), Linux.

#![cfg(target_os = "linux")]
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;
#[path = "common/user_home.rs"]
#[allow(dead_code)]
mod user_home;

use std::os::unix::process::CommandExt;
use std::path::Path;
use std::process::{Child, Command, Stdio};
use std::time::Duration;

use common::live;
use tabref_cdp::{find_chrome, profile_lock_pid, read_active_port};
use user_home::{UserHome, kill};

/// The other browser, killed when dropped, so a failed test leaves none.
struct Other(Option<Child>);

impl Drop for Other {
    fn drop(&mut self) {
        if let Some(child) = self.0.take() {
            kill(child);
        }
    }
}

/// A headless Chrome debugging on the profile at `dir`.
fn start_on(dir: &Path) -> Other {
    std::fs::create_dir_all(dir).unwrap();
    let browser = Command::new(find_chrome().unwrap())
        .args([
            format!("--user-data-dir={}", dir.display()),
            "--headless=new".to_owned(),
            "--no-first-run".to_owned(),
            "--remote-debugging-port=0".to_owned(),
            "about:blank".to_owned(),
        ])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .process_group(0)
        .spawn()
        .unwrap();
    let up = (0..100).any(|_| {
        std::thread::sleep(Duration::from_millis(100));
        read_active_port(dir).is_ok() && profile_lock_pid(dir).is_some()
    });
    let other = Other(Some(browser));
    assert!(up, "the other browser never came up");
    other
}

/// The user's Chrome profile with only a port file left from an earlier
/// run, naming the port the other browser on `other` debugs on now.
fn stale_port_file(home: &UserHome, other: &Path) {
    let port = read_active_port(other).unwrap();
    std::fs::write(
        home.profile().join("DevToolsActivePort"),
        format!("{}\n{}\n", port.port, port.websocket_path),
    )
    .unwrap();
}

#[test]
fn a_stale_port_file_naming_another_browsers_port_is_refused_for_its_profile() {
    if !live() {
        return;
    }
    let home = UserHome::new("uc-stale-port");
    let other_dir = home.config.join("chromium");
    let other = start_on(&other_dir);
    stale_port_file(&home, &other_dir);
    let named = home.tabref(&[
        "--json",
        "connect",
        "--user-chrome",
        "--user-data-dir",
        home.profile().to_str().unwrap(),
    ]);
    let kept = home.state.join("user_chrome.json").exists();
    drop(other);

    assert!(
        !named.ok,
        "attached through a stale port file: {}",
        named.stdout
    );
    assert_eq!(
        named.json()["error"]["details"]["kind"],
        "cdp_user_chrome_not_found",
        "{}",
        named.stdout
    );
    assert!(!kept, "a refused profile was opted in to");
}

#[test]
fn a_plain_connect_names_the_browser_that_really_holds_the_port() {
    if !live() {
        return;
    }
    let home = UserHome::new("uc-stale-plain");
    let other_dir = home.config.join("chromium");
    let other = start_on(&other_dir);
    stale_port_file(&home, &other_dir);
    let connected = home.tabref(&["--json", "connect", "--user-chrome"]);
    let record = std::fs::read_to_string(home.state.join("user_chrome.json")).unwrap_or_default();
    let _ = home.tabref(&["connect", "--off"]);
    drop(other);

    assert!(connected.ok, "{}{}", connected.stdout, connected.stderr);
    let data = &connected.json()["data"];
    assert!(
        data.to_string().contains("Chromium"),
        "the report did not name the browser it attached to: {data}"
    );
    assert!(
        record.contains("chromium") && !record.contains("google-chrome"),
        "the opt-in names the wrong profile: {record}"
    );
}

#[test]
fn a_stale_lock_pointing_at_another_browser_is_not_taken_for_the_profile() {
    if !live() {
        return;
    }
    let home = UserHome::new("uc-stale-lock");
    let other_dir = home.config.join("chromium");
    let other = start_on(&other_dir);
    stale_port_file(&home, &other_dir);
    std::os::unix::fs::symlink(
        format!("host-{}", profile_lock_pid(&other_dir).unwrap()),
        home.profile().join("SingletonLock"),
    )
    .unwrap();
    let named = home.tabref(&[
        "--json",
        "connect",
        "--user-chrome",
        "--user-data-dir",
        home.profile().to_str().unwrap(),
    ]);
    let kept = home.state.join("user_chrome.json").exists();
    let plain = home.tabref(&["--json", "connect", "--user-chrome"]);
    let plain_record =
        std::fs::read_to_string(home.state.join("user_chrome.json")).unwrap_or_default();
    let _ = home.tabref(&["connect", "--off"]);
    drop(other);

    assert!(
        !named.ok,
        "took the other browser for this profile: {}",
        named.stdout
    );
    assert_eq!(
        named.json()["error"]["details"]["kind"],
        "cdp_user_chrome_not_found",
        "{}",
        named.stdout
    );
    assert!(!kept, "a refused profile was opted in to");
    assert!(plain.ok, "{}{}", plain.stdout, plain.stderr);
    assert!(
        plain_record.contains("chromium") && !plain_record.contains("google-chrome"),
        "a plain connect took the stale profile: {plain_record}"
    );
}
