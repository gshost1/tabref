use serde_json::{Value, json};
use tabref_core::ErrorCode;

use super::select_option;
use crate::CdpConnection;
use crate::act::act_fixture::{deadline, resolved, session, token};
use tabref_cdp::ScriptedTransport;

fn call(transport: ScriptedTransport, reply: Value) -> ScriptedTransport {
    resolved(transport)
        .expect(
            "Runtime.callFunctionOn",
            json!({ "result": { "value": reply } }),
        )
        .expect("Runtime.releaseObject", json!({}))
}

fn reading(options: Value) -> ScriptedTransport {
    call(ScriptedTransport::new(), json!({ "options": options }))
}

fn colours() -> Value {
    json!([
        { "label": "Red", "text": "Red", "value": "c1", "disabled": false },
        { "label": "Green", "text": "Green", "value": "c2", "disabled": false },
        { "label": "Grey", "text": "Grey", "value": "c3", "disabled": true },
    ])
}

#[test]
fn the_printed_form_chooses_the_option_and_returns_the_new_value() {
    let transport = call(
        reading(colours()),
        json!({ "value": "c2", "label": "Green", "previous": "Red" }),
    );
    let mut connection = CdpConnection::new(transport.boxed());
    let chosen = select_option(
        &mut connection,
        &session(),
        &token(4),
        "Green (c2)",
        deadline(),
    )
    .unwrap();
    assert_eq!(chosen.value, "c2");
    assert_eq!(chosen.previous.as_deref(), Some("Red"));
}

#[test]
fn a_missing_option_lists_the_options_as_printed() {
    let mut connection = CdpConnection::new(reading(colours()).boxed());
    let error =
        select_option(&mut connection, &session(), &token(4), "Blue", deadline()).unwrap_err();
    assert_eq!(error.code, ErrorCode::ActionFailed);
    assert_eq!(
        error.message,
        "no option matches 'Blue'; options: Red (c1) | Green (c2) | Grey (c3) (disabled)"
    );
    assert_eq!(
        error.details.as_ref().unwrap()["options"],
        json!(["Red (c1)", "Green (c2)", "Grey (c3)"])
    );
}

#[test]
fn a_disabled_option_is_refused_by_name() {
    let mut connection = CdpConnection::new(reading(colours()).boxed());
    let error =
        select_option(&mut connection, &session(), &token(4), "grey", deadline()).unwrap_err();
    assert_eq!(error.code, ErrorCode::ActionFailed);
    assert!(
        error.message.starts_with("option 'Grey (c3)' is disabled"),
        "{}",
        error.message
    );
}

#[test]
fn an_element_with_no_select_or_aria_pattern_is_not_supported() {
    let transport = call(ScriptedTransport::new(), json!({ "error": "not_select" }));
    let transport = call(transport, json!({ "kind": "none", "describe": "<div>" }));
    let mut connection = CdpConnection::new(transport.boxed());
    let error =
        select_option(&mut connection, &session(), &token(4), "Green", deadline()).unwrap_err();
    assert_eq!(error.code, ErrorCode::ActionNotSupported);
    assert_eq!(error.kind(), Some("select_unsupported"));
    assert!(
        error.message.contains("is a <div>, not a <select>"),
        "{}",
        error.message
    );
}

#[test]
fn several_options_at_the_best_rank_are_ambiguous() {
    let twins = json!([
        { "label": "Ada A", "text": "Ada A", "value": "1", "disabled": false },
        { "label": "Ada B", "text": "Ada B", "value": "2", "disabled": false },
    ]);
    let mut connection = CdpConnection::new(reading(twins).boxed());
    let error =
        select_option(&mut connection, &session(), &token(4), "Ada", deadline()).unwrap_err();
    assert_eq!(error.code, ErrorCode::AmbiguousTarget);
    assert_eq!(
        error.message,
        "'Ada' matches 2 options: Ada A (1) | Ada B (2); name one as listed"
    );
}

#[test]
fn a_long_select_lists_thirty_options_in_details_and_says_how_many() {
    let many: Vec<Value> = (0..10_000)
        .map(|n| json!({ "label": format!("Item {n}"), "text": format!("Item {n}"), "value": format!("i{n}"), "disabled": false }))
        .collect();
    let mut connection = CdpConnection::new(reading(json!(many)).boxed());
    let error =
        select_option(&mut connection, &session(), &token(4), "Blue", deadline()).unwrap_err();
    let details = error.details.as_ref().unwrap();
    assert_eq!(details["options"].as_array().unwrap().len(), 30);
    assert_eq!(details["options_total"], json!(10_000));
    assert!(error.message.len() < 1_000, "{}", error.message.len());
}
