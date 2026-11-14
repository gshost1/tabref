use std::path::Path;

use tabref_core::{Deadline, ErrorCode};

use super::{browser_args, launch_chrome};
use crate::LaunchOptions;

#[test]
fn a_headless_launch_asks_for_a_picked_port_and_a_throwaway_profile() {
    let args = browser_args(&LaunchOptions::default(), Path::new("/tmp/p"));
    assert!(args.contains(&"--headless=new".to_owned()));
    assert!(args.contains(&"--remote-debugging-port=0".to_owned()));
    assert!(args.contains(&"--user-data-dir=/tmp/p".to_owned()));
    assert_eq!(args.last().map(String::as_str), Some("about:blank"));
}

#[test]
fn a_headed_launch_keeps_its_window_and_extra_flags() {
    let options = LaunchOptions {
        headed: true,
        extra_args: vec!["--no-sandbox".into()],
        ..LaunchOptions::default()
    };
    let args = browser_args(&options, Path::new("/tmp/p"));
    assert!(!args.iter().any(|arg| arg.starts_with("--headless")));
    assert!(args.contains(&"--no-sandbox".to_owned()));
}

#[test]
fn a_binary_that_does_not_exist_fails_before_waiting() {
    let options = LaunchOptions {
        binary: Some("/nonexistent/tabref-test-browser".into()),
        profile_root: Some(std::env::temp_dir()),
        ..LaunchOptions::default()
    };
    let error = launch_chrome(&options, Deadline::after(2_000).unwrap()).unwrap_err();
    assert_eq!(error.code, ErrorCode::ActionFailed);
    assert_eq!(error.kind(), Some("chrome_spawn_failed"));
}
