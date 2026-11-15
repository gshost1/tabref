//! `open --launch --profile DIR` reuses the one browser on a profile: two
//! launches started together on a new profile end with one browser, and a
//! symlinked spelling of the profile reaches it too. Live (`TABREF_LIVE=1`).

#![cfg(target_os = "linux")]
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;
#[path = "common/profile_home.rs"]
mod profile_home;

use common::{harness_home, live};
use profile_home::{Dirs, launched_pid, stop_browser, tabref, tabref_command};

#[test]
fn two_launches_on_a_new_profile_share_one_browser() {
    if !live() {
        return;
    }
    let first = harness_home("reuse-a");
    let second = harness_home("reuse-b");
    let profiles = harness_home("reuse-dir");
    let config = first.with_extension("config");
    let dirs = Dirs {
        homes: vec![first.clone(), second.clone(), profiles.clone()],
    };
    let profile = profiles.join("work");
    let args = [
        "--timeout",
        "30000",
        "open",
        "about:blank",
        "--launch",
        "--profile",
        profile.to_str().unwrap(),
    ];
    let a = tabref_command(&first, &config, &args, true)
        .stdout(std::process::Stdio::piped())
        .spawn()
        .unwrap();
    let b = tabref_command(&second, &config, &args, true)
        .stdout(std::process::Stdio::piped())
        .spawn()
        .unwrap();
    let a = a.wait_with_output().unwrap();
    let b = b.wait_with_output().unwrap();
    let pids = (launched_pid(&first), launched_pid(&second));

    let alias = profiles.join("alias");
    std::os::unix::fs::symlink(&profile, &alias).unwrap();
    let third = harness_home("reuse-c");
    let via_alias = tabref(
        &third,
        &config,
        &[
            "open",
            "about:blank",
            "--launch",
            "--profile",
            alias.to_str().unwrap(),
        ],
        true,
    );
    let pid_alias = launched_pid(&third);
    let record = std::fs::read_to_string(third.join("launched.json")).unwrap_or_default();
    if let Some(pid) = pids.0 {
        stop_browser(pid);
    }
    drop(Dirs {
        homes: vec![third.clone()],
    });
    drop(dirs);

    assert!(
        a.status.success() && b.status.success(),
        "{}{}",
        String::from_utf8_lossy(&a.stderr),
        String::from_utf8_lossy(&b.stderr)
    );
    assert!(pids.0.is_some());
    assert_eq!(
        pids.0, pids.1,
        "two launches on one profile started two browsers"
    );
    assert!(via_alias.ok, "{}{}", via_alias.stdout, via_alias.stderr);
    assert_eq!(
        pid_alias, pids.0,
        "a symlinked spelling started another browser"
    );
    assert!(
        !record.contains("alias"),
        "the record kept the symlinked spelling: {record}"
    );
}
