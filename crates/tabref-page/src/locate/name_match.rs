//! How a target's name meets a node's: compared after lowering case,
//! collapsing whitespace and dropping a trailing `:` or `*`, and ranked so
//! an exact name beats a name that only starts with the target's words.

use tabref_core::AccessibilityNode;

/// The match ranks, best first. A node matches at the best rank it reaches.
pub(crate) const EXACT_NAME: u8 = 1;
pub(crate) const EXACT_DESCRIPTION: u8 = 2;
pub(crate) const NAME_PREFIX: u8 = 3;
pub(crate) const NAME_WORDS: u8 = 4;
pub(crate) const DESCRIPTION_WORDS: u8 = 5;
pub(crate) const RANKS: [u8; 5] = [
    EXACT_NAME,
    EXACT_DESCRIPTION,
    NAME_PREFIX,
    NAME_WORDS,
    DESCRIPTION_WORDS,
];

pub(crate) fn normalize(text: &str) -> String {
    let collapsed = text
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_lowercase();
    collapsed.trim_end_matches([':', '*']).trim_end().to_owned()
}

/// The best rank at which `node` matches the normalized `query`.
pub(crate) fn rank(node: &AccessibilityNode, query: &str) -> Option<u8> {
    if query.is_empty() {
        return None;
    }
    let name = node.display_name().map(normalize).unwrap_or_default();
    let description = node
        .identity
        .description
        .as_deref()
        .map(normalize)
        .unwrap_or_default();
    if name == query {
        Some(EXACT_NAME)
    } else if description == query {
        Some(EXACT_DESCRIPTION)
    } else if starts_with_words(&name, query) {
        Some(NAME_PREFIX)
    } else if contains_words(&name, query) {
        Some(NAME_WORDS)
    } else if contains_words(&description, query) {
        Some(DESCRIPTION_WORDS)
    } else {
        None
    }
}

/// Whether the node's name or value contains the normalized `query`
/// anywhere, as the text a scope names.
pub(crate) fn shows_text(node: &AccessibilityNode, query: &str) -> bool {
    [node.display_name(), node.identity.value.as_deref()]
        .into_iter()
        .flatten()
        .any(|text| normalize(text).contains(query))
}

fn starts_with_words(haystack: &str, query: &str) -> bool {
    haystack.starts_with(query) && boundary_after(haystack, query.len())
}

fn contains_words(haystack: &str, query: &str) -> bool {
    haystack.match_indices(query).any(|(start, _)| {
        let before = haystack[..start].chars().next_back();
        before.is_none_or(|c| !c.is_alphanumeric()) && boundary_after(haystack, start + query.len())
    })
}

fn boundary_after(haystack: &str, end: usize) -> bool {
    haystack[end..]
        .chars()
        .next()
        .is_none_or(|c| !c.is_alphanumeric())
}

#[cfg(test)]
#[path = "name_match_tests.rs"]
mod tests;
