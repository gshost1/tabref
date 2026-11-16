#![cfg_attr(not(unix), allow(unused_imports, dead_code))]

use std::path::{Path, PathBuf};

use super::may_signal;
use crate::{LaunchedBrowser, ProcessView, ProfileUse};

fn record(pid: u32, profile_use: ProfileUse) -> LaunchedBrowser {
    LaunchedBrowser {
        pid,
        port: 40000,
        websocket_path: "/devtools/browser/x".to_owned(),
        profile_dir: PathBuf::from("/tmp/tabref-profile-s0000001"),
        headless: true,
        started: Some("boot:1".to_owned()),
        profile_use,
    }
}

fn view(pid: u32, exe: &str, profile: &Path) -> ProcessView {
    ProcessView {
        pid,
        exe: PathBuf::from(exe),
        cmdline: vec![
            exe.to_owned(),
            format!("--user-data-dir={}", profile.display()),
        ],
    }
}

#[test]
fn only_a_chrome_on_the_recorded_profile_that_leads_its_group_may_be_signalled() {
    let launched = record(4242, ProfileUse::THROWAWAY);
    let profile = launched.profile_dir.clone();
    let chrome = view(4242, "/opt/google/chrome/chrome", &profile);
    assert!(may_signal(&launched, Some(4242), Some(&chrome)));

    assert!(!may_signal(
        &record(1, ProfileUse::THROWAWAY),
        Some(1),
        Some(&view(1, "/opt/google/chrome/chrome", &profile))
    ));
    assert!(!may_signal(
        &record(0, ProfileUse::THROWAWAY),
        Some(0),
        Some(&view(0, "/opt/google/chrome/chrome", &profile))
    ));
    assert!(!may_signal(&launched, Some(4000), Some(&chrome)));
    assert!(!may_signal(&launched, None, Some(&chrome)));
    assert!(!may_signal(
        &launched,
        Some(4242),
        Some(&view(4242, "/usr/bin/sleep", &profile))
    ));
    assert!(!may_signal(
        &launched,
        Some(4242),
        Some(&view(
            4242,
            "/opt/google/chrome/chrome",
            Path::new("/other")
        ))
    ));
    assert!(!may_signal(
        &launched,
        Some(4242),
        Some(&view(4243, "/opt/google/chrome/chrome", &profile))
    ));
    assert!(!may_signal(&launched, Some(4242), None));
    assert!(!may_signal(
        &record(4242, ProfileUse::ADOPTED),
        Some(4242),
        Some(&chrome)
    ));
}

#[cfg(unix)]
#[test]
fn a_program_with_the_recorded_pid_and_start_time_that_is_not_the_browser_survives() {
    use std::os::unix::process::CommandExt;
    let mut sleeper = std::process::Command::new("sleep")
        .arg("30")
        .process_group(0)
        .spawn()
        .unwrap();
    let pid = sleeper.id();
    let mut recorded = record(pid, ProfileUse::THROWAWAY);
    recorded.started = super::super::process_start::process_start(pid);
    let signalled = super::kill_recorded(&recorded);
    std::thread::sleep(std::time::Duration::from_millis(100));
    let alive = sleeper.try_wait().unwrap().is_none();
    let _ = sleeper.kill();
    let _ = sleeper.wait();
    assert!(!signalled);
    assert!(
        alive,
        "a program that is not the recorded browser was killed"
    );
}
