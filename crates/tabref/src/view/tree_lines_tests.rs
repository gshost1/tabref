use tabref_core::AccessibilityNode;

use super::tree_lines;
use crate::view::view_mode::ViewMode;

fn node(role: &str, name: Option<&str>, children: Vec<AccessibilityNode>) -> AccessibilityNode {
    let mut node = AccessibilityNode {
        role: role.into(),
        children,
        ..AccessibilityNode::default()
    };
    node.identity.name = name.map(str::to_owned);
    node
}

fn with_ref(mut node: AccessibilityNode, ref_id: &str) -> AccessibilityNode {
    node.ref_id = Some(ref_id.into());
    node
}

fn page() -> AccessibilityNode {
    node(
        "webarea",
        Some("Shop"),
        vec![
            node("heading", Some("Basket"), Vec::new()),
            node(
                "group",
                None,
                vec![node(
                    "navigation",
                    Some("Main"),
                    vec![with_ref(node("link", Some("Home"), Vec::new()), "@s1:e1")],
                )],
            ),
            node("navigation", Some("Empty"), Vec::new()),
            node("paragraph", Some("Total: 4 items"), Vec::new()),
            node(
                "form",
                None,
                vec![with_ref(node("button", Some("Pay"), Vec::new()), "@s1:e2")],
            ),
        ],
    )
}

#[test]
fn the_default_view_keeps_controls_headings_and_named_regions_that_hold_them() {
    assert_eq!(
        tree_lines(&page(), ViewMode::Interactive),
        vec![
            r#"heading "Basket""#,
            r#"navigation "Main""#,
            r#"  @s1:e1 link "Home""#,
            r#"@s1:e2 button "Pay""#,
        ]
    );
}

#[test]
fn the_full_view_adds_text() {
    let lines = tree_lines(&page(), ViewMode::All);
    assert!(lines.contains(&r#"paragraph "Total: 4 items""#.to_owned()));
    assert!(!lines.iter().any(|line| line.contains("Empty")));
}

#[test]
fn an_alert_shows_even_without_controls() {
    let root = node(
        "webarea",
        None,
        vec![node("alert", Some("Card declined"), Vec::new())],
    );
    assert_eq!(
        tree_lines(&root, ViewMode::Interactive),
        vec![r#"alert "Card declined""#]
    );
}
