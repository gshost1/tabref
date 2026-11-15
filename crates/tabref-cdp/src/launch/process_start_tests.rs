#![cfg_attr(not(target_os = "linux"), allow(unused_imports, dead_code))]

use std::path::PathBuf;

use super::{launched_alive, linux_start_time, process_start};
use crate::LaunchedBrowser;

fn record(pid: u32, started: Option<String>) -> LaunchedBrowser {
    LaunchedBrowser {
        pid,
        port: 40000,
        websocket_path: "/devtools/browser/x".to_owned(),
        profile_dir: PathBuf::from("/nonexistent/profile"),
        headless: true,
        started,
        profile_use: crate::ProfileUse::THROWAWAY,
    }
}

#[test]
fn the_start_time_is_read_after_a_command_name_with_spaces_and_parens() {
    let stat = "4242 (my (odd) chrome) S 1 4242 4242 0 -1 4194560 100 0 0 0 1 2 0 0 20 0 9 0 \
                987654 1000 50 18446744073709551615";
    assert_eq!(linux_start_time(stat).as_deref(), Some("987654"));
    assert_eq!(linux_start_time("garbage"), None);
}

#[cfg(any(target_os = "linux", target_os = "macos"))]
#[test]
fn a_running_process_has_a_stable_start_time_and_a_missing_one_none() {
    let me = std::process::id();
    let first = process_start(me);
    assert!(first.is_some());
    assert_eq!(process_start(me), first);
    assert_eq!(process_start(u32::MAX - 7), None);
}

#[cfg(any(target_os = "linux", target_os = "macos"))]
#[test]
fn a_launch_record_is_alive_only_for_the_same_process() {
    let me = std::process::id();
    assert_eq!(launched_alive(&record(me, process_start(me))), Some(true));
    assert_eq!(
        launched_alive(&record(me, Some("an earlier process".to_owned()))),
        Some(false)
    );
    assert_eq!(
        launched_alive(&record(u32::MAX - 7, Some("1".to_owned()))),
        Some(false)
    );
    assert_eq!(launched_alive(&record(me, None)), None);
}

#[cfg(target_os = "linux")]
#[test]
fn a_start_time_names_the_boot_it_was_read_in() {
    let boot = std::fs::read_to_string("/proc/sys/kernel/random/boot_id").unwrap();
    let start = process_start(std::process::id()).unwrap();
    assert!(start.starts_with(&format!("{}:", boot.trim())), "{start}");
}

#[cfg(target_os = "linux")]
#[test]
fn a_process_that_exited_but_was_not_reaped_counts_as_gone() {
    let mut child = std::process::Command::new("true").spawn().unwrap();
    let pid = child.id();
    let stat = format!("/proc/{pid}/stat");
    let started = std::time::Instant::now();
    while !std::fs::read_to_string(&stat).is_ok_and(|text| text.contains(") Z "))
        && started.elapsed() < std::time::Duration::from_secs(5)
    {
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    let boot = std::fs::read_to_string("/proc/sys/kernel/random/boot_id").unwrap();
    let raw = linux_start_time(&std::fs::read_to_string(&stat).unwrap()).unwrap();
    let start = process_start(pid);
    let alive = launched_alive(&record(pid, Some(format!("{}:{raw}", boot.trim()))));
    let _ = child.wait();
    assert_eq!(start, None);
    assert_eq!(alive, Some(false));
}
