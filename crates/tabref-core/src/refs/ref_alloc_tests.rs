use super::allocate_refs;
use crate::{AccessibilityNode, NameSource, RefMap, capability};

fn node(role: &str, name: Option<&str>, token: Option<&str>) -> AccessibilityNode {
    let mut node = AccessibilityNode {
        role: role.into(),
        ..AccessibilityNode::default()
    };
    node.identity.name = name.map(str::to_owned);
    node.identity.retained_object = token.map(str::to_owned);
    node
}

#[test]
fn refs_follow_preorder_and_skip_tokenless_and_inert_nodes() {
    let mut first = node("group", None, Some("cdp:T:F:L:1"));
    first.children = vec![node("link", Some("Home"), Some("cdp:T:F:L:2"))];
    let mut root = node("webarea", Some("Page"), Some("cdp:T:F:L:9"));
    root.children = vec![
        first,
        node("button", Some("Go"), None),
        node("button", Some("Save"), Some("cdp:T:F:L:3")),
    ];
    let mut map = RefMap::new("T", "https://example.test/");

    let allocated = allocate_refs(&mut root, "s1234567", &mut map);

    assert_eq!(allocated, 2);
    assert_eq!(root.ref_id, None);
    assert_eq!(
        root.children[0].children[0].ref_id.as_deref(),
        Some("@s1234567:e1")
    );
    assert_eq!(root.children[1].ref_id, None);
    assert_eq!(root.children[2].ref_id.as_deref(), Some("@s1234567:e2"));
    assert_eq!(map.get(1).unwrap().token, "cdp:T:F:L:2");
    assert_eq!(map.get(2).unwrap().name.as_deref(), Some("Save"));
    assert!(map.get(3).is_none());
    assert!(map.get(0).is_none());
}

#[test]
fn a_promoted_node_gets_a_ref_without_recording_its_derived_name() {
    let mut row = node("group", None, Some("cdp:T:F:L:5"));
    row.presentation.available_actions = vec![capability::CLICK.into()];
    row.presentation.name_source = Some(NameSource::ContainedText);
    row.presentation.derived_name = Some("Open the report".into());
    let mut map = RefMap::default();

    allocate_refs(&mut row, "s1234567", &mut map);

    assert_eq!(row.ref_id.as_deref(), Some("@s1234567:e1"));
    assert_eq!(map.get(1).unwrap().name, None);
    assert_eq!(map.get(1).unwrap().role, "group");
}
