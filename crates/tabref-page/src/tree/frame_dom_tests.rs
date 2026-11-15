use serde_json::json;
use tabref_core::Deadline;

use super::FrameDom;
use crate::{CdpConnection, PageSession};
use tabref_cdp::ScriptedTransport;

#[test]
fn a_dom_too_deep_for_a_nested_reply_is_read_from_one_flat_capture() {
    let depth = 400;
    let capture = json!({
        "strings": ["id", "leaf"],
        "documents": [{ "nodes": {
            "backendNodeId": (1..=depth).collect::<Vec<i64>>(),
            "attributes": (1..=depth)
                .map(|backend| if backend == depth { json!([0, 1]) } else { json!([]) })
                .collect::<Vec<_>>(),
        } }],
    });
    let transport = ScriptedTransport::new().expect("DOMSnapshot.captureSnapshot", capture);
    let sent = transport.sent_log();
    let mut connection = CdpConnection::new(transport.boxed());
    let session = PageSession {
        target_id: "T".into(),
        session_id: "S".into(),
    };

    let reply = connection
        .call(
            "DOMSnapshot.captureSnapshot",
            FrameDom::capture_params(),
            Some(&session.session_id),
            Deadline::after(1_000).unwrap(),
        )
        .unwrap();
    let facts = FrameDom::from_capture(reply).facts;

    assert_eq!(facts.ids.get(&depth).map(String::as_str), Some("leaf"));
    let sent = sent.lock().unwrap();
    assert_eq!(
        sent[0]["params"],
        json!({ "computedStyles": ["display", "visibility", "position"] })
    );
    assert!(connection.take_events().is_empty());
}
