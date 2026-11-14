use serde_json::json;
use tabref_core::{ErrorCode, Point};

use super::hover;
use crate::CdpConnection;
use crate::act::act_fixture::{deadline, laid_out, proven, session, token};
use tabref_cdp::ScriptedTransport;

#[test]
fn a_hover_moves_the_pointer_to_the_hit_tested_point_without_pressing() {
    let transport = laid_out(
        proven(ScriptedTransport::new()).expect("DOM.scrollIntoViewIfNeeded", json!({})),
        json!([[10, 10, 110, 10, 110, 30, 10, 30]]),
    )
    .expect("DOM.getNodeForLocation", json!({ "backendNodeId": 7 }))
    .expect("Input.dispatchMouseEvent", json!({}));
    let sent = transport.sent_log();
    let mut connection = CdpConnection::new(transport.boxed());

    let point = hover(&mut connection, &session(), &token(7), deadline()).unwrap();

    assert_eq!(point, Point { x: 60.0, y: 20.0 });
    let sent = sent.lock().unwrap();
    let moves: Vec<_> = sent
        .iter()
        .filter(|request| request["method"] == "Input.dispatchMouseEvent")
        .collect();
    assert_eq!(moves.len(), 1);
    assert_eq!(moves[0]["params"]["type"], "mouseMoved");
}

#[test]
fn a_node_of_a_replaced_document_is_stale_before_any_input() {
    let transport = ScriptedTransport::new().expect(
        "Page.getFrameTree",
        json!({ "frameTree": { "frame": { "id": "F", "loaderId": "NEW" } } }),
    );
    let mut connection = CdpConnection::new(transport.boxed());

    let error = hover(&mut connection, &session(), &token(7), deadline()).unwrap_err();

    assert_eq!(error.code, ErrorCode::StaleRef);
}
