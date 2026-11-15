use tabref_core::{AccessibilityNode, RefMap};

use super::{allocate_stable_refs, is_short_ref};

fn button(name: &str, backend: u32, loader: &str) -> AccessibilityNode {
    let mut node = AccessibilityNode {
        role: "button".into(),
        ..AccessibilityNode::default()
    };
    node.identity.name = Some(name.into());
    node.identity.retained_object = Some(format!("cdp:T:F:{loader}:{backend}"));
    node
}

fn page(children: Vec<AccessibilityNode>) -> AccessibilityNode {
    AccessibilityNode {
        role: "webarea".into(),
        children,
        ..AccessibilityNode::default()
    }
}

fn refs(tree: &AccessibilityNode) -> Vec<String> {
    tree.children
        .iter()
        .map(|child| child.ref_id.clone().unwrap_or_default())
        .collect()
}

#[test]
fn an_element_keeps_its_number_and_a_new_one_gets_the_next() {
    let mut map = RefMap::new("T", "http://x/");
    let mut next = 1;
    let mut first = page(vec![button("Edit", 11, "L"), button("Edit", 12, "L")]);
    assert_eq!(
        allocate_stable_refs(&mut first, "sa", &mut map, &mut next),
        2
    );
    assert_eq!(refs(&first), ["@sa:e1", "@sa:e2"]);

    let mut second = page(vec![
        button("New", 13, "L"),
        button("Edit", 12, "L"),
        button("Edit", 11, "L"),
    ]);
    allocate_stable_refs(&mut second, "sb", &mut map, &mut next);
    assert_eq!(refs(&second), ["@sb:e3", "@sb:e2", "@sb:e1"]);
}

#[test]
fn a_new_document_continues_the_numbers_so_old_ones_never_name_its_elements() {
    let mut map = RefMap::new("T", "http://x/");
    let mut next = 1;
    let mut first = page(vec![button("Edit", 11, "L1")]);
    allocate_stable_refs(&mut first, "sa", &mut map, &mut next);
    let mut later = page(vec![button("Edit", 11, "L2")]);
    allocate_stable_refs(&mut later, "sb", &mut map, &mut next);
    assert_eq!(refs(&later), ["@sb:e2"]);
    assert!(map.get(1).unwrap().token.contains(":L1:"));
}

#[test]
fn short_refs_are_e_and_a_number() {
    assert!(is_short_ref("e4"));
    assert!(is_short_ref("@e12"));
    assert!(!is_short_ref("e0"));
    assert!(!is_short_ref("edit"));
    assert!(!is_short_ref("@s8f3k2p9:e4"));
}

#[test]
fn numbers_another_tab_took_are_never_given_again() {
    let mut next = 1;
    let mut one = RefMap::new("A", "http://x/");
    let mut first = page(vec![button("Delete everything", 11, "LA")]);
    allocate_stable_refs(&mut first, "sa", &mut one, &mut next);
    let mut two = RefMap::new("B", "http://y/");
    let mut second = page(vec![button("Save", 11, "LB")]);
    allocate_stable_refs(&mut second, "sb", &mut two, &mut next);
    assert_eq!(refs(&second), ["@sb:e2"]);
    assert!(two.get(1).unwrap().token.is_empty());
    assert_eq!(next, 3);
}
