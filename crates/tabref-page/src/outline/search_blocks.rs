use super::block_kind::BlockKind;
use super::dom_tree::DomTree;
use super::element_kind::{is_block_display, semantic_kind};
use super::node_kind::NodeKind;
use super::text_runs::block_text;

const LINE_WINDOW: usize = 160;
const TAG_BLOCKS: &[&str] = &["DIV", "P", "LI", "DD", "DT", "SECTION", "ARTICLE", "TR"];

/// Whether the element at `index` is a block of its own for search: by
/// meaning, by layout, or, for content that has no layout, by tag. Table
/// cells are not, so a match in a cell reads with the rest of its row.
pub(crate) fn is_search_block(tree: &DomTree, index: usize) -> bool {
    let Some(node) = tree.node(index) else {
        return false;
    };
    if !matches!(node.kind, NodeKind::Element(_)) {
        return false;
    }
    let kind = semantic_kind(node, false);
    if matches!(kind, Some(BlockKind::Cell | BlockKind::HeaderCell)) {
        return false;
    }
    let laid_out_block = node.layout.is_some()
        && is_block_display(node.display())
        && !matches!(node.tag(), "TD" | "TH");
    let tag_block = node.layout.is_none() && TAG_BLOCKS.contains(&node.tag());
    kind.is_some() || laid_out_block || tag_block
}

/// The nearest block holding the node at `index`.
pub(crate) fn block_of(tree: &DomTree, index: usize) -> usize {
    tree.ancestors(index)
        .into_iter()
        .skip(1)
        .find(|ancestor| is_search_block(tree, *ancestor))
        .unwrap_or(0)
}

/// The text of the block at `index` without the blocks nested in it, so a
/// match counts once, in the innermost block that holds it.
pub(crate) fn own_text(tree: &DomTree, index: usize) -> String {
    block_text(tree, index, &|child| is_search_block(tree, child))
}

/// `text` lower-cased one character at a time (the first character of each
/// lower-case form), so positions in the folded text are positions in the
/// original.
pub(crate) fn fold(text: &str) -> Vec<char> {
    text.chars()
        .map(|c| c.to_lowercase().next().unwrap_or(c))
        .collect()
}

/// Where `needle` (already folded) first occurs in `text`, in characters.
pub(crate) fn find_folded(text: &[char], needle: &[char]) -> Option<usize> {
    if needle.is_empty() || needle.len() > text.len() {
        return None;
    }
    (0..=text.len() - needle.len()).find(|start| text[*start..].starts_with(needle))
}

/// The block's text, cut to a window around the first match.
pub(crate) fn window(line: &str, needle: &[char]) -> String {
    let chars: Vec<char> = line.chars().collect();
    if chars.len() <= LINE_WINDOW {
        return line.to_owned();
    }
    let at = find_folded(&fold(line), needle).unwrap_or(0);
    let start = at.saturating_sub(LINE_WINDOW / 3);
    let end = (start + LINE_WINDOW).min(chars.len());
    let mut out: String = chars[start..end].iter().collect();
    if start > 0 {
        out.insert(0, '…');
    }
    if end < chars.len() {
        out.push('…');
    }
    out
}
