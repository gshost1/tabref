use std::sync::atomic::{AtomicI64, Ordering};

use tabref_core::{AccessibilityNode, ErrorCode};

use crate::locate::locate_intent::LocateIntent;
use crate::locate::locate_node::locate;
use crate::locate::locator::Locator;
use crate::locate::word::Word;

static NEXT: AtomicI64 = AtomicI64::new(1);

fn node(role: &str, name: &str, children: Vec<AccessibilityNode>) -> AccessibilityNode {
    let mut node = AccessibilityNode {
        role: role.into(),
        children,
        ..AccessibilityNode::default()
    };
    if !name.is_empty() {
        node.identity.name = Some(name.into());
    }
    node
}

fn control(role: &str, name: &str) -> AccessibilityNode {
    let mut node = node(role, name, Vec::new());
    let id = NEXT.fetch_add(1, Ordering::Relaxed);
    node.identity.retained_object = Some(format!("cdp:T:F:L:{id}"));
    node
}

fn text(name: &str) -> AccessibilityNode {
    node("statictext", name, Vec::new())
}

fn cell(children: Vec<AccessibilityNode>) -> AccessibilityNode {
    node("group", "", children)
}

fn near(tree: &AccessibilityNode, name: &str, scope: &str) -> Result<String, ErrorCode> {
    let words = [Word::quoted(name), Word::bare("in"), Word::quoted(scope)];
    locate(
        tree,
        &Locator::from_words(&words).unwrap(),
        LocateIntent::Click,
    )
    .map(|found| format!("{} {}", found.role, found.name.unwrap_or_default()))
    .map_err(|error| error.code)
}

#[test]
fn scope_text_counts_only_at_its_best_rank_so_row_1_is_not_row_10() {
    let tree = node(
        "webarea",
        "",
        vec![node(
            "table",
            "",
            vec![
                node("row", "", vec![cell(vec![text("Row 1")])]),
                node(
                    "row",
                    "",
                    vec![
                        cell(vec![text("Row 10")]),
                        cell(vec![control("link", "Edit")]),
                    ],
                ),
            ],
        )],
    );
    assert_eq!(
        near(&tree, "Edit", "Row 1"),
        Err(ErrorCode::ElementNotFound)
    );
    assert_eq!(near(&tree, "Edit", "Row 10"), Ok("link Edit".into()));
}

#[test]
fn a_sold_out_card_does_not_borrow_the_next_card_s_button() {
    let card = |title: &str, add: bool| {
        let mut children = vec![node("heading", title, vec![text(title)])];
        children.push(if add {
            control("button", "Add")
        } else {
            text("Sold out")
        });
        node("group", "", children)
    };
    let tree = node(
        "webarea",
        "",
        vec![node(
            "group",
            "",
            vec![
                card("Widget A", true),
                card("Widget B", false),
                card("Widget C", true),
            ],
        )],
    );
    assert_eq!(
        near(&tree, "Add", "Widget B"),
        Err(ErrorCode::ElementNotFound)
    );
    assert_eq!(near(&tree, "Add", "Widget C"), Ok("button Add".into()));
}

#[test]
fn two_matches_equally_near_the_text_are_ambiguous() {
    let tree = node(
        "webarea",
        "",
        vec![node(
            "table",
            "",
            vec![node(
                "row",
                "",
                vec![
                    cell(vec![control("link", "Edit")]),
                    cell(vec![text("Hoodie")]),
                    cell(vec![control("link", "Edit")]),
                ],
            )],
        )],
    );
    assert_eq!(
        near(&tree, "Edit", "Hoodie"),
        Err(ErrorCode::AmbiguousTarget)
    );
}

#[test]
fn a_form_holds_its_own_controls_but_the_search_never_climbs_past_it() {
    let tree = node(
        "webarea",
        "",
        vec![
            node(
                "form",
                "",
                vec![text("Shipping"), control("button", "Save")],
            ),
            node("form", "", vec![text("Billing")]),
            control("button", "Save"),
        ],
    );
    assert_eq!(near(&tree, "Save", "Shipping"), Ok("button Save".into()));
    assert_eq!(
        near(&tree, "Save", "Billing"),
        Err(ErrorCode::ElementNotFound)
    );
}
