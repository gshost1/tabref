//! `tabref connect --user-chrome`: attaching to the user's own browser is an
//! explicit opt-in, checked like every other endpoint. The "user's browser"
//! here is a profile under a throwaway `XDG_CONFIG_HOME`, so the test never
//! touches the real one. The impostor test needs no browser; the other is
//! live (`TABREF_LIVE=1`).

#![cfg(target_os = "linux")]
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;
#[path = "common/user_home.rs"]
mod user_home;

use std::net::TcpListener;
use std::process::Command;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use common::live;
use user_home::{UserHome, kill, list_pages, start_user_chrome};

fn impostor() -> (u16, Arc<AtomicUsize>) {
    let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
    let port = listener.local_addr().unwrap().port();
    let contacts = Arc::new(AtomicUsize::new(0));
    let seen = Arc::clone(&contacts);
    std::thread::spawn(move || {
        for stream in listener.incoming() {
            seen.fetch_add(1, Ordering::SeqCst);
            drop(stream);
        }
    });
    (port, contacts)
}

#[test]
fn a_profile_pointing_at_another_program_is_never_contacted() {
    let home = UserHome::new("uc-impostor");
    let (port, contacts) = impostor();
    std::fs::write(
        home.profile().join("DevToolsActivePort"),
        format!("{port}\n/devtools/browser/x\n"),
    )
    .unwrap();
    std::os::unix::fs::symlink(
        format!("host-{}", std::process::id()),
        home.profile().join("SingletonLock"),
    )
    .unwrap();
    let _open = std::fs::File::create(home.profile().join("History")).unwrap();

    let without = home.tabref(&["--json", "tabs"]);
    let refused = home.tabref(&["--json", "connect", "--user-chrome"]);
    let after = home.tabref(&["--json", "tabs"]);

    assert!(!without.ok, "{}", without.stdout);
    assert!(!refused.ok, "{}", refused.stdout);
    let error = &refused.json()["error"];
    assert_eq!(
        error["details"]["kind"], "cdp_endpoint_unverified",
        "{error}"
    );
    assert!(
        !home.state.join("user_chrome.json").exists(),
        "a refused choice was kept"
    );
    assert!(!after.ok);
    assert_eq!(
        contacts.load(Ordering::SeqCst),
        0,
        "the impostor was sent a connection"
    );
}

#[test]
fn the_users_browser_is_used_only_between_connect_and_off() {
    if !live() {
        return;
    }
    let home = UserHome::new("uc-live");
    let browser = start_user_chrome(&home).expect("the user's Chrome never came up");
    let pid = browser.id();

    let before = home.tabref(&["--json", "tabs"]);
    let connected = home.tabref(&["connect", "--user-chrome"]);
    let opened = home.tabref(&["open", "data:text/html,<h1>In your Chrome</h1>"]);
    let listed = home.tabref(&["--json", "tabs"]);
    let missing = home.tabref(&["--json", "tab", "999"]);
    let pages = list_pages(&home.profile());
    let off = home.tabref(&["connect", "--off"]);
    let daemon = home.tabref(&["--json", "daemon", "status"]);
    let later = home.tabref(&["--json", "tabs"]);
    kill(browser);

    assert!(
        !before.ok,
        "used the user's browser without the opt-in: {}",
        before.stdout
    );
    assert!(connected.ok, "{}{}", connected.stdout, connected.stderr);
    assert!(
        connected.stdout.contains("attached to your Google Chrome")
            && connected.stdout.contains("profile ")
            && connected.stdout.contains(&format!("pid {pid}")),
        "{}",
        connected.stdout
    );
    assert!(opened.ok, "{}{}", opened.stdout, opened.stderr);
    let warning = "your logged-in Google Chrome";
    assert_eq!(
        opened.stdout.lines().next(),
        Some("warning: your logged-in Google Chrome")
    );
    assert!(listed.ok, "{}", listed.stdout);
    assert_eq!(
        listed.json()["data"]["warnings"],
        serde_json::json!([warning])
    );
    assert!(!missing.ok, "{}", missing.stdout);
    assert_eq!(
        missing.json()["error"]["details"]["warnings"],
        serde_json::json!([warning])
    );
    assert!(pages.contains("In your Chrome"), "{pages}");
    assert!(
        off.ok && off.stdout.contains("stopped using your Google Chrome"),
        "{}",
        off.stdout
    );
    assert_eq!(daemon.json()["data"]["running"], false, "{}", daemon.stdout);
    assert!(
        !later.ok,
        "still used the user's browser after --off: {}",
        later.stdout
    );
}

#[test]
fn open_launch_while_opted_in_uses_tabrefs_own_browser() {
    if !live() {
        return;
    }
    let home = UserHome::new("uc-launch");
    let browser = start_user_chrome(&home).expect("the user's Chrome never came up");
    let connected = home.tabref(&["connect", "--user-chrome"]);
    let launched = home.tabref(&["open", "data:text/html,<h1>Launched here</h1>", "--launch"]);
    let pages = list_pages(&home.profile());
    let record = std::fs::read_to_string(home.state.join("launched.json")).unwrap_or_default();
    let _ = home.tabref(&["daemon", "stop"]);
    kill(browser);
    if let Ok(record) = serde_json::from_str::<serde_json::Value>(&record) {
        let _ = Command::new("kill")
            .args(["-KILL", &format!("-{}", record["pid"])])
            .status();
        let _ = std::fs::remove_dir_all(record["profile_dir"].as_str().unwrap_or("/nonexistent"));
    }

    assert!(connected.ok, "{}{}", connected.stdout, connected.stderr);
    assert!(launched.ok, "{}{}", launched.stdout, launched.stderr);
    assert!(
        launched.stdout.contains("Launched here"),
        "{}",
        launched.stdout
    );
    assert!(
        !launched.stdout.contains("warning: your logged-in"),
        "{}",
        launched.stdout
    );
    assert!(
        !pages.contains("Launched here"),
        "--launch landed in the user's browser: {pages}"
    );
}

#[test]
fn a_restarted_users_browser_needs_a_new_opt_in() {
    if !live() {
        return;
    }
    let home = UserHome::new("uc-restart");
    let first = start_user_chrome(&home).expect("the user's Chrome never came up");
    let connected = home.tabref(&["connect", "--user-chrome"]);
    kill(first);
    let second = start_user_chrome(&home).expect("the user's Chrome never came back");
    let after_restart = home.tabref(&["--json", "tabs"]);
    let kept = home.state.join("user_chrome.json").exists();
    let later = home.tabref(&["--json", "tabs"]);
    kill(second);

    assert!(connected.ok, "{}{}", connected.stdout, connected.stderr);
    assert!(
        !after_restart.ok,
        "used a restarted browser: {}",
        after_restart.stdout
    );
    assert_eq!(
        after_restart.json()["error"]["details"]["kind"],
        "cdp_user_chrome_gone",
        "{}",
        after_restart.stdout
    );
    assert!(!kept, "the opt-in outlived the browser it was given for");
    assert!(
        !later.ok,
        "used the restarted browser later: {}",
        later.stdout
    );
}
