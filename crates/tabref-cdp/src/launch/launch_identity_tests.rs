use std::path::PathBuf;

use tabref_core::{AppError, ErrorCode};

use super::{browser_guid, check_active_port, same_browser_run};
use crate::{ActivePort, LaunchedBrowser};

const RUN: &str = "/devtools/browser/0b9f6c1e-1111-4a2b-9c3d-000000000001";
const OTHER_RUN: &str = "/devtools/browser/0b9f6c1e-1111-4a2b-9c3d-000000000002";

fn record() -> LaunchedBrowser {
    LaunchedBrowser {
        pid: 4242,
        port: 51234,
        websocket_path: RUN.to_owned(),
        profile_dir: PathBuf::from("/home/u/.cache/tabref-profile-a"),
        headless: true,
        started: Some("x".to_owned()),
        persistent: false,
    }
}

fn file(port: u16, path: &str) -> Result<ActivePort, AppError> {
    Ok(ActivePort {
        port,
        websocket_path: path.to_owned(),
    })
}

#[test]
fn the_id_is_the_last_part_of_a_browser_path_only() {
    assert_eq!(
        browser_guid(RUN),
        Some("0b9f6c1e-1111-4a2b-9c3d-000000000001")
    );
    assert_eq!(browser_guid("/devtools/browser/"), None);
    assert_eq!(browser_guid("/devtools/page/ABC"), None);
    assert_eq!(browser_guid("/devtools/browser/a/b"), None);
    assert_eq!(browser_guid("/devtools/browser/a?x"), None);
    assert_eq!(browser_guid(""), None);
}

#[test]
fn only_the_recorded_run_is_the_same_browser() {
    assert!(same_browser_run(&record(), RUN));
    assert!(!same_browser_run(&record(), OTHER_RUN));
    assert!(!same_browser_run(&record(), "/devtools/browser/"));
    let mut bare = record();
    bare.websocket_path = "/devtools/browser/".to_owned();
    assert!(!same_browser_run(&bare, "/devtools/browser/"));
}

#[test]
fn a_port_file_naming_the_record_is_accepted() {
    assert!(check_active_port(&record(), file(51234, RUN)).is_ok());
}

#[test]
fn a_port_file_naming_another_port_or_run_is_refused() {
    for (port, path) in [
        (51234, OTHER_RUN),
        (51235, RUN),
        (51234, "/devtools/browser/"),
    ] {
        let error = check_active_port(&record(), file(port, path)).unwrap_err();
        assert_eq!(error.code, ErrorCode::PermDenied);
        assert_eq!(error.kind(), Some("cdp_launch_unverified"));
        assert!(error.message.contains("pid 4242"), "{}", error.message);
        assert!(!error.message.contains("0b9f6c1e"), "{}", error.message);
    }
}

#[test]
fn a_missing_port_file_is_refused_with_its_path() {
    let missing = Err(AppError::new(
        ErrorCode::ActionFailed,
        "No DevToolsActivePort file",
    ));
    let error = check_active_port(&record(), missing).unwrap_err();
    assert_eq!(error.kind(), Some("cdp_launch_unverified"));
    assert!(
        error
            .message
            .contains("/home/u/.cache/tabref-profile-a/DevToolsActivePort could not be read"),
        "{}",
        error.message
    );
    assert!(
        error
            .suggestion
            .as_deref()
            .is_some_and(|text| text.starts_with("Quit that browser (pid 4242)"))
    );
}
