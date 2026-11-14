use serde_json::json;
use tabref_core::{AppError, ErrorCode};

use super::error_text;

#[test]
fn an_error_reads_code_kind_message_detail_and_hint() {
    let error = AppError::new(ErrorCode::ActionFailed, "Another element covers it")
        .with_details(json!({ "kind": "cdp_click_intercepted" }))
        .with_platform_detail("hit 12")
        .with_suggestion("Close the overlay");
    assert_eq!(
        error_text(&error),
        "error ACTION_FAILED (cdp_click_intercepted): Another element covers it\n\
         detail: hit 12\nhint: Close the overlay"
    );
}

#[test]
fn a_bare_error_is_one_line() {
    assert_eq!(
        error_text(&AppError::internal("boom")),
        "error INTERNAL: boom"
    );
}
