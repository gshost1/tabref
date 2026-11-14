use serde_json::json;

use super::AppError;
use crate::{DeliverySemantics, ErrorCode};

#[test]
fn builders_fill_every_field_and_kind_reads_details() {
    let error = AppError::new(ErrorCode::ActionFailed, "covered")
        .with_suggestion("close it")
        .with_platform_detail("hit 7")
        .with_details(json!({ "kind": "cdp_click_intercepted" }))
        .with_disposition(DeliverySemantics::not_delivered());

    assert_eq!(error.to_string(), "covered");
    assert_eq!(error.suggestion.as_deref(), Some("close it"));
    assert_eq!(error.platform_detail.as_deref(), Some("hit 7"));
    assert_eq!(error.kind(), Some("cdp_click_intercepted"));
}

#[test]
fn stale_ref_is_safe_to_retry() {
    let error = AppError::stale_ref("@s1234567:e3");
    assert_eq!(error.code, ErrorCode::StaleRef);
    assert_eq!(error.disposition, DeliverySemantics::not_delivered());
    assert!(error.message.contains("@s1234567:e3"));
}

#[test]
fn io_errors_become_internal_errors() {
    let error = AppError::from(std::io::Error::from(std::io::ErrorKind::NotFound));
    assert_eq!(error.code, ErrorCode::Internal);
    assert_eq!(error.kind(), Some("io"));
}
