use serde_json::json;
use tabref_core::ErrorCode;

use super::evaluate;
use crate::CdpConnection;
use crate::act::act_fixture::{deadline, session};
use tabref_cdp::ScriptedTransport;

fn answer(result: serde_json::Value) -> serde_json::Value {
    let mut connection = CdpConnection::new(
        ScriptedTransport::new()
            .expect("Runtime.evaluate", result)
            .boxed(),
    );
    evaluate(&mut connection, &session(), "1 + 1", deadline()).unwrap()
}

#[test]
fn values_come_back_as_json() {
    assert_eq!(
        answer(json!({ "result": { "type": "number", "value": 2 } })),
        json!(2)
    );
    assert_eq!(
        answer(json!({ "result": { "type": "object", "value": { "a": [1] } } })),
        json!({ "a": [1] })
    );
}

#[test]
fn undefined_nodes_and_infinities_still_answer() {
    assert_eq!(
        answer(json!({ "result": { "type": "undefined" } })),
        json!(null)
    );
    assert_eq!(
        answer(
            json!({ "result": { "type": "object", "subtype": "node", "description": "div#main" } })
        ),
        json!("div#main")
    );
    assert_eq!(
        answer(json!({ "result": { "type": "number", "unserializableValue": "Infinity" } })),
        json!("Infinity")
    );
}

#[test]
fn a_thrown_exception_is_action_failed() {
    let transport = ScriptedTransport::new().expect(
        "Runtime.evaluate",
        json!({ "result": { "type": "object" },
            "exceptionDetails": { "text": "Uncaught", "exception": { "description": "ReferenceError: x is not defined" } } }),
    );
    let mut connection = CdpConnection::new(transport.boxed());

    let error = evaluate(&mut connection, &session(), "x", deadline()).unwrap_err();

    assert_eq!(error.code, ErrorCode::ActionFailed);
    assert_eq!(
        error.platform_detail.as_deref(),
        Some("ReferenceError: x is not defined")
    );
}
