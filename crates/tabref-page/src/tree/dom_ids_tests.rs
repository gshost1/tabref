use std::collections::{HashMap, HashSet};

use serde_json::json;

use super::{collect_clickable, collect_dom_ids};

#[test]
fn ids_come_from_the_flat_capture_of_every_document() {
    let capture = json!({
        "strings": ["id", "go", "class", "primary", "more", "  "],
        "documents": [
            { "nodes": {
                "backendNodeId": [1, 20, 30],
                "attributes": [[], [0, 1, 2, 3], [2, 1]],
            } },
            { "nodes": {
                "backendNodeId": [100, 101, 102],
                "attributes": [[], [0, 4], [0, 5]],
            } },
        ],
    });

    let ids = collect_dom_ids(&capture);

    assert_eq!(
        ids,
        HashMap::from([(20, "go".to_owned()), (101, "more".to_owned())])
    );
}

#[test]
fn clickable_nodes_come_from_each_documents_is_clickable_indexes() {
    let capture = json!({
        "strings": [],
        "documents": [
            { "nodes": {
                "backendNodeId": [1, 20, 30],
                "isClickable": { "index": [1, 7] },
            } },
            { "nodes": { "backendNodeId": [100, 101] } },
            { "nodes": {
                "backendNodeId": [200, 201],
                "isClickable": { "index": [0] },
            } },
        ],
    });

    assert_eq!(collect_clickable(&capture), HashSet::from([20, 200]));
}

#[test]
fn a_documents_html_and_body_are_never_clickable_targets() {
    let capture = json!({
        "strings": ["#document", "HTML", "BODY", "DIV", "body"],
        "documents": [
            { "nodes": {
                "backendNodeId": [1, 2, 3, 4],
                "nodeName": [0, 1, 2, 3],
                "isClickable": { "index": [1, 2, 3] },
            } },
            { "nodes": {
                "backendNodeId": [10, 11],
                "nodeName": [4, 3],
                "isClickable": { "index": [0, 1] },
            } },
        ],
    });

    assert_eq!(collect_clickable(&capture), HashSet::from([4, 11]));
}
