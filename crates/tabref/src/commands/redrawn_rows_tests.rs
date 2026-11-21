use tabref_core::AccessibilityNode;
use tabref_page::{DomNode, DomTree, NodeKind, TextChange};

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
    let (_, shown, summary) =
        preview_controls(&[TextChange::Added("Saved".into())], &[&undo], |_| None);
    assert!(summary.is_empty());
    assert_eq!(shown, [0]);
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
    let (_, outside, summary) =
        preview_controls(&[sorted], &[&link, &unnamed, &close], |_| Some(&documents));
    assert_eq!(
        summary,
        ["rows drawn again: 1 new refs, earlier refs in them are stale: tabref snap"]
    );
    assert_eq!(outside, [1, 2]);
}
