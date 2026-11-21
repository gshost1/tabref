#![allow(dead_code, clippy::unwrap_used, clippy::expect_used, clippy::panic)]

#[path = "../src/view/action_changes.rs"]
mod action_changes;

use std::collections::HashSet;

use serde_json::{Value, json};
use tabref_core::AccessibilityNode;
use tabref_page::{DomNode, DomTree, NodeKind};

fn found(role: &str, capture: Option<&Value>, active: Option<&str>) -> Vec<String> {
    let mut nodes: Vec<_> = [("BODY", 1), ("TD", 2), ("TD", 3), ("DIV", 4)]
        .into_iter()
        .map(|(tag, backend_id)| DomNode {
            parent: (backend_id != 1).then_some(0),
            children: vec![],
            kind: NodeKind::Element(tag.into()),
            backend_id,
            attrs: vec![],
            form: Default::default(),
            layout: None,
        })
        .collect();
    nodes[0].children = vec![1, 2, 3];
    nodes[1].attrs.push(("id".into(), "active".into()));
    if let Some(id) = active {
        nodes[3]
            .attrs
            .push(("aria-activedescendant".into(), id.into()));
    }
    let docs = [DomTree {
        target_id: "target".into(),
        frame_id: "frame".into(),
        nodes,
        ..Default::default()
    }];
    let tree = AccessibilityNode {
        children: [(role, 2, "Interactive"), ("cell", 3, "Passive")]
            .into_iter()
            .map(|(role, id, name)| {
                let mut node = AccessibilityNode {
                    role: role.into(),
                    ref_id: Some(format!("e{id}")),
                    ..Default::default()
                };
                node.identity.name = Some(name.into());
                node.identity.retained_object = Some(format!("cdp:target:frame:loader:{id}"));
                node.presentation.available_actions.push("Click".into());
                node
            })
            .collect(),
        ..Default::default()
    };
    let report = action_changes::prepare(
        &tree,
        &HashSet::new(),
        &[],
        |target| {
            assert_eq!(target, "target");
            capture.map(|reply| (docs.as_slice(), reply))
        },
        |_| HashSet::new(),
    );
    report
        .controls
        .iter()
        .map(|node| node.identity.name.clone().unwrap())
        .collect()
}

#[test]
fn clickable_snapshot_nodes_survive_without_ax_focusability() {
    let capture = json!({"strings":["BODY","TD"],"documents":[{"nodes":{
        "backendNodeId":[1,2,3],"nodeName":[0,1,1],"isClickable":{"index":[1]}
    }}]});
    for role in ["cell", "row"] {
        assert_eq!(found(role, Some(&capture), None), ["Interactive"]);
    }
}

#[test]
fn empty_clickable_lists_omit_passive_cells_and_active_ids_match_exactly() {
    let capture = json!({"documents":[]});
    assert!(found("cell", Some(&capture), None).is_empty());
    assert!(found("cell", Some(&capture), Some("other")).is_empty());
    assert_eq!(
        found("cell", Some(&capture), Some("active")),
        ["Interactive"]
    );
}

#[test]
fn unavailable_capture_keeps_potential_controls() {
    assert_eq!(found("cell", None, None), ["Interactive", "Passive"]);
}
