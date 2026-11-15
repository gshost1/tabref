use std::collections::HashSet;

use super::block_kind::BlockKind;
use super::control_build::control_of;
use super::dom_tree::DomTree;
use super::element_kind::{is_block_display, is_skipped, semantic_kind};
use super::expander::{expander_for, is_hidden_at};
use super::node_kind::NodeKind;
use super::outline_input::OutlineInput;
use super::text_match::TextMatch;
use super::text_runs::{squash, subtree_text};

const LINE_WINDOW: usize = 160;
const CONTROL_SCAN: usize = 400;
const ATTRIBUTES: &[&str] = &["aria-label", "title", "alt", "placeholder", "value"];

/// Every block of every captured document whose text contains `needle`,
/// ignoring case and spacing, in page order: shown or hidden, offscreen or
/// collapsed. A block that matches more than once counts once. Matches
/// past `keep` are counted but not described; the count comes back beside
/// them.
pub fn search_text(input: &OutlineInput, needle: &str, keep: usize) -> (Vec<TextMatch>, usize) {
    let needle = squash(needle).to_lowercase();
    if needle.is_empty() {
        return (Vec::new(), 0);
    }
    let mut found = Vec::new();
    let mut total = 0usize;
    let trees = input.trees.iter().chain(input.frames.values().flatten());
    for tree in trees {
        let (more, count) = search_tree(input, tree, &needle, keep.saturating_sub(found.len()));
        found.extend(more);
        total += count;
    }
    (found, total)
}

/// The matches in one document, at most `room` of them described, and how
/// many there are in all.
fn search_tree(
    input: &OutlineInput,
    tree: &DomTree,
    needle: &str,
    room: usize,
) -> (Vec<TextMatch>, usize) {
    let mut found = Vec::new();
    let mut total = 0usize;
    let mut headings: Vec<(u8, String)> = Vec::new();
    let mut matched_blocks: HashSet<usize> = HashSet::new();
    let skipped = skipped_nodes(tree);
    for (index, node) in tree.nodes.iter().enumerate() {
        if skipped.contains(&index) {
            continue;
        }
        if let Some(BlockKind::Heading(level)) = semantic_kind(node, false) {
            headings.retain(|(known, _)| *known < level);
            headings.push((level, subtree_text(tree, index)));
        }
        let text = match &node.kind {
            NodeKind::Text(text) => squash(text),
            NodeKind::Element(_) => ATTRIBUTES
                .iter()
                .filter_map(|name| node.attr(name))
                .map(squash)
                .find(|value| value.to_lowercase().contains(needle))
                .unwrap_or_default(),
            _ => continue,
        };
        if !text.to_lowercase().contains(needle) {
            continue;
        }
        let block = block_of(tree, index);
        if !matched_blocks.insert(block) {
            continue;
        }
        total += 1;
        if found.len() >= room {
            continue;
        }
        let in_heading = headings
            .last()
            .is_some_and(|(_, heading)| heading.to_lowercase().contains(needle))
            && is_heading_text(tree, index);
        let section = headings
            .iter()
            .take(headings.len().saturating_sub(usize::from(in_heading)))
            .map(|(_, heading)| heading.clone())
            .collect();
        let hidden = is_hidden_at(tree, index);
        found.push(TextMatch {
            section,
            line: window(&subtree_text(tree, block), needle, &text),
            control: control_near(input, tree, index, block),
            hidden,
            expander: if hidden {
                expander_for(input, tree, index)
            } else {
                None
            },
        });
    }
    (found, total)
}

fn skipped_nodes(tree: &DomTree) -> HashSet<usize> {
    let mut skipped = HashSet::new();
    for (index, node) in tree.nodes.iter().enumerate() {
        let parent_skipped = node.parent.is_some_and(|parent| skipped.contains(&parent));
        let own = matches!(
            node.tag(),
            "SCRIPT" | "STYLE" | "NOSCRIPT" | "TEMPLATE" | "HEAD" | "SVG"
        ) || (is_skipped(node) && node.tag() != "IMG" && node.tag() != "INPUT");
        if parent_skipped || own {
            skipped.insert(index);
        }
    }
    skipped
}

fn is_heading_text(tree: &DomTree, index: usize) -> bool {
    tree.ancestors(index)
        .iter()
        .filter_map(|ancestor| tree.node(*ancestor))
        .any(|node| matches!(semantic_kind(node, false), Some(BlockKind::Heading(_))))
}

/// The nearest ancestor that is a block of its own: by meaning, by layout,
/// or, for content that has no layout, by tag. A table cell gives way to
/// its row, so a match in a cell reads with the rest of the row.
fn block_of(tree: &DomTree, index: usize) -> usize {
    for ancestor in tree.ancestors(index) {
        let Some(node) = tree.node(ancestor) else {
            continue;
        };
        if !matches!(node.kind, NodeKind::Element(_)) {
            continue;
        }
        let laid_out_block = node.layout.is_some() && is_block_display(node.display());
        let tag_block = node.layout.is_none()
            && matches!(
                node.tag(),
                "DIV" | "P" | "LI" | "DD" | "DT" | "SECTION" | "ARTICLE" | "TR"
            );
        let laid_out_block = laid_out_block && !matches!(node.tag(), "TD" | "TH");
        let kind = semantic_kind(node, false);
        if matches!(kind, Some(BlockKind::Cell | BlockKind::HeaderCell)) {
            continue;
        }
        if kind.is_some() || laid_out_block || tag_block {
            return ancestor;
        }
    }
    0
}

/// The block's text, cut to a window around the first match.
fn window(line: &str, needle: &str, fallback: &str) -> String {
    let line = if line.is_empty() { fallback } else { line };
    let chars: Vec<char> = line.chars().collect();
    if chars.len() <= LINE_WINDOW {
        return line.to_owned();
    }
    let lowered: Vec<char> = line.to_lowercase().chars().collect();
    let needle: Vec<char> = needle.chars().collect();
    let at = (0..=lowered.len().saturating_sub(needle.len()))
        .find(|start| lowered[*start..].starts_with(&needle))
        .unwrap_or(0);
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

/// The control the match sits in, or else the first control of its block.
fn control_near(
    input: &OutlineInput,
    tree: &DomTree,
    index: usize,
    block: usize,
) -> Option<super::control::Control> {
    let label_of = |at: usize| {
        let node = tree.node(at)?;
        input.refs.get(&(tree.target_id.clone(), node.backend_id))
    };
    for ancestor in tree.ancestors(index) {
        if let Some(control) = control_of(tree, ancestor, label_of(ancestor)) {
            return Some(control);
        }
        if ancestor == block {
            break;
        }
    }
    let mut pending = vec![block];
    let mut seen = 0usize;
    while let Some(current) = pending.pop() {
        seen += 1;
        if seen > CONTROL_SCAN {
            break;
        }
        if current != block {
            if let Some(control) = control_of(tree, current, label_of(current))
                .filter(|control| control.ref_text.is_some())
            {
                return Some(control);
            }
        }
        if let Some(node) = tree.node(current) {
            pending.extend(node.children.iter().rev());
        }
    }
    None
}

#[cfg(test)]
#[path = "text_search_tests.rs"]
mod tests;
