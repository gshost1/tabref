use serde_json::json;

use super::Response;
use crate::{AppError, ErrorCode, ErrorPayload};

#[test]
fn an_ok_envelope_carries_data_and_no_error() {
    let value = serde_json::to_value(Response::ok("snap", json!({ "ref_count": 2 }))).unwrap();
    assert_eq!(value["ok"], true);
    assert_eq!(value["command"], "snap");
    assert_eq!(value["data"]["ref_count"], 2);
    assert!(value.get("error").is_none());
    assert_eq!(value["version"], crate::ENVELOPE_VERSION);
}

#[test]
fn a_stale_ref_error_carries_a_refresh_recovery() {
    let payload = ErrorPayload::from_error(&AppError::stale_ref("@s1234567:e1"));
    let value = serde_json::to_value(Response::err("click", payload)).unwrap();
    assert_eq!(value["ok"], false);
    assert_eq!(value["error"]["code"], "STALE_REF");
    assert_eq!(
        value["error"]["recovery"]["strategy"],
        "refresh_snapshot_then_retry_original"
    );
    assert_eq!(value["error"]["disposition"]["retry"], "safe");
    assert!(value.get("data").is_none());
}

#[test]
fn an_unsafe_error_has_no_recovery_and_omits_empty_members() {
    let payload = ErrorPayload::from_error(&AppError::new(ErrorCode::Timeout, "slow"));
    let value = serde_json::to_value(payload).unwrap();
    assert!(value.get("recovery").is_none());
    assert!(value.get("details").is_none());
    assert_eq!(value["disposition"]["delivery"], "unknown");
}
