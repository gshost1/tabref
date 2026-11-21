#![allow(dead_code, clippy::unwrap_used, clippy::expect_used, clippy::panic)]

#[path = "../src/view/action_changes.rs"]
mod action_changes;

use std::collections::HashSet;
use tabref_core::AccessibilityNode;
use tabref_page::{DomNode, DomTree, NodeKind, NodeLayout, TextChange};

fn dom(tag: &str, id: i64, parent: Option<usize>, children: Vec<usize>) -> DomNode {
    DomNode {
        parent,
        children,
        kind: NodeKind::Element(tag.into()),
        backend_id: id,
        attrs: vec![],
        form: Default::default(),
        layout: Some(NodeLayout {
            display: "block".into(),
            position: "static".into(),
            visible: true,
            bounds: [0., 0., 100., 20.],
        }),
    }
}

fn text(value: &str, id: i64, parent: usize) -> DomNode {
    let mut node = dom("", id, Some(parent), vec![]);
    node.kind = NodeKind::Text(value.into());
    node
}

fn control(role: &str, name: &str, id: i64) -> AccessibilityNode {
    let mut node = AccessibilityNode {
        role: role.into(),
        ref_id: Some(format!("e{id}")),
        ..Default::default()
    };
    node.identity.name = Some(name.into());
    node.identity.retained_object = Some(format!("cdp:target:frame:loader:{id}"));
    node
}

fn document(nodes: Vec<DomNode>) -> [DomTree; 1] {
    [DomTree {
        target_id: "target".into(),
        frame_id: "frame".into(),
        nodes,
        ..Default::default()
    }]
}

fn shown(controls: &[&AccessibilityNode]) -> HashSet<String> {
    controls
        .iter()
        .take(8)
        .filter_map(|node| node.identity.retained_object.clone())
        .collect()
}

#[test]
fn native_options_are_filtered_but_aria_owned_options_are_not() {
    let docs = document(vec![
        dom("BODY", 1, None, vec![1, 4]),
        dom("SELECT", 2, Some(0), vec![2]),
        dom("OPTGROUP", 3, Some(1), vec![3]),
        dom("OPTION", 4, Some(2), vec![]),
        dom("UL", 5, Some(0), vec![5]),
        dom("LI", 6, Some(4), vec![]),
    ]);
    let tree = AccessibilityNode {
        children: vec![
            control("option", "Open", 4),
            control("option", "Announcements", 6),
        ],
        ..Default::default()
    };
    let report = action_changes::prepare(&tree, &HashSet::new(), &[], |_| Some(&docs), shown);
    assert_eq!(report.controls.len(), 1);
    assert_eq!(
        report.controls[0].identity.name.as_deref(),
        Some("Announcements")
    );
}

#[test]
fn only_complete_owned_added_lines_are_deduplicated() {
    let docs = document(vec![
        dom("BODY", 1, None, vec![1]),
        dom("BUTTON", 2, Some(0), vec![2]),
        text("  Save   changes ", 3, 1),
    ]);
    let tree = control("button", "Save changes", 2);
    let changes = vec![
        TextChange::Added("Save changes".into()),
        TextChange::Added("Save changes failed".into()),
        TextChange::Added("Validation failed".into()),
        TextChange::Changed {
            before: "Value old".into(),
            after: "Value new".into(),
        },
    ];
    let report = action_changes::prepare(&tree, &HashSet::new(), &changes, |_| Some(&docs), shown);
    assert_eq!(report.changes, changes[1..]);
    assert_eq!(report.controls.len(), 1);
}

#[test]
fn an_error_matching_a_control_name_survives_and_unknown_dom_is_conservative() {
    let docs = document(vec![
        dom("BODY", 1, None, vec![1, 3]),
        dom("BUTTON", 2, Some(0), vec![2]),
        text("Retry", 3, 1),
        dom("P", 4, Some(0), vec![4]),
        text("Retry", 5, 3),
    ]);
    let tree = control("button", "Retry", 2);
    let changes = vec![
        TextChange::Added("Retry".into()),
        TextChange::Added("Retry".into()),
    ];
    let report = action_changes::prepare(&tree, &HashSet::new(), &changes, |_| Some(&docs), shown);
    assert_eq!(report.changes, changes);
    assert_eq!(
        action_changes::prepare(&tree, &HashSet::new(), &changes, |_| None, shown).changes,
        changes
    );
    assert!(
        action_changes::prepare(&tree, &HashSet::new(), &[], |_| None, shown)
            .changes
            .is_empty()
    );
}

#[test]
fn known_controls_changed_values_and_removed_lines_remain() {
    let docs = document(vec![
        dom("BODY", 1, None, vec![1]),
        dom("BUTTON", 2, Some(0), vec![2]),
        text("Continue", 3, 1),
    ]);
    let tree = control("button", "Continue", 2);
    let known = HashSet::from([tree.identity.retained_object.clone().unwrap()]);
    let changes = vec![
        TextChange::Added("Continue".into()),
        TextChange::Changed {
            before: "10".into(),
            after: "Continue".into(),
        },
        TextChange::Removed("Continue".into()),
    ];
    let report = action_changes::prepare(&tree, &known, &changes, |_| Some(&docs), shown);
    assert!(report.controls.is_empty());
    assert_eq!(report.changes, changes);
    let report = action_changes::prepare(&tree, &HashSet::new(), &changes, |_| Some(&docs), shown);
    assert_eq!(report.controls.len(), 1);
    assert_eq!(report.changes, changes[1..]);
}

#[test]
fn inaccessible_labels_invalid_tokens_and_truncated_names_keep_added_text() {
    for name in ["External label".to_owned(), "Long name ".repeat(10)] {
        let docs = document(vec![
            dom("BODY", 1, None, vec![1, 2]),
            dom("BUTTON", 2, Some(0), vec![]),
            text(&name, 3, 0),
        ]);
        let mut tree = control("button", &name, 2);
        let changes = vec![TextChange::Added(name)];
        assert_eq!(
            action_changes::prepare(&tree, &HashSet::new(), &changes, |_| Some(&docs), shown)
                .changes,
            changes
        );
        tree.identity.retained_object = Some("invalid".into());
        let report =
            action_changes::prepare(&tree, &HashSet::new(), &changes, |_| Some(&docs), shown);
        assert_eq!(report.controls.len(), 1);
        assert_eq!(report.changes, changes);
    }
}

#[test]
fn capped_controls_and_controls_only_in_row_summaries_keep_their_added_names() {
    let mut nodes = vec![dom("BODY", 1, None, (0..9).map(|n| n * 2 + 1).collect())];
    let mut children = Vec::new();
    let mut changes = Vec::new();
    for n in 0..9 {
        let index = nodes.len();
        let id = index as i64 + 1;
        let name = format!("Option {n}");
        nodes.push(dom("LI", id, Some(0), vec![index + 1]));
        nodes.push(text(&name, id + 1, index));
        children.push(control("option", &name, id));
        changes.push(TextChange::Added(name));
    }
    let docs = document(nodes);
    let tree = AccessibilityNode {
        children,
        ..Default::default()
    };
    let report = action_changes::prepare(&tree, &HashSet::new(), &changes, |_| Some(&docs), shown);
    assert_eq!(report.changes, [TextChange::Added("Option 8".into())]);
    let report = action_changes::prepare(
        &tree,
        &HashSet::new(),
        &changes,
        |_| Some(&docs),
        |_| HashSet::new(),
    );
    assert_eq!(report.changes, changes);
}

#[test]
fn passive_table_parts_do_not_hide_their_inner_controls() {
    for role in ["cell", "gridcell", "row", "rowheader", "columnheader"] {
        let mut nodes = vec![
            dom("BODY", 1, None, vec![1]),
            dom("TD", 2, Some(0), vec![2]),
            dom("BUTTON", 3, Some(1), vec![3]),
            text("Open", 4, 2),
        ];
        nodes[1].attrs = vec![
            ("contenteditable".into(), "false".into()),
            ("tabindex".into(), "invalid".into()),
            ("aria-sort".into(), "ascending".into()),
        ];
        let docs = document(nodes);
        let mut tree = control(tabref_page::map_cdp_role(role), "Tracking", 2);
        tree.presentation.available_actions = vec!["Click".into()];
        tree.children.push(control("button", "Open", 3));
        let changes = [TextChange::Added("Tracking".into())];
        let report =
            action_changes::prepare(&tree, &HashSet::new(), &changes, |_| Some(&docs), shown);
        assert_eq!(report.controls.len(), 1, "{role}");
        assert_eq!(report.controls[0].role, "button");
        assert_eq!(report.changes, changes);
    }
}

#[test]
fn table_parts_with_their_own_interactions_are_preserved() {
    for (attr, value) in [
        ("tabindex", "0"),
        ("tabindex", "-1"),
        ("contenteditable", "true"),
        ("contenteditable", ""),
        ("contenteditable", "plaintext-only"),
        ("onclick", "save()"),
    ] {
        let mut nodes = vec![dom("BODY", 1, None, vec![1]), dom("TD", 2, Some(0), vec![])];
        nodes[1].attrs.push((attr.into(), value.into()));
        let docs = document(nodes);
        let tree = control("cell", "Editable", 2);
        assert_eq!(
            action_changes::prepare(&tree, &HashSet::new(), &[], |_| Some(&docs), shown)
                .controls
                .len(),
            1,
            "{attr}"
        );
    }
    let docs = document(vec![
        dom("BODY", 1, None, vec![1]),
        dom("TD", 2, Some(0), vec![]),
    ]);
    for action in ["SetFocus", "SetValue", "TypeText"] {
        let mut tree = control("cell", "Interactive", 2);
        tree.presentation.available_actions.push(action.into());
        assert_eq!(
            action_changes::prepare(&tree, &HashSet::new(), &[], |_| Some(&docs), shown)
                .controls
                .len(),
            1,
            "{action}"
        );
    }
    let tree = control("cell", "Unknown", 2);
    assert_eq!(
        action_changes::prepare(&tree, &HashSet::new(), &[], |_| None, shown)
            .controls
            .len(),
        1
    );
}
