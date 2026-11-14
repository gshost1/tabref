use tabref_core::AccessibilityNode;

use super::{
    DESCRIPTION_WORDS, EXACT_DESCRIPTION, EXACT_NAME, NAME_PREFIX, NAME_WORDS, normalize, rank,
};

fn named(name: &str, description: Option<&str>) -> AccessibilityNode {
    let mut node = AccessibilityNode {
        role: "button".into(),
        ..AccessibilityNode::default()
    };
    node.identity.name = Some(name.into());
    node.identity.description = description.map(str::to_owned);
    node
}

#[test]
fn names_compare_without_case_extra_space_or_a_trailing_colon() {
    assert_eq!(normalize("  Price  ($) :"), "price ($)");
    assert_eq!(normalize("Email *"), "email");
}

#[test]
fn an_exact_name_outranks_a_prefix_and_words_inside() {
    assert_eq!(rank(&named("Save", None), "save"), Some(EXACT_NAME));
    assert_eq!(rank(&named("Price ($)", None), "price"), Some(NAME_PREFIX));
    assert_eq!(rank(&named("Unit price", None), "price"), Some(NAME_WORDS));
    assert_eq!(
        rank(&named("▲", Some("Upvote")), "upvote"),
        Some(EXACT_DESCRIPTION)
    );
    assert_eq!(
        rank(&named("▲", Some("Upvote this post")), "upvote"),
        Some(DESCRIPTION_WORDS)
    );
}

#[test]
fn part_of_a_word_is_no_match() {
    assert_eq!(rank(&named("Saved items", None), "save"), None);
    assert_eq!(rank(&named("Presave", None), "save"), None);
}
