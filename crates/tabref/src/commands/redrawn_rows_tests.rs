use tabref_page::{DomNode, DomTree, NodeKind, TextChange};

use super::{NewControl, redrawn};

#[test]
fn without_a_reorder_every_new_control_is_listed() {
    let found = [NewControl {
        line: "[e1 button Undo]".into(),
        name: Some("Undo"),
        token: "cdp:target:frame:loader:1",
    }];
    let (note, outside) = redrawn(&[TextChange::Added("Saved".into())], &found, |_| None);
    assert_eq!(note, None);
    assert_eq!(outside, ["[e1 button Undo]"]);
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
    let found = [
        NewControl {
            line: "[e1 link Close]".into(),
            name: Some("Close"),
            token: "cdp:target:frame:loader:2",
        },
        NewControl {
            line: "[e2 button]".into(),
            name: None,
            token: "cdp:target:frame:loader:3",
        },
        NewControl {
            line: "[e3 button Close]".into(),
            name: Some("Close"),
            token: "cdp:target:frame:loader:6",
        },
    ];
    let sorted = TextChange::Reordered {
        above: None,
        run: vec!["P-2 44 kg".into(), "Close 12 kg".into()],
        table: true,
    };
    let (note, outside) = redrawn(&[sorted], &found, |_| Some(&documents));
    assert_eq!(
        note.as_deref(),
        Some("rows drawn again: 1 new refs, earlier refs in them are stale: tabref snap")
    );
    assert_eq!(outside, ["[e2 button]", "[e3 button Close]"]);
}
