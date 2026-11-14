use serde_json::json;
use tabref_core::{AppError, ErrorCode};

use super::browser_is_gone;

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
