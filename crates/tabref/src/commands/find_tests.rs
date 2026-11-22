use tabref_core::AccessibilityNode;
use tabref_page::{OutlineInput, RefLabel};

use super::merged_matches;

mod dom_node {
    pub use tabref_page::DomNode;
}
mod dom_tree {
    pub use tabref_page::DomTree;
}
mod form_state {
    pub use tabref_page::FormState;
}
mod node_kind {
    pub use tabref_page::NodeKind;
}
mod node_layout {
    pub use tabref_page::NodeLayout;
}

#[allow(dead_code)]
#[path = "../../../tabref-page/src/outline/test_dom.rs"]
mod test_dom;

fn ax(name: &str, reference: Option<&str>) -> AccessibilityNode {
    let mut node = AccessibilityNode {
        role: "image".into(),
        ref_id: reference.map(str::to_owned),
        ..AccessibilityNode::default()
    };
    node.identity.name = Some(name.into());
    node
}

#[test]
fn ax_only_image_is_returned_alongside_a_deduplicated_dom_caption() {
    let mut dom = test_dom::TestDom::new();
    let caption = dom.element(2, "p", &[], Some("block"));
    let text = "Electricity use (kWh) by month";
    dom.text(caption, text, true);
    let input = OutlineInput {
        trees: vec![dom.tree],
        ..OutlineInput::default()
    };
    let tree = AccessibilityNode {
        children: vec![ax(text, None), ax("January 2024: 42,507 kWh", None)],
        ..AccessibilityNode::default()
    };
    let (hits, total, _) = merged_matches(&input, &tree, "kWh", 20);
    assert_eq!(total, 2);
    assert_eq!(hits[0].line, text);
    assert_eq!(hits[1].line, "January 2024: 42,507 kWh");
}

#[test]
fn dom_and_ax_matches_are_merged_without_repeating_dom_hits() {
    for owner in ["rect", "svg", "a"] {
        let mut dom = test_dom::TestDom::new();
        let caption = dom.element(2, "p", &[], Some("block"));
        let text = "Electricity use (kWh) by month";
        dom.text(caption, text, true);
        let link = dom.element(2, "a", &[("href", "/chart")], Some("inline"));
        let svg = dom.element(link, "svg", &[], Some("inline"));
        let bar = dom.element(
            svg,
            "rect",
            &[("role", "img"), ("aria-label", "March 2024: 45,371 kWh")],
            Some("inline"),
        );
        let title = dom.element(bar, "title", &[], None);
        dom.text(title, "March 2024 kWh are estimates", false);
        let index = match owner {
            "rect" => bar,
            "svg" => svg,
            _ => link,
        };
        let mut input = OutlineInput {
            trees: vec![dom.tree],
            ..OutlineInput::default()
        };
        input.refs.insert(
            ("T1".into(), i64::try_from(index).unwrap() + 1),
            RefLabel {
                ref_text: "@s1:e1".into(),
                ..RefLabel::default()
            },
        );
        let mut referenced = ax("March 2024: 45,371 kWh", None);
        referenced.children = vec![ax("March 2024 kWh are estimates", None)];
        let mut ancestor = AccessibilityNode {
            ref_id: Some("@s1:e1".into()),
            children: vec![referenced],
            ..AccessibilityNode::default()
        };
        ancestor.children.push(ax("March 2024: 45,371 kWh", None));
        let tree = AccessibilityNode {
            children: vec![
                ax(text, None),
                ax("January 2024: 42,507 kWh", None),
                ancestor,
            ],
            ..AccessibilityNode::default()
        };
        let (hits, total, _) = merged_matches(&input, &tree, "kWh", 20);
        assert_eq!(total, 3, "{owner}");
        assert_eq!(hits[0].line, text);
        assert_eq!(hits.len(), 3);
        assert!(hits[2].line.contains("January 2024: 42,507 kWh"));
        let (hits, total, _) = merged_matches(&input, &tree, "kWh", 1);
        assert_eq!(total, 3);
        assert_eq!(hits.len(), 1);
        let (hits, total, _) = merged_matches(&input, &tree, "absent", 20);
        assert!(hits.is_empty());
        assert_eq!(total, 0);
    }
}

#[path = "find_review_tests.rs"]
mod review;
