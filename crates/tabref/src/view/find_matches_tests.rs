use tabref_core::AccessibilityNode;

use super::find_matches;

fn node(role: &str, name: Option<&str>, children: Vec<AccessibilityNode>) -> AccessibilityNode {
    let mut node = AccessibilityNode {
        role: role.into(),
        children,
        ..AccessibilityNode::default()
    };
    node.identity.name = name.map(str::to_owned);
    node
}

fn page() -> AccessibilityNode {
    let mut buy = node(
        "button",
        Some("Buy now"),
        vec![node("statictext", Some("Price 5 EUR"), Vec::new())],
    );
    buy.ref_id = Some("@s1:e9".into());
    node(
        "webarea",
        Some("Shop"),
        vec![
            node(
                "region",
                Some("Offers"),
                vec![node("statictext", Some("Price 3 EUR"), Vec::new())],
            ),
            buy,
        ],
    )
}

#[test]
fn text_matches_name_their_ref_or_their_context() {
    assert_eq!(
        find_matches(&page(), "price", None),
        vec![
            r#"text "Price 3 EUR" in region "Offers""#,
            r#"text "Price 5 EUR" in @s1:e9 button "Buy now""#,
        ]
    );
}

#[test]
fn a_role_filter_takes_aliases_and_keeps_only_that_role() {
    assert_eq!(
        find_matches(&page(), "buy", Some("Button")),
        vec![r#"@s1:e9 button "Buy now""#]
    );
    assert!(find_matches(&page(), "price", Some("textbox")).is_empty());
}

#[test]
fn a_container_whose_child_matches_is_not_repeated() {
    let matches = find_matches(&page(), "offers", None);
    assert_eq!(matches, vec![r#"region "Offers""#]);
    assert_eq!(
        find_matches(&page(), "shop", None),
        vec![r#"webarea "Shop""#]
    );
}
