use tabref_core::{AppError, ErrorCode};

use super::failed_method;

#[test]
fn a_refused_follow_up_is_named_not_the_attach() {
    let refused = AppError::new(ErrorCode::ActionFailed, "DevTools rejected 'Page.enable'")
        .with_details(serde_json::json!({ "kind": "cdp_protocol_error", "method": "Page.enable" }));
    let bare = AppError::new(ErrorCode::ActionFailed, "no details");

    assert_eq!(failed_method(&refused), "Page.enable");
    assert_eq!(failed_method(&bare), "Target.attachToTarget");
}
