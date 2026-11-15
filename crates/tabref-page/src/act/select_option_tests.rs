use serde_json::{Value, json};
use tabref_core::ErrorCode;

use super::select_option;
use crate::CdpConnection;
use crate::act::act_fixture::{deadline, resolved, session, token};
use tabref_cdp::ScriptedTransport;

fn answering(outcome: Value) -> CdpConnection {
    let transport = resolved(ScriptedTransport::new())
        .expect(
            "Runtime.callFunctionOn",
            json!({ "result": { "value": outcome } }),
        )
        .expect("Runtime.releaseObject", json!({}));
    CdpConnection::new(transport.boxed())
}

#[test]
fn a_matching_option_returns_the_new_value() {
    let mut connection =
        answering(json!({ "value": "green", "label": "Green", "previous": "Red" }));
    let chosen =
        select_option(&mut connection, &session(), &token(4), "Green", deadline()).unwrap();
    assert_eq!(chosen.value, "green");
    assert_eq!(chosen.previous.as_deref(), Some("Red"));
}

#[test]
fn a_missing_option_lists_the_available_ones() {
    let mut connection = answering(json!({ "error": "no_option", "options": ["Red", "Green"] }));
    let error =
        select_option(&mut connection, &session(), &token(4), "Blue", deadline()).unwrap_err();
    assert_eq!(error.code, ErrorCode::ActionFailed);
    assert_eq!(
        error.details.as_ref().unwrap()["options"],
        json!(["Red", "Green"])
    );
}

#[test]
fn a_non_select_is_not_supported() {
    let mut connection = answering(json!({ "error": "not_select" }));
    let error =
        select_option(&mut connection, &session(), &token(4), "Green", deadline()).unwrap_err();
    assert_eq!(error.code, ErrorCode::ActionNotSupported);
}

#[test]
fn several_options_at_the_best_rank_are_ambiguous() {
    let mut connection = answering(json!({ "error": "ambiguous", "options": ["Ada A", "Ada B"] }));
    let error =
        select_option(&mut connection, &session(), &token(4), "Ada", deadline()).unwrap_err();
    assert_eq!(error.code, ErrorCode::AmbiguousTarget);
    assert!(error.message.contains("Ada A, Ada B"), "{}", error.message);
}
