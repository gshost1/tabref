use std::path::{Path, PathBuf};

use serde_json::json;
use tabref_cdp::{ListenerVerdict, ProcessView};
use tabref_core::{AppError, ErrorCode};

use super::{browser_is_gone, implicit_port, launched_check, launched_from};

fn process(args: &[&str]) -> ProcessView {
    ProcessView {
        pid: 7,
        exe: PathBuf::from("/opt/google/chrome/chrome"),
        cmdline: args.iter().map(|arg| (*arg).to_owned()).collect(),
    }
}

fn nc() -> ListenerVerdict {
    ListenerVerdict::Untrusted {
        holder: Some("nc (pid 9)".into()),
    }
}

#[test]
fn port_9222_is_refused_unless_a_chrome_of_this_user_holds_it() {
    assert!(
        implicit_port(ListenerVerdict::Nothing, false)
            .unwrap()
            .is_none()
    );
    assert!(
        implicit_port(ListenerVerdict::Unknown, false)
            .unwrap()
            .is_none()
    );
    for refused in [
        nc(),
        ListenerVerdict::Untrusted { holder: None },
        ListenerVerdict::CantCheck("unreadable".into()),
    ] {
        let error = implicit_port(refused, false).unwrap_err();
        assert_eq!(error.kind(), Some("cdp_endpoint_unverified"));
        assert!(
            error
                .suggestion
                .unwrap()
                .starts_with("Run 'tabref open <url> --launch'")
        );
    }
}

#[test]
fn an_untrusted_9222_is_passed_over_when_the_command_may_launch_its_own() {
    assert!(implicit_port(nc(), true).unwrap().is_none());
    assert!(
        implicit_port(ListenerVerdict::CantCheck("x".into()), true)
            .unwrap()
            .is_none()
    );
}

#[cfg(target_os = "linux")]
#[test]
fn a_launch_record_whose_port_is_free_is_not_used() {
    let record = tabref_cdp::LaunchedBrowser {
        pid: std::process::id(),
        port: free_port(),
        websocket_path: "/devtools/browser/x".to_owned(),
        profile_dir: PathBuf::from("/nonexistent/tabref-profile"),
        headless: true,
        started: tabref_cdp::process_start(std::process::id()),
    };
    assert!(launched_check(&record).is_none());
}

#[cfg(target_os = "linux")]
fn free_port() -> u16 {
    let listener = std::net::TcpListener::bind(("127.0.0.1", 0)).unwrap();
    listener.local_addr().unwrap().port()
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
