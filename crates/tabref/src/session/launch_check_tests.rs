use std::cell::Cell;
use std::path::PathBuf;

use tabref_cdp::{ActivePort, LaunchedBrowser, ListenerVerdict};
use tabref_core::{AppError, ErrorCode};

use super::{LaunchCheck, decide};

const RUN: &str = "/devtools/browser/5d1c0e3a-aaaa-4bbb-8ccc-000000000001";
const LATER_RUN: &str = "/devtools/browser/5d1c0e3a-aaaa-4bbb-8ccc-000000000002";

fn record() -> LaunchedBrowser {
    LaunchedBrowser {
        pid: 4242,
        port: 51234,
        websocket_path: RUN.to_owned(),
        profile_dir: PathBuf::from("/home/u/.cache/tabref-profile-a"),
        headless: true,
        started: Some("Mon Nov 16 10:00:00 2026".to_owned()),
        persistent: false,
    }
}

fn file(port: u16, path: &str) -> Result<ActivePort, AppError> {
    Ok(ActivePort {
        port,
        websocket_path: path.to_owned(),
    })
}

fn missing() -> Result<ActivePort, AppError> {
    Err(AppError::new(
        ErrorCode::ActionFailed,
        "No DevToolsActivePort file",
    ))
}

fn off_linux(alive: Option<bool>, read: Result<ActivePort, AppError>) -> LaunchCheck {
    decide(&record(), ListenerVerdict::Unknown, &|| alive, &|| {
        read.clone()
    })
}

fn kind(check: &LaunchCheck) -> Option<&str> {
    match check {
        LaunchCheck::Unverified(error) => error.kind(),
        _ => None,
    }
}

#[test]
fn off_linux_the_recorded_browser_is_accepted_when_its_port_file_names_it() {
    assert!(matches!(
        off_linux(Some(true), file(51234, RUN)),
        LaunchCheck::Alive
    ));
    assert!(matches!(
        decide(
            &record(),
            ListenerVerdict::CantCheck("x".into()),
            &|| Some(true),
            &|| file(51234, RUN)
        ),
        LaunchCheck::Alive
    ));
}

#[test]
fn off_linux_a_live_pid_whose_profile_names_another_run_is_refused() {
    let check = off_linux(Some(true), file(51234, LATER_RUN));
    assert!(!check.is_launched());
    assert_eq!(kind(&check), Some("cdp_launch_unverified"));
}

#[test]
fn off_linux_a_live_pid_whose_profile_names_another_port_is_refused() {
    let check = off_linux(Some(true), file(9222, RUN));
    assert!(!check.is_launched());
    assert_eq!(kind(&check), Some("cdp_launch_unverified"));
}

#[test]
fn off_linux_a_missing_port_file_fails_closed_with_a_message() {
    let check = off_linux(Some(true), missing());
    assert!(!check.is_launched());
    let LaunchCheck::Unverified(error) = check else {
        panic!("not refused");
    };
    assert_eq!(error.kind(), Some("cdp_launch_unverified"));
    assert!(
        error.message.contains("DevToolsActivePort"),
        "{}",
        error.message
    );
}

#[test]
fn a_reused_pid_or_a_record_without_a_start_time_is_gone_without_reading_the_profile() {
    let read = Cell::new(false);
    for alive in [Some(false), None] {
        let check = decide(&record(), ListenerVerdict::Unknown, &|| alive, &|| {
            read.set(true);
            file(51234, RUN)
        });
        assert!(matches!(check, LaunchCheck::Gone));
    }
    assert!(!read.get());
}

#[test]
fn a_free_port_or_another_holder_is_gone() {
    for verdict in [
        ListenerVerdict::Nothing,
        ListenerVerdict::Untrusted { holder: None },
    ] {
        let check = decide(&record(), verdict, &|| Some(true), &|| file(51234, RUN));
        assert!(matches!(check, LaunchCheck::Gone));
    }
}
