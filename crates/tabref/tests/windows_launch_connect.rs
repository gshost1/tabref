//! The Windows CLI can reconnect to a recorded launch without PowerShell.

#![cfg(windows)]
#![allow(clippy::unwrap_used)]

mod common;

use std::fs::{self, OpenOptions};
use std::net::TcpListener;
use std::os::windows::fs::OpenOptionsExt;
use std::process::Command;

use common::scratch;
use tabref_cdp::{LaunchedBrowser, ProfileUse, launched_alive};
use tabref_core::RefStore;

#[test]
fn connecting_to_a_launch_does_not_run_the_profile_owner_check() {
    let home = scratch("windows-launch-connect");
    let profile = home.0.join("profile");
    fs::create_dir(&profile).unwrap();
    let lock = OpenOptions::new()
        .write(true)
        .create_new(true)
        .share_mode(0)
        .open(profile.join("lockfile"))
        .unwrap();
    let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
    let port = listener.local_addr().unwrap().port();
    let websocket_path = "/devtools/browser/test-id";
    fs::write(
        profile.join("DevToolsActivePort"),
        format!("{port}\n{websocket_path}\n"),
    )
    .unwrap();
    let record = LaunchedBrowser {
        pid: std::process::id(),
        port,
        websocket_path: websocket_path.to_owned(),
        profile_dir: profile,
        headless: true,
        started: None,
        profile_use: ProfileUse::THROWAWAY,
    };
    assert_eq!(launched_alive(&record), Some(true));
    RefStore::new(&home.0)
        .write_state("launched.json", &serde_json::to_string(&record).unwrap())
        .unwrap();

    let run = |args: &[&str]| {
        let output = Command::new(env!("CARGO_BIN_EXE_tabref"))
            .args(["--json", "--timeout", "500"])
            .args(args)
            .env("TABREF_HOME", &home.0)
            .env("TABREF_NO_DAEMON", "1")
            .env_remove("TABREF_CDP")
            .env("SystemRoot", home.0.join("no-powershell"))
            .output()
            .unwrap();
        serde_json::from_slice::<serde_json::Value>(&output.stdout).unwrap()
    };
    let managed = run(&["open", "about:blank", "--launch"]);
    assert_eq!(
        managed["error"]["details"]["kind"], "cdp_launch_unverified",
        "{managed}"
    );
    let attached = run(&["tabs"]);
    assert_ne!(
        attached["error"]["details"]["kind"], "cdp_launch_unverified",
        "{attached}"
    );
    drop(lock);
}
