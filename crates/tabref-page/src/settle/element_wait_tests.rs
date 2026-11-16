use serde_json::json;
use tabref_core::{DeliverySemantics, ErrorCode};

use super::read_script_failed;

#[test]
fn a_script_exception_in_a_wait_was_not_delivered() {
    let error = read_script_failed(&json!({
        "text": "Uncaught",
        "exception": { "description": "TypeError: boom" },
    }));
    assert_eq!(error.code, ErrorCode::ActionFailed);
    assert_eq!(error.disposition, DeliverySemantics::not_delivered());
    assert_eq!(error.platform_detail.as_deref(), Some("TypeError: boom"));
}
