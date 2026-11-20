//! The Windows CLI can reconnect to a recorded launch without PowerShell.

#![cfg(windows)]
#![allow(clippy::unwrap_used)]

mod common;

use std::fs::{self, OpenOptions};
use std::net::TcpListener;
use std::os::windows::fs::OpenOptionsExt;
use std::process::Command;

use common::harness_home;
use serde_json::json;

#[test]
fn connecting_to_a_launch_does_not_run_the_profile_owner_check() {
    let home = harness_home("windows-launch-connect");
    let profile = home.join("profile");
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
    fs::write(
        home.join("launched.json"),
        json!({
            "pid": std::process::id(), "port": port, "websocket_path": websocket_path,
            "profile_dir": profile, "headless": true, "persistent": false, "adopted": false
        })
        .to_string(),
    )
    .unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_tabref"))
        .args(["--json", "--timeout", "500", "tabs"])
        .env("TABREF_HOME", &home)
        .env("TABREF_NO_DAEMON", "1")
        .env_remove("TABREF_CDP")
        .env("SystemRoot", home.join("no-powershell"))
        .output()
        .unwrap();
    listener.set_nonblocking(true).unwrap();
    let connected = listener.accept().is_ok();
    drop(lock);
    fs::remove_dir_all(&home).unwrap();

    assert!(
        connected,
        "the command never reached the recorded port: {} {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}
