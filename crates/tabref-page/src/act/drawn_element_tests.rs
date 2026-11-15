use serde_json::json;

use super::{DRAWN_LABEL, drawn_element};
use crate::CdpConnection;
use crate::act::act_fixture::{deadline, session, token};
use tabref_cdp::ScriptedTransport;

#[test]
fn a_drawn_label_becomes_a_token_in_the_same_document() {
    let transport = ScriptedTransport::new()
        .expect(
            "Runtime.callFunctionOn",
            json!({ "result": { "type": "object", "objectId": "L" } }),
        )
        .expect(
            "DOM.describeNode",
            json!({ "node": { "backendNodeId": 9 } }),
        )
        .expect("Runtime.releaseObject", json!({}));
    let mut connection = CdpConnection::new(transport.boxed());

    let label = drawn_element(
        &mut connection,
        &session(),
        ("O", DRAWN_LABEL),
        &token(3),
        deadline(),
    )
    .unwrap()
    .unwrap();

    assert_eq!(label.backend_node_id, 9);
    assert_eq!(label.loader_id, token(3).loader_id);
}

#[test]
fn no_drawn_label_is_none() {
    let transport = ScriptedTransport::new().expect(
        "Runtime.callFunctionOn",
        json!({ "result": { "type": "object", "subtype": "null", "value": null } }),
    );
    let mut connection = CdpConnection::new(transport.boxed());

    let label = drawn_element(
        &mut connection,
        &session(),
        ("O", DRAWN_LABEL),
        &token(3),
        deadline(),
    )
    .unwrap();

    assert_eq!(label, None);
}
