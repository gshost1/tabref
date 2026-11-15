use std::collections::HashSet;

use super::block_kind::BlockKind;
use super::control_build::control_of;
use super::dom_tree::DomTree;
use super::element_kind::{is_skipped, semantic_kind};
use super::expander::{expander_for, is_hidden_at};
use super::node_kind::NodeKind;
use super::outline_input::OutlineInput;
use super::search_blocks::{block_of, find_folded, fold, own_text, window};
use super::text_match::TextMatch;
use super::text_runs::{squash, subtree_text};

const CONTROL_SCAN: usize = 400;
const ATTRIBUTES: &[&str] = &["aria-label", "title", "alt", "placeholder", "value"];

/// Every block of every captured document whose text contains `needle`,
/// ignoring case and spacing, in page order: shown or hidden, offscreen or
/// collapsed. A block that matches more than once counts once. Matches
/// past `keep` are counted but not described; the count comes back beside
/// them.
pub fn search_text(input: &OutlineInput, needle: &str, keep: usize) -> (Vec<TextMatch>, usize) {
    let needle = fold(&squash(needle));
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
/// many there are in all. Each block's own text is matched whole, so a
/// match may span inline elements (`Alan <b>Turing</b>`).
fn search_tree(
    input: &OutlineInput,
    tree: &DomTree,
    needle: &[char],
    room: usize,
) -> (Vec<TextMatch>, usize) {
    let mut found = Vec::new();
    let mut total = 0usize;
    let mut headings: Vec<(u8, String)> = Vec::new();
    let mut evaluated: HashSet<usize> = HashSet::new();
    let mut matched: HashSet<usize> = HashSet::new();
    let skipped = skipped_nodes(tree);
    for (index, node) in tree.nodes.iter().enumerate() {
        if skipped.contains(&index) {
            continue;
        }
        if let Some(BlockKind::Heading(level)) = semantic_kind(node, false) {
            headings.retain(|(known, _)| *known < level);
            headings.push((level, subtree_text(tree, index)));
        }
        let block = block_of(tree, index);
        let text = match &node.kind {
            NodeKind::Text(text) if !text.trim().is_empty() => {
                if !evaluated.insert(block) {
                    continue;
                }
                own_text(tree, block)
            }
            NodeKind::Element(_) => ATTRIBUTES
                .iter()
                .filter_map(|name| node.attr(name))
                .map(squash)
                .find(|value| find_folded(&fold(value), needle).is_some())
                .unwrap_or_default(),
            _ => continue,
        };
        if find_folded(&fold(&text), needle).is_none() || !matched.insert(block) {
            continue;
        }
        total += 1;
        if found.len() >= room {
            continue;
        }
        let in_heading = headings
            .last()
            .is_some_and(|(_, heading)| find_folded(&fold(heading), needle).is_some())
            && is_heading_text(tree, index);
        let section = headings
            .iter()
            .take(headings.len().saturating_sub(usize::from(in_heading)))
            .map(|(_, heading)| heading.clone())
            .collect();
        let hidden = is_hidden_at(tree, index);
        found.push(TextMatch {
            section,
            line: window(&text, needle),
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
