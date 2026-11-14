use serde_json::json;
use tabref_core::{Deadline, ErrorCode};

use super::snapshot_page;
use crate::CdpConnection;
use crate::tree::frame_skip_reason::FrameSkipReason;
use crate::tree::snapshot_fixture::{
    attached, capture, child_tree, frame_tree, main_frame_only, page_reads, release, session,
};
use tabref_cdp::ScriptedTransport;

fn deadline() -> Deadline {
    Deadline::after(1_000).unwrap()
}

#[test]
fn frames_are_grafted_under_their_owner_and_origins_follow_preorder() {
    let transport = page_reads(Vec::new(), frame_tree(), capture())
        .expect("Accessibility.getFullAXTree", child_tree())
        .expect("DOM.getFrameOwner", json!({ "backendNodeId": 30 }))
        .expect_error(
            "Accessibility.getFullAXTree",
            -32000,
            "Frame with the given id was not found.",
        )
        .expect("DOM.getFrameOwner", json!({}));
    let transport = release(transport, 1);
    let sent = transport.sent_log();
    let mut connection = CdpConnection::new(transport.boxed());

    let snapshot = snapshot_page(&mut connection, &session(), deadline()).unwrap();

    assert_eq!(snapshot.unreadable_frames, ["REMOTE"]);
    assert_eq!(
        snapshot.skipped_frames[0].reason,
        FrameSkipReason::Unreadable
    );
    let preorder = snapshot.preorder();
    let summary: Vec<(&str, Option<&str>, Option<i64>)> = preorder
        .iter()
        .map(|(node, origin)| {
            (
                node.role.as_str(),
                node.identity.name.as_deref(),
                origin.map(|origin| origin.backend_node_id),
            )
        })
        .collect();
    assert_eq!(
        summary,
        [
            ("webarea", Some("Top"), Some(1)),
            ("button", Some("Go"), Some(20)),
            ("group", None, Some(30)),
            ("webarea", Some("Inner"), Some(100)),
            ("link", Some("More"), Some(101)),
        ]
    );
    assert_eq!(preorder[4].1.unwrap().frame_id, "CHILD");
    assert_eq!(preorder[1].0.identity.dom_id.as_deref().unwrap(), "go");
    assert_eq!(preorder[4].0.identity.dom_id.as_deref().unwrap(), "more");
    assert_eq!(
        preorder[4].0.identity.retained_object.as_deref(),
        Some("cdp:T:CHILD:LC:101")
    );
    let sent = sent.lock().unwrap();
    assert_eq!(sent[0]["params"]["autoAttach"], true);
    assert_eq!(
        sent[2]["params"],
        json!({ "computedStyles": ["display", "visibility"] })
    );
    assert_eq!(sent[3]["params"], json!({}));
    assert_eq!(sent[4]["params"]["frameId"], "CHILD");
    assert_eq!(sent[8]["params"]["autoAttach"], false);
}

#[test]
fn an_unreadable_main_frame_fails_the_snapshot() {
    let transport = ScriptedTransport::new()
        .expect("Target.setAutoAttach", json!({}))
        .expect("Page.getFrameTree", main_frame_only())
        .expect("DOMSnapshot.captureSnapshot", json!({}))
        .expect_error(
            "Accessibility.getFullAXTree",
            -32000,
            "Accessibility is not enabled",
        );
    let mut connection = CdpConnection::new(release(transport, 1).boxed());

    let error = snapshot_page(&mut connection, &session(), deadline()).unwrap_err();

    assert_eq!(error.code, ErrorCode::ActionFailed);
}

#[test]
fn an_empty_main_frame_is_a_structured_error() {
    let transport = ScriptedTransport::new()
        .expect("Target.setAutoAttach", json!({}))
        .expect("Page.getFrameTree", main_frame_only())
        .expect("DOMSnapshot.captureSnapshot", json!({}))
        .expect("Accessibility.getFullAXTree", json!({ "nodes": [] }));
    let mut connection = CdpConnection::new(release(transport, 1).boxed());

    let error = snapshot_page(&mut connection, &session(), deadline()).unwrap_err();

    assert_eq!(error.details.as_ref().unwrap()["kind"], "cdp_tree_empty");
}

#[test]
fn an_out_of_process_iframe_is_read_through_its_own_target_and_grafted() {
    let oopif_frames = json!({ "frameTree": {
        "frame": { "id": "OOP", "parentId": "MAIN", "loaderId": "LO" },
        "childFrames": [{ "frame": { "id": "OOP-CHILD", "parentId": "OOP", "loaderId": "LOC" } }],
    } });
    let transport = page_reads(vec![attached("S2", "OOP")], main_frame_only(), json!({}))
        .expect("Target.setAutoAttach", json!({}))
        .expect("Page.getFrameTree", oopif_frames)
        .expect("DOMSnapshot.captureSnapshot", json!({}))
        .expect("Accessibility.getFullAXTree", child_tree())
        .expect("DOM.getFrameOwner", json!({ "backendNodeId": 30 }))
        .expect_error(
            "Accessibility.getFullAXTree",
            -32000,
            "Frame with the given id was not found.",
        )
        .expect("DOM.getFrameOwner", json!({}));
    let transport = release(transport, 2);
    let sent = transport.sent_log();
    let mut connection = CdpConnection::new(transport.boxed());

    let snapshot = snapshot_page(&mut connection, &session(), deadline()).unwrap();

    let preorder = snapshot.preorder();
    assert_eq!(preorder[2].0.role, "group");
    assert_eq!(preorder[3].0.identity.name.as_deref(), Some("Inner"));
    assert_eq!(
        preorder[4].0.identity.retained_object.as_deref(),
        Some("cdp:OOP:OOP:LO:101")
    );
    assert_eq!(snapshot.unreadable_frames, ["OOP-CHILD"]);
    let sent = sent.lock().unwrap();
    let oopif_calls: Vec<&str> = sent
        .iter()
        .filter(|request| request["sessionId"] == "S2")
        .map(|request| request["method"].as_str().unwrap())
        .collect();
    assert_eq!(
        oopif_calls,
        [
            "Target.setAutoAttach",
            "Page.getFrameTree",
            "DOMSnapshot.captureSnapshot",
            "Accessibility.getFullAXTree",
            "Accessibility.getFullAXTree",
            "DOM.getFrameOwner",
            "Target.setAutoAttach",
        ]
    );
    let owner = sent
        .iter()
        .find(|request| request["method"] == "DOM.getFrameOwner")
        .unwrap();
    assert_eq!(owner["sessionId"], "S");
    assert_eq!(owner["params"]["frameId"], "OOP");
    let stops: Vec<&serde_json::Value> = sent
        .iter()
        .filter(|request| request["params"]["autoAttach"] == false)
        .map(|request| &request["sessionId"])
        .collect();
    assert_eq!(stops, [&json!("S2"), &json!("S")]);
}

#[test]
fn an_iframe_target_that_vanishes_mid_read_is_listed_unreadable() {
    let transport = page_reads(vec![attached("S2", "OOP")], main_frame_only(), json!({}))
        .expect_error(
            "Target.setAutoAttach",
            -32001,
            "Session with given id not found.",
        )
        .expect_error(
            "Page.getFrameTree",
            -32001,
            "Session with given id not found.",
        )
        .expect("DOMSnapshot.captureSnapshot", json!({}))
        .expect("Accessibility.getFullAXTree", child_tree())
        .expect("DOM.getFrameOwner", json!({}));
    let mut connection = CdpConnection::new(release(transport, 2).boxed());

    let snapshot = snapshot_page(&mut connection, &session(), deadline()).unwrap();

    assert_eq!(snapshot.unreadable_frames, ["OOP"]);
    assert_eq!(snapshot.skipped_frames[0].url, "https://OOP.test/");
    assert_eq!(snapshot.preorder().len(), 3);
}

#[test]
fn a_frame_with_no_text_and_no_control_is_left_out_as_empty() {
    let blank = json!({ "nodes": [
        { "nodeId": "1", "role": { "type": "role", "value": "RootWebArea" },
          "name": { "type": "computedString", "value": "SafeFrame Container" }, "backendDOMNodeId": 100 },
    ] });
    let transport = page_reads(vec![attached("S2", "AD")], main_frame_only(), json!({}))
        .expect("Target.setAutoAttach", json!({}))
        .expect(
            "Page.getFrameTree",
            json!({ "frameTree": { "frame": { "id": "AD", "parentId": "MAIN", "loaderId": "LA" } } }),
        )
        .expect("DOMSnapshot.captureSnapshot", json!({}))
        .expect("Accessibility.getFullAXTree", blank)
        .expect("DOM.getFrameOwner", json!({ "backendNodeId": 30 }));
    let mut connection = CdpConnection::new(release(transport, 2).boxed());

    let snapshot = snapshot_page(&mut connection, &session(), deadline()).unwrap();

    assert!(snapshot.unreadable_frames.is_empty());
    assert_eq!(snapshot.skipped_frames.len(), 1);
    assert_eq!(snapshot.skipped_frames[0].frame_id, "AD");
    assert_eq!(snapshot.skipped_frames[0].reason, FrameSkipReason::Empty);
    assert_eq!(snapshot.preorder().len(), 3);
}
