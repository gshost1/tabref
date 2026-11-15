//! Scripted CDP replies the snapshot tests share. The reader sends, per
//! target and in this order: `Target.setAutoAttach`, `Page.getFrameTree`,
//! `DOMSnapshot.captureSnapshot`, the root `Accessibility.getFullAXTree`;
//! then, as replies come in, each child target's four and its
//! `DOM.getFrameOwner` on the parent, and each in-process frame's tree and
//! owner. Last, auto-attach is turned off, newest target first.

use serde_json::{Value, json};

use crate::PageSession;
use tabref_cdp::ScriptedTransport;

pub(crate) fn session() -> PageSession {
    PageSession {
        target_id: "T".into(),
        session_id: "S".into(),
    }
}

pub(crate) fn role(name: &str) -> Value {
    json!({ "type": "role", "value": name })
}

pub(crate) fn text(value: &str) -> Value {
    json!({ "type": "computedString", "value": value })
}

pub(crate) fn frame_tree() -> Value {
    json!({ "frameTree": {
        "frame": { "id": "MAIN", "loaderId": "LM" },
        "childFrames": [
            { "frame": { "id": "CHILD", "parentId": "MAIN", "loaderId": "LC" } },
            { "frame": { "id": "REMOTE", "parentId": "MAIN", "loaderId": "LR" } },
        ]
    } })
}

pub(crate) fn main_frame_only() -> Value {
    json!({ "frameTree": { "frame": { "id": "MAIN", "loaderId": "LM" } } })
}

pub(crate) fn main_tree() -> Value {
    json!({ "nodes": [
        { "nodeId": "1", "role": role("RootWebArea"), "name": text("Top"), "childIds": ["2", "3"], "backendDOMNodeId": 1 },
        { "nodeId": "2", "role": role("button"), "name": text("Go"), "backendDOMNodeId": 20 },
        { "nodeId": "3", "role": role("Iframe"), "backendDOMNodeId": 30 },
    ] })
}

pub(crate) fn child_tree() -> Value {
    json!({ "nodes": [
        { "nodeId": "1", "role": role("RootWebArea"), "name": text("Inner"), "childIds": ["2"], "backendDOMNodeId": 100 },
        { "nodeId": "2", "role": role("link"), "name": text("More"), "backendDOMNodeId": 101 },
    ] })
}

pub(crate) fn capture() -> Value {
    json!({
        "strings": ["id", "go", "class", "primary", "src", "x.html", "more"],
        "documents": [
            { "nodes": { "backendNodeId": [1, 20, 30], "attributes": [[], [0, 1, 2, 3], [4, 5]] } },
            { "nodes": { "backendNodeId": [99, 101], "attributes": [[], [0, 6]] } },
        ],
    })
}

/// A capture that lays the iframe element (backend 30) out at `rect`.
pub(crate) fn capture_with_owner(rect: [f64; 4]) -> Value {
    json!({
        "strings": ["HTML", "IFRAME", "block", "visible"],
        "documents": [{
            "nodes": { "nodeName": [0, 1], "backendNodeId": [1, 30] },
            "layout": {
                "nodeIndex": [0, 1],
                "bounds": [[0, 0, 800, 600], rect],
                "styles": [[2, 3], [2, 3]],
            },
        }],
    })
}

pub(crate) fn attached(session_id: &str, target_id: &str) -> Value {
    json!({
        "method": "Target.attachedToTarget",
        "sessionId": "S",
        "params": {
            "sessionId": session_id,
            "targetInfo": { "targetId": target_id, "type": "iframe", "parentFrameId": "MAIN",
                "url": format!("https://{target_id}.test/") },
            "waitingForDebugger": false,
        },
    })
}

/// The page's own four requests, with `children` reported by its
/// auto-attach and `capture` as its DOM capture.
pub(crate) fn page_reads(children: Vec<Value>, frames: Value, capture: Value) -> ScriptedTransport {
    ScriptedTransport::new()
        .expect_with_events("Target.setAutoAttach", children, json!({ "result": {} }))
        .expect("Page.getFrameTree", frames)
        .expect("DOMSnapshot.captureSnapshot", capture)
        .expect("Accessibility.getFullAXTree", main_tree())
}

/// A child target's four requests and its owner, all answered, with
/// `frame_id` as its only frame.
pub(crate) fn child_reads(transport: ScriptedTransport, frame_id: &str) -> ScriptedTransport {
    let frames = json!({ "frameTree": {
        "frame": { "id": frame_id, "parentId": "MAIN", "loaderId": "LO" },
    } });
    transport
        .expect("Target.setAutoAttach", json!({}))
        .expect("Page.getFrameTree", frames)
        .expect("DOMSnapshot.captureSnapshot", json!({}))
        .expect("Accessibility.getFullAXTree", child_tree())
        .expect("DOM.getFrameOwner", json!({ "backendNodeId": 30 }))
}

/// A child target whose renderer never answers; its owner does.
pub(crate) fn stalled_child(transport: ScriptedTransport) -> ScriptedTransport {
    transport
        .expect_silence("Target.setAutoAttach")
        .expect_silence("Page.getFrameTree")
        .expect_silence("DOMSnapshot.captureSnapshot")
        .expect_silence("Accessibility.getFullAXTree")
        .expect("DOM.getFrameOwner", json!({ "backendNodeId": 30 }))
}

/// `count` auto-attach switch-offs.
pub(crate) fn release(mut transport: ScriptedTransport, count: usize) -> ScriptedTransport {
    for _ in 0..count {
        transport = transport.expect("Target.setAutoAttach", json!({}));
    }
    transport
}
