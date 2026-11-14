use serde_json::json;
use tabref_core::{AppError, DeliverySemantics, ErrorCode};

use super::{CODES, WireError};

#[test]
fn every_member_of_an_error_survives_the_wire() {
    let error = AppError::new(ErrorCode::StaleRef, "gone")
        .with_suggestion("snap again")
        .with_platform_detail("loader changed")
        .with_details(json!({ "kind": "cdp_document_replaced" }))
        .with_disposition(DeliverySemantics::not_delivered());

    let text = serde_json::to_string(&WireError::from(&error)).unwrap();
    let back: AppError = serde_json::from_str::<WireError>(&text).unwrap().into();

    assert_eq!(back, error);
}

#[test]
fn every_code_round_trips() {
    for code in CODES {
        let back: AppError = WireError::from(&AppError::new(code, "x")).into();
        assert_eq!(back.code, code);
    }
}
