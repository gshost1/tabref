use tabref_core::AccessibilityNode;
use tabref_page::{DomNode, DomTree, NodeKind, NodeLayout, TextChange};

use super::preview_controls;

fn control(role: &str, name: Option<&str>, backend_id: i64) -> AccessibilityNode {
    let mut node = AccessibilityNode {
        role: role.to_owned(),
        ..AccessibilityNode::default()
    };
    node.identity.name = name.map(str::to_owned);
    node.identity.retained_object = Some(format!("cdp:target:frame:loader:{backend_id}"));
    node
}

#[test]
fn without_a_reorder_every_new_control_is_listed() {
    let undo = control("button", Some("Undo"), 1);
    let preview = preview_controls(&[TextChange::Added("Saved".into())], &[&undo], |_| None);
    assert!(preview.summary.is_empty());
    assert_eq!(preview.shown, [0]);
}

#[test]
fn only_named_controls_inside_reordered_rows_are_summarized() {
    let element = |tag: &str, backend_id, parent, children| DomNode {
        parent,
        children,
        kind: NodeKind::Element(tag.into()),
        backend_id,
        attrs: vec![],
        layout: None,
        form: Default::default(),
    };
    let mut text = element("", 4, Some(0), vec![]);
    text.kind = NodeKind::Text("Close 12 kg".into());
    let documents = [DomTree {
        frame_id: "frame".into(),
        nodes: vec![
            element("TR", 1, None, vec![1, 2, 3]),
            element("A", 2, Some(0), vec![]),
            element("BUTTON", 3, Some(0), vec![]),
            text,
            element("DIALOG", 5, None, vec![5]),
            element("BUTTON", 6, Some(4), vec![]),
        ],
        ..DomTree::default()
    }];
    let link = control("link", Some("Close"), 2);
    let unnamed = control("button", None, 3);
    let close = control("button", Some("Close"), 6);
    let sorted = TextChange::Reordered {
        above: None,
        run: vec!["P-2 44 kg".into(), "Close 12 kg".into()],
        table: true,
    };
    let preview = preview_controls(&[sorted], &[&link, &unnamed, &close], |_| Some(&documents));
    assert_eq!(
        preview.summary,
        ["rows drawn again: 1 new refs, earlier refs in them are stale: tabref snap"]
    );
    assert_eq!(preview.shown, [1, 2]);
}

#[test]
fn a_flat_container_uses_the_preceding_heading_for_each_control() {
    let node = |kind, index: u32, parent, children| DomNode {
        parent,
        children,
        kind,
        backend_id: i64::from(index),
        attrs: vec![],
        layout: Some(NodeLayout {
            display: "block".into(),
            position: "static".into(),
            visible: true,
            bounds: [0.0, 0.0, 20.0, 20.0],
        }),
        form: Default::default(),
    };
    let tag = |name: &str| NodeKind::Element(name.into());
    let documents = [DomTree {
        frame_id: "frame".into(),
        nodes: vec![
            node(tag("DIV"), 0, None, vec![1, 3, 4, 6]),
            node(tag("H3"), 1, Some(0), vec![2]),
            node(NodeKind::Text("Card A".into()), 2, Some(1), vec![]),
            node(tag("BUTTON"), 3, Some(0), vec![]),
            node(tag("H3"), 4, Some(0), vec![5]),
            node(NodeKind::Text("Card B".into()), 5, Some(4), vec![]),
            node(tag("BUTTON"), 6, Some(0), vec![]),
        ],
        ..DomTree::default()
    }];
    let a = control("button", Some("Actions"), 3);
    let b = control("button", Some("Actions"), 6);
    let preview = preview_controls(&[], &[&a, &b], |_| Some(&documents));
    assert_eq!(
        preview.labels,
        [Some("Card A".into()), Some("Card B".into())]
    );
}
