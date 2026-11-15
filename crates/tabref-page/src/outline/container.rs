use std::collections::HashMap;

use super::dom_tree::DomTree;
use super::ref_label::RefLabel;
use super::text_runs::subtree_text;

const CONTAINER_TAGS: &[&str] = &[
    "H1", "H2", "H3", "H4", "H5", "H6", "P", "UL", "OL", "TABLE", "LI", "ARTICLE", "SECTION",
    "BUTTON", "INPUT", "SELECT", "TEXTAREA",
];

const LONG_NAME: usize = 160;

/// Whether a control holds content that reads better as blocks than as
/// one name: headings, paragraphs, lists or other controls inside it, or
/// more than a long line of text. A link around a whole product card is a
/// container; a link around a word is not. Form fields never are.
pub(crate) fn is_container(
    tree: &DomTree,
    index: usize,
    refs: &HashMap<(String, i64), RefLabel>,
) -> bool {
    let Some(node) = tree.node(index) else {
        return false;
    };
    if matches!(
        node.tag(),
        "INPUT" | "SELECT" | "TEXTAREA" | "SUMMARY" | "OPTION"
    ) {
        return false;
    }
    let mut pending: Vec<usize> = node.children.clone();
    let mut seen = 0usize;
    while let Some(current) = pending.pop() {
        seen += 1;
        if seen > 4_000 {
            return true;
        }
        let Some(inner) = tree.node(current) else {
            continue;
        };
        let nested_ref = refs.contains_key(&(tree.target_id.clone(), inner.backend_id));
        let nested_link = inner.tag() == "A" && inner.attr("href").is_some();
        if inner.is_shown() && (CONTAINER_TAGS.contains(&inner.tag()) || nested_ref || nested_link)
        {
            return true;
        }
        pending.extend(inner.children.iter().copied());
    }
    subtree_text(tree, index).chars().count() > LONG_NAME
}
