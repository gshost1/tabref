use serde_json::json;

use super::parse_capture;
use crate::outline::node_kind::NodeKind;

fn capture() -> serde_json::Value {
    json!({
        "strings": ["#document", "HTML", "BODY", "A", "href", "/x?y=1", "#text", "Go", "block", "visible", "static",
                    "inline", "hidden", "SELECT", "OPTION", "One", "http://site.test/", "frame-1"],
        "documents": [{
            "documentURL": 16, "baseURL": 16, "frameId": 17,
            "nodes": {
                "parentIndex": [-1, 0, 1, 2, 3, 2, 5, 6],
                "nodeType": [9, 1, 1, 1, 3, 1, 1, 3],
                "nodeName": [0, 1, 2, 3, 6, 13, 14, 6],
                "nodeValue": [-1, -1, -1, -1, 7, -1, -1, 15],
                "backendNodeId": [1, 2, 3, 4, 5, 6, 7, 8],
                "attributes": [[], [], [], [4, 5], [], [], [], []],
                "optionSelected": { "index": [6] },
                "inputValue": { "index": [5], "value": [15] }
            },
            "layout": {
                "nodeIndex": [0, 1, 2, 3, 4, 5],
                "styles": [[], [8, 9, 10], [8, 9, 10], [11, 12, 10], [11, 9, 10], [11, 9, 10]],
                "bounds": [[0, 0, 800, 600], [0, 0, 800, 600], [8, 8, 784, 584], [8, 8, 20, 17], [8, 8, 20, 17], [30, 8, 60, 20]]
            }
        }]
    })
}

#[test]
fn a_capture_reads_into_a_tree_with_attributes_layout_and_form_state() {
    let trees = parse_capture(&capture(), "T1");
    assert_eq!(trees.len(), 1);
    let tree = &trees[0];
    assert_eq!(
        (
            tree.url.as_str(),
            tree.frame_id.as_str(),
            tree.target_id.as_str()
        ),
        ("http://site.test/", "frame-1", "T1")
    );
    assert_eq!(tree.nodes[2].children, vec![3, 5]);
    assert_eq!(tree.nodes[3].attr("HREF"), Some("/x?y=1"));
    assert_eq!(tree.nodes[4].kind, NodeKind::Text("Go".into()));
    assert!(!tree.nodes[3].is_shown(), "visibility hidden");
    assert!(tree.nodes[4].is_shown());
    assert!(tree.nodes[6].layout.is_none(), "options have no box");
    assert!(tree.nodes[6].form.option_selected);
    assert_eq!(tree.nodes[5].form.input_value.as_deref(), Some("One"));
    assert_eq!(tree.body(), 2);
    assert_eq!(tree.find_backend(8), Some(7));
}

#[test]
fn an_empty_reply_has_no_documents() {
    assert!(parse_capture(&json!({}), "T1").is_empty());
}
