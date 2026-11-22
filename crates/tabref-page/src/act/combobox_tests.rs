use serde_json::json;
use tabref_cdp::ScriptedTransport;

use super::{PopupClosure, PopupSearch, close};
use crate::CdpConnection;
use crate::act::act_fixture::{deadline, proven, resolved, session, token};

fn open_popup(transport: ScriptedTransport) -> ScriptedTransport {
    proven(transport)
        .expect(
            "DOMSnapshot.captureSnapshot",
            json!({
                "strings": ["BUTTON", "UL", "role", "listbox", "block", "visible", "static", "F"],
                "documents": [{
                    "frameId": 7,
                    "nodes": {
                        "parentIndex": [-1, -1],
                        "nodeType": [1, 1],
                        "nodeName": [0, 1],
                        "backendNodeId": [1, 2],
                        "attributes": [[], [2, 3]]
                    },
                    "layout": {
                        "nodeIndex": [0, 1],
                        "styles": [[4, 5, 6], [4, 5, 6]],
                        "bounds": [[0, 0, 100, 20], [0, 20, 100, 80]]
                    }
                }]
            }),
        )
        .expect(
            "Accessibility.queryAXTree",
            json!({ "nodes": [{
                "nodeId": "3", "ignored": false, "backendDOMNodeId": 3,
                "role": { "type": "role", "value": "option" }
            }] }),
        )
}

#[test]
fn a_timed_out_close_probe_counts_as_still_open() {
    let session = session();
    let combo = token(1);
    let search = PopupSearch {
        session: &session,
        combo: &combo,
        baseline: Default::default(),
        locator: Default::default(),
    };
    let transport = resolved(open_popup(ScriptedTransport::new()))
        .expect("Runtime.callFunctionOn", json!({}))
        .expect("Runtime.releaseObject", json!({}));
    let transport = open_popup(transport)
        .expect("Input.dispatchKeyEvent", json!({}))
        .expect("Input.dispatchKeyEvent", json!({}));
    let transport = proven(transport).expect_silence("DOMSnapshot.captureSnapshot");
    let mut connection = CdpConnection::new(transport.boxed());
    assert_eq!(
        close(&mut connection, &search, Some(2), deadline()),
        Ok(PopupClosure::StillOpen(1))
    );
}
