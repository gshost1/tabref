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
    let transport = ScriptedTransport::new()
        .expect(
            "Runtime.evaluate",
            json!({ "result": { "type": "object" },
                "exceptionDetails": { "text": "Uncaught", "exception": { "description": "ReferenceError: x is not defined" } } }),
        )
        .expect("Runtime.releaseObjectGroup", json!({}));
    let mut connection = CdpConnection::new(transport.boxed());

    let error = evaluate(&mut connection, &session(), "x", deadline()).unwrap_err();

    assert_eq!(error.code, ErrorCode::ActionFailed);
    assert_eq!(error.kind(), Some("js_exception"));
    assert_eq!(
        error.platform_detail.as_deref(),
        Some("ReferenceError: x is not defined")
    );
}

#[test]
fn a_returned_promise_is_awaited_for_its_value() {
    let transport = ScriptedTransport::new()
        .expect(
            "Runtime.evaluate",
            json!({ "result": { "type": "object", "subtype": "promise", "objectId": "p1" } }),
        )
        .expect(
            "Runtime.awaitPromise",
            json!({ "result": { "type": "number", "value": 42 } }),
        )
        .expect("Runtime.releaseObjectGroup", json!({}));
    let sent = transport.sent_log();
    let mut connection = CdpConnection::new(transport.boxed());
    let value = evaluate(&mut connection, &session(), "p()", deadline()).unwrap();
    assert_eq!(value, json!(42));
    let sent = sent.lock().unwrap();
    assert_eq!(sent[1]["params"]["promiseObjectId"], "p1");
    assert_eq!(sent[1]["params"]["returnByValue"], true);
}

#[test]
fn a_rejected_promise_is_action_failed_with_its_reason() {
    let transport = ScriptedTransport::new()
        .expect(
            "Runtime.evaluate",
            json!({ "result": { "type": "object", "subtype": "promise", "objectId": "p1" } }),
        )
        .expect(
            "Runtime.awaitPromise",
            json!({ "result": { "type": "object" },
                "exceptionDetails": { "text": "Uncaught (in promise)", "exception": { "description": "Error: quota gone" } } }),
        )
        .expect("Runtime.releaseObjectGroup", json!({}));
    let mut connection = CdpConnection::new(transport.boxed());
    let error = evaluate(&mut connection, &session(), "p()", deadline()).unwrap_err();
    assert_eq!(error.code, ErrorCode::ActionFailed);
    assert!(error.message.contains("rejected"), "{}", error.message);
    assert_eq!(error.platform_detail.as_deref(), Some("Error: quota gone"));
}

#[test]
fn an_object_comes_back_by_value() {
    let transport = ScriptedTransport::new()
        .expect(
            "Runtime.evaluate",
            json!({ "result": { "type": "object", "objectId": "o1" } }),
        )
        .expect(
            "Runtime.callFunctionOn",
            json!({ "result": { "type": "object", "value": { "rows": 3 } } }),
        )
        .expect("Runtime.releaseObjectGroup", json!({}));
    let mut connection = CdpConnection::new(transport.boxed());
    let value = evaluate(&mut connection, &session(), "o", deadline()).unwrap();
    assert_eq!(value, json!({ "rows": 3 }));
}
