use serde_json::{Value, json};
use tabref_core::{ErrorCode, Point};

use super::press_point;
use crate::CdpConnection;
use crate::act::act_fixture::{deadline, metrics, resolved, session, token};
use tabref_cdp::ScriptedTransport;

fn quads(page_y: f64, quads: Value) -> ScriptedTransport {
    ScriptedTransport::new()
        .expect("Page.getLayoutMetrics", metrics(page_y))
        .expect("DOM.getContentQuads", json!({ "quads": quads }))
}

fn covered(transport: ScriptedTransport, hit: i64) -> ScriptedTransport {
    transport
        .expect("DOM.getNodeForLocation", json!({ "backendNodeId": hit }))
        .expect(
            "DOM.resolveNode",
            json!({ "object": { "objectId": "target" } }),
        )
        .expect(
            "DOM.resolveNode",
            json!({ "object": { "objectId": "hit" } }),
        )
        .expect(
            "Runtime.callFunctionOn",
            json!({ "result": { "type": "object", "value": { "inside": false, "steer": false } } }),
        )
        .expect("Runtime.releaseObject", json!({}))
        .expect("Runtime.releaseObject", json!({}))
}

fn hit_tests(sent: &[Value]) -> Vec<(i64, i64)> {
    sent.iter()
        .filter(|request| request["method"] == "DOM.getNodeForLocation")
        .map(|request| {
            (
                request["params"]["x"].as_i64().unwrap(),
                request["params"]["y"].as_i64().unwrap(),
            )
        })
        .collect()
}

#[test]
fn a_scrolled_page_is_hit_tested_at_the_document_position_of_the_viewport_point() {
    let transport = quads(1_800.0, json!([[20, 570, 70, 570, 70, 596, 20, 596]]))
        .expect("DOM.getNodeForLocation", json!({ "backendNodeId": 7 }));
    let sent = transport.sent_log();
    let mut connection = CdpConnection::new(transport.boxed());

    let point = press_point(&mut connection, &session(), &token(7), deadline()).unwrap();

    assert_eq!(point, Point { x: 45.0, y: 583.0 });
    assert_eq!(hit_tests(&sent.lock().unwrap()), vec![(45, 2_383)]);
}

#[test]
fn a_wrapped_link_is_pressed_on_the_first_line_that_hits_it() {
    let lines = json!([
        [150, 10, 190, 10, 190, 26, 150, 26],
        [0, 58, 40, 58, 40, 74, 0, 74]
    ]);
    let transport = covered(quads(0.0, lines), 9)
        .expect("DOM.getNodeForLocation", json!({ "backendNodeId": 7 }));
    let sent = transport.sent_log();
    let mut connection = CdpConnection::new(transport.boxed());

    let point = press_point(&mut connection, &session(), &token(7), deadline()).unwrap();

    assert_eq!(point, Point { x: 20.0, y: 66.0 });
    assert_eq!(hit_tests(&sent.lock().unwrap()), vec![(170, 18), (20, 66)]);
}

#[test]
fn a_node_covered_on_every_line_is_refused_with_the_first_refusal() {
    let lines = json!([
        [150, 10, 190, 10, 190, 26, 150, 26],
        [0, 58, 40, 58, 40, 74, 0, 74]
    ]);
    let transport = covered(covered(quads(0.0, lines), 9), 12);
    let mut connection = CdpConnection::new(transport.boxed());

    let error = press_point(&mut connection, &session(), &token(7), deadline()).unwrap_err();

    assert_eq!(error.code, ErrorCode::ActionFailed);
    let details = error.details.unwrap();
    assert_eq!(details["kind"], "cdp_click_intercepted");
    assert_eq!(details["hit_backend_node_id"], 9);
    assert_eq!(details["point"], json!({ "x": 170.0, "y": 18.0 }));
}

#[test]
fn only_the_visible_part_of_a_tall_node_is_pressed() {
    let transport = quads(0.0, json!([[0, 400, 100, 400, 100, 1_000, 0, 1_000]]))
        .expect("DOM.getNodeForLocation", json!({ "backendNodeId": 7 }));
    let mut connection = CdpConnection::new(transport.boxed());

    let point = press_point(&mut connection, &session(), &token(7), deadline()).unwrap();

    assert_eq!(point, Point { x: 50.0, y: 500.0 });
}

/// Scripts the look for an element a boxless node draws through, finding
/// `child` (a backend node id) or nothing.
fn drawn_child(transport: ScriptedTransport, child: Option<i64>) -> ScriptedTransport {
    let found = match child {
        Some(_) => json!({ "result": { "type": "object", "objectId": "C" } }),
        None => json!({ "result": { "type": "object", "subtype": "null", "value": null } }),
    };
    let transport = resolved(transport).expect("Runtime.callFunctionOn", found);
    match child {
        Some(backend) => transport
            .expect(
                "DOM.describeNode",
                json!({ "node": { "backendNodeId": backend } }),
            )
            .expect("Runtime.releaseObject", json!({}))
            .expect("Runtime.releaseObject", json!({})),
        None => transport.expect("Runtime.releaseObject", json!({})),
    }
}

#[test]
fn a_node_with_no_boxes_has_nothing_to_press() {
    let mut connection = CdpConnection::new(drawn_child(quads(0.0, json!([])), None).boxed());

    let error = press_point(&mut connection, &session(), &token(7), deadline()).unwrap_err();

    assert_eq!(error.code, ErrorCode::ActionFailed);
    assert_eq!(error.details.unwrap()["kind"], "cdp_node_not_rendered");
}

#[test]
fn a_display_contents_host_is_pressed_where_its_child_draws() {
    let transport = drawn_child(quads(0.0, json!([])), Some(8))
        .expect(
            "DOM.getContentQuads",
            json!({ "quads": [[0, 0, 20, 0, 20, 10, 0, 10]] }),
        )
        .expect("DOM.getNodeForLocation", json!({ "backendNodeId": 8 }))
        .expect(
            "DOM.resolveNode",
            json!({ "object": { "objectId": "target" } }),
        )
        .expect(
            "DOM.resolveNode",
            json!({ "object": { "objectId": "hit" } }),
        )
        .expect(
            "Runtime.callFunctionOn",
            json!({ "result": { "type": "object", "value": { "inside": true, "steer": false } } }),
        )
        .expect("Runtime.releaseObject", json!({}))
        .expect("Runtime.releaseObject", json!({}));
    let mut connection = CdpConnection::new(transport.boxed());

    let point = press_point(&mut connection, &session(), &token(7), deadline()).unwrap();

    assert_eq!(point, Point { x: 10.0, y: 5.0 });
}

fn hit_inside(transport: ScriptedTransport, hit: i64, steer: bool) -> ScriptedTransport {
    transport
        .expect("DOM.getNodeForLocation", json!({ "backendNodeId": hit }))
        .expect(
            "DOM.resolveNode",
            json!({ "object": { "objectId": "target" } }),
        )
        .expect(
            "DOM.resolveNode",
            json!({ "object": { "objectId": "hit" } }),
        )
        .expect(
            "Runtime.callFunctionOn",
            json!({ "result": { "type": "object", "value": { "inside": true, "steer": steer } } }),
        )
        .expect("Runtime.releaseObject", json!({}))
        .expect("Runtime.releaseObject", json!({}))
}

#[test]
fn a_press_that_would_land_on_a_control_the_node_draws_moves_to_a_child_that_is_none() {
    let own = quads(0.0, json!([[0, 0, 40, 0, 40, 10, 0, 10]]));
    let steered = drawn_child(hit_inside(own, 5, true), Some(8)).expect(
        "DOM.getContentQuads",
        json!({ "quads": [[50, 0, 90, 0, 90, 10, 50, 10]] }),
    );
    let transport = hit_inside(steered, 8, false);
    let mut connection = CdpConnection::new(transport.boxed());

    let point = press_point(&mut connection, &session(), &token(7), deadline()).unwrap();

    assert_eq!(point, Point { x: 70.0, y: 5.0 });
}

#[test]
fn a_node_that_draws_only_separate_controls_is_not_pressed() {
    let own = quads(0.0, json!([[0, 0, 40, 0, 40, 10, 0, 10]]));
    let transport = drawn_child(hit_inside(own, 5, true), None);
    let mut connection = CdpConnection::new(transport.boxed());

    let error = press_point(&mut connection, &session(), &token(7), deadline()).unwrap_err();

    assert_eq!(error.kind(), Some("cdp_press_on_inner_control"));
}
