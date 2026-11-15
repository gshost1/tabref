use std::path::{Path, PathBuf};

use serde_json::json;
use tabref_cdp::{ListenerVerdict, ProcessView};
use tabref_core::{AppError, ErrorCode};

use super::{browser_is_gone, implicit_port, launched_from};

fn process(args: &[&str]) -> ProcessView {
    ProcessView {
        pid: 7,
        exe: PathBuf::from("/opt/google/chrome/chrome"),
        cmdline: args.iter().map(|arg| (*arg).to_owned()).collect(),
    }
}

#[test]
fn port_9222_is_used_only_when_a_chrome_of_this_user_holds_it() {
    assert!(implicit_port(&ListenerVerdict::Trusted, false).unwrap());
    assert!(!implicit_port(&ListenerVerdict::Nothing, false).unwrap());
    assert!(!implicit_port(&ListenerVerdict::Unknown, false).unwrap());
    let error = implicit_port(&ListenerVerdict::Untrusted("held by nc".into()), false).unwrap_err();
    assert_eq!(error.kind(), Some("cdp_endpoint_unverified"));
    let suggestion = error.suggestion.unwrap();
    assert!(suggestion.contains("--cdp 9222"), "{suggestion}");
    assert!(suggestion.contains("--launch"), "{suggestion}");
}

#[test]
fn an_untrusted_9222_is_passed_over_when_the_command_may_launch_its_own() {
    assert!(!implicit_port(&ListenerVerdict::Untrusted("held by nc".into()), true).unwrap());
}

#[test]
fn a_launch_record_is_recognised_by_its_own_profile() {
    let ours = launched_from(Path::new("/tmp/tabref-profile-a"));
    assert!(ours(&process(&[
        "chrome",
        "--user-data-dir=/tmp/tabref-profile-a"
    ])));
    assert!(!ours(&process(&[
        "chrome",
        "--user-data-dir=/tmp/tabref-profile-ab"
    ])));
    assert!(!ours(&process(&["chrome"])));
}

fn refused() -> AppError {
    AppError::new(ErrorCode::ActionFailed, "refused")
        .with_details(json!({ "kind": "cdp_endpoint_unavailable" }))
}

fn timed_out() -> AppError {
    AppError::timeout("slow").with_details(json!({ "kind": "cdp_timeout" }))
}

#[test]
fn a_refused_port_or_an_exited_process_is_gone() {
    assert!(browser_is_gone(&refused(), None));
    assert!(browser_is_gone(&refused(), Some(true)));
    assert!(browser_is_gone(&timed_out(), Some(false)));
}

#[test]
fn a_slow_browser_whose_process_lives_is_busy_not_gone() {
    assert!(!browser_is_gone(&timed_out(), Some(true)));
    assert!(!browser_is_gone(&timed_out(), None));
}
