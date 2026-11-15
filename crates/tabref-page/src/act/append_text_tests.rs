use serde_json::{Value, json};
use tabref_core::ErrorCode;

use super::{append_text, appended};
use crate::CdpConnection;
use crate::act::act_fixture::{deadline, resolved, session, token};
use tabref_cdp::ScriptedTransport;

fn prepared(reply: Value) -> ScriptedTransport {
    resolved(ScriptedTransport::new()).expect(
        "Runtime.callFunctionOn",
        json!({ "result": { "type": "object", "value": reply } }),
    )
}

fn read_back(transport: ScriptedTransport, value: &str) -> ScriptedTransport {
    transport
        .expect(
            "Runtime.callFunctionOn",
            json!({ "result": { "type": "string", "value": value } }),
        )
        .expect("Runtime.releaseObject", json!({}))
}

fn methods(sent: &[Value]) -> Vec<String> {
    sent.iter()
        .map(|request| request["method"].as_str().unwrap_or_default().to_owned())
        .collect()
}

#[test]
fn text_is_inserted_after_the_caret_and_read_back_whole() {
    let transport = read_back(
        prepared(json!({ "editable": true, "before": "Hello", "caret": "set" }))
            .expect("Input.insertText", json!({})),
        "Hello world",
    );
    let sent = transport.sent_log();
    let mut connection = CdpConnection::new(transport.boxed());

    let outcome =
        append_text(&mut connection, &session(), &token(5), " world", deadline()).unwrap();

    assert_eq!(outcome.previous, "Hello");
    assert_eq!(outcome.observed, "Hello world");
    assert!(outcome.verified);
    let sent = sent.lock().unwrap();
    assert_eq!(sent[3]["params"]["text"], " world");
}

#[test]
fn an_input_with_no_selection_api_gets_an_end_key_before_the_text() {
    let transport = read_back(
        prepared(json!({ "editable": true, "before": "a@b", "caret": "end-key" }))
            .expect("Input.dispatchKeyEvent", json!({}))
            .expect("Input.dispatchKeyEvent", json!({}))
            .expect("Input.insertText", json!({})),
        "a@b.com",
    );
    let sent = transport.sent_log();
    let mut connection = CdpConnection::new(transport.boxed());

    let outcome = append_text(&mut connection, &session(), &token(5), ".com", deadline()).unwrap();

    assert!(outcome.verified);
    let sent = sent.lock().unwrap();
    let methods = methods(&sent);
    let end = methods
        .iter()
        .position(|method| method == "Input.dispatchKeyEvent")
        .unwrap();
    let insert = methods
        .iter()
        .position(|method| method == "Input.insertText")
        .unwrap();
    assert!(end < insert);
    assert_eq!(sent[end]["params"]["key"], "End");
}

#[test]
fn a_node_that_is_not_a_text_field_is_refused_before_any_input() {
    let transport = prepared(json!({ "editable": false, "reason": "input type checkbox" }))
        .expect("Runtime.releaseObject", json!({}));
    let sent = transport.sent_log();
    let mut connection = CdpConnection::new(transport.boxed());

    let error =
        append_text(&mut connection, &session(), &token(5), "LEAK", deadline()).unwrap_err();

    assert_eq!(error.code, ErrorCode::ActionNotSupported);
    assert_eq!(error.kind(), Some("cdp_not_editable"));
    assert!(
        methods(&sent.lock().unwrap())
            .iter()
            .all(|method| !method.starts_with("Input."))
    );
}

#[test]
fn a_number_input_points_at_fill() {
    let transport =
        prepared(json!({ "whole": "number" })).expect("Runtime.releaseObject", json!({}));
    let mut connection = CdpConnection::new(transport.boxed());

    let error = append_text(&mut connection, &session(), &token(5), "3", deadline()).unwrap_err();

    assert_eq!(error.kind(), Some("cdp_whole_value_input"));
    assert!(
        error
            .message
            .starts_with("type adds text to a text field; use fill")
    );
}

#[test]
fn only_the_whole_typed_text_after_the_old_content_verifies() {
    assert!(appended("Hello", " world", "Hello world"));
    assert!(!appended("Hello", "o", "Hello"));
    assert!(appended("Dear", " Ada", "Dear\u{a0}Ada"));
    assert!(appended("Once", " a\n", "Once a\n"));
    assert!(appended("Once\n", "x", "Once\nx"));
    assert!(!appended("12", "3", "312"));
}
