use std::collections::HashSet;

use super::block_kind::BlockKind;
use super::control_build::control_of;
use super::dom_node::DomNode;
use super::dom_tree::DomTree;
use super::element_kind::{has_svg_name, is_skipped, semantic_kind, skip_svg, svg_metadata};
use super::expander::{expander_for, is_hidden_at};
use super::node_kind::NodeKind;
use super::outline_input::OutlineInput;
use super::search_blocks::{block_of, find_folded, fold, own_text, window};
use super::text_match::TextMatch;
use super::text_runs::{cut_chars, squash, subtree_text};

const CONTROL_SCAN: usize = 400;
const ATTRIBUTES: &[&str] = &["aria-label", "title", "alt", "placeholder", "value"];

/// Every block of every captured document whose text contains `needle`,
/// ignoring case and spacing, in page order: shown or hidden, offscreen or
/// collapsed. Each preformatted line counts separately. Other blocks
/// that match more than once count once. Matches past `keep` are counted
/// but not described.
pub fn search_text(input: &OutlineInput, needle: &str, keep: usize) -> (Vec<TextMatch>, usize) {
    TextMatch::search_with(input, needle, keep, |_, _| {})
}

impl TextMatch {
    /// Search while reporting DOM text and its enclosing ref to a caller
    /// merging results from another source. Coverage includes undisplayed hits.
    pub fn search_with(
        input: &OutlineInput,
        needle: &str,
        keep: usize,
        mut observe: impl FnMut(Option<&str>, &str),
    ) -> (Vec<Self>, usize) {
        let needle = fold(&squash(needle));
        if needle.is_empty() {
            return (Vec::new(), 0);
        }
        let mut found = Vec::new();
        let mut total = 0usize;
        let trees = input.trees.iter().chain(input.frames.values().flatten());
        for tree in trees {
            let (more, count) = search_tree(
                input,
                tree,
                &needle,
                keep.saturating_sub(found.len()),
                &mut observe,
            );
            found.extend(more);
            total += count;
        }
        (found, total)
    }
}

/// The matches in one document, at most `room` of them described, and how
/// many there are in all. Each block's own text is matched whole, so a
/// match may span inline elements (`Alan <b>Turing</b>`).
fn search_tree(
    input: &OutlineInput,
    tree: &DomTree,
    needle: &[char],
    room: usize,
    observe: &mut dyn FnMut(Option<&str>, &str),
) -> (Vec<TextMatch>, usize) {
    let mut found = Vec::new();
    let mut total = 0usize;
    let mut headings: Vec<(u8, String)> = Vec::new();
    let mut evaluated: HashSet<usize> = HashSet::new();
    let mut matched: HashSet<usize> = HashSet::new();
    let mut evaluated_pre = HashSet::new();
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
        let reference = || {
            tree.ancestors(index).iter().find_map(|ancestor| {
                let node = tree.node(*ancestor)?;
                input
                    .refs
                    .get(&(tree.target_id.clone(), node.backend_id))
                    .map(|label| label.ref_text.as_str())
            })
        };
        let svg_content = is_svg_content(tree, index);
        let svg_label = matches!(&node.kind, NodeKind::Element(_))
            && svg_content
            && (node.tag() == "SVG"
                || has_svg_name(node)
                || svg_metadata(tree, node).next().is_some());
        let text = match &node.kind {
            NodeKind::Text(text) if !text.trim().is_empty() => {
                if find_folded(&fold(&squash(text)), needle).is_some() {
                    observe(reference(), text);
                }
                let pre = tree
                    .ancestors(index)
                    .into_iter()
                    .find(|ancestor| tree.node(*ancestor).is_some_and(|node| node.tag() == "PRE"));
                if let Some(pre) = pre {
                    if !evaluated_pre.insert(pre) {
                        continue;
                    }
                    let raw = preformatted_text(tree, pre, &skipped);
                    let reference = reference();
                    observe(reference, &raw);
                    let multiline = raw.trim_end_matches(['\r', '\n']).contains('\n');
                    for (number, line) in raw.lines().enumerate() {
                        let line = squash(line);
                        if find_folded(&fold(&line), needle).is_none() {
                            continue;
                        }
                        observe(reference, &line);
                        total += 1;
                        if found.len() < room {
                            let line = window(&line, needle);
                            let snippet = if multiline {
                                format!("pre line {}: {line}", number + 1)
                            } else {
                                line
                            };
                            found.push(describe(
                                input,
                                tree,
                                index,
                                pre,
                                heading_path(tree, index, &headings, needle),
                                snippet,
                            ));
                        }
                    }
                    continue;
                }
                if !evaluated.insert(block) {
                    continue;
                }
                own_text(tree, block)
            }
            NodeKind::Element(_) if svg_label => svg_search_text(tree, node, observe),
            NodeKind::Element(_) => ATTRIBUTES
                .iter()
                .filter_map(|name| node.attr(name))
                .map(squash)
                .find(|value| find_folded(&fold(value), needle).is_some())
                .unwrap_or_default(),
            _ => continue,
        };
        if find_folded(&fold(&text), needle).is_none() {
            continue;
        }
        observe(reference(), &text);
        let block = if svg_label { index } else { block };
        if !matched.insert(block) {
            continue;
        }
        total += 1;
        if found.len() < room {
            let mut snippet = window(&text, needle);
            if is_heading_text(tree, index) {
                if let Some(version) = version_badge(tree, block) {
                    snippet.push(' ');
                    snippet.push_str(&cut_chars(&version, 40));
                }
            }
            found.push(describe(
                input,
                tree,
                index,
                block,
                heading_path(tree, index, &headings, needle),
                snippet,
            ));
        }
    }
    (found, total)
}

fn heading_path(
    tree: &DomTree,
    index: usize,
    headings: &[(u8, String)],
    needle: &[char],
) -> Vec<String> {
    let in_heading = headings
        .last()
        .is_some_and(|(_, heading)| find_folded(&fold(heading), needle).is_some())
        && is_heading_text(tree, index);
    headings
        .iter()
        .take(headings.len().saturating_sub(usize::from(in_heading)))
        .map(|(_, heading)| heading.clone())
        .collect()
}

fn describe(
    input: &OutlineInput,
    tree: &DomTree,
    index: usize,
    block: usize,
    section: Vec<String>,
    line: String,
) -> TextMatch {
    let hidden = is_hidden_at(tree, index);
    TextMatch {
        section,
        line,
        control: control_near(input, tree, index, block),
        hidden,
        expander: if hidden {
            expander_for(input, tree, index)
        } else {
            None
        },
    }
}

fn preformatted_text(tree: &DomTree, block: usize, skipped: &HashSet<usize>) -> String {
    let mut raw = String::new();
    let mut pending = vec![block];
    while let Some(index) = pending.pop() {
        if skipped.contains(&index) {
            continue;
        }
        let Some(node) = tree.node(index) else {
            continue;
        };
        match &node.kind {
            NodeKind::Text(text) => raw.push_str(text),
            NodeKind::Element(tag) if tag == "BR" => raw.push('\n'),
            _ => pending.extend(node.children.iter().rev().copied()),
        }
    }
    raw
}

fn version_badge(tree: &DomTree, heading: usize) -> Option<String> {
    let parent = tree.node(tree.node(heading)?.parent?)?;
    let mut pending: Vec<usize> = parent.children.iter().rev().copied().collect();
    for _ in 0..CONTROL_SCAN {
        let index = pending.pop()?;
        let Some(node) = tree.node(index) else {
            continue;
        };
        if node.tag() != "SPAN" || is_skipped(node) {
            continue;
        }
        if node
            .attr("class")
            .is_some_and(|classes| classes.split_whitespace().any(|class| class == "since"))
        {
            let text = subtree_text(tree, index);
            if !text.is_empty() {
                return Some(text);
            }
        }
        pending.extend(node.children.iter().rev().copied());
    }
    None
}

fn is_svg_content(tree: &DomTree, index: usize) -> bool {
    let Some(node) = tree.node(index) else {
        return false;
    };
    node.tag() == "SVG"
        || (matches!(node.kind, NodeKind::Element(_))
            && (has_svg_name(node) || !node.children.is_empty())
            && tree
                .ancestors(index)
                .iter()
                .skip(1)
                .any(|ancestor| tree.node(*ancestor).is_some_and(|node| node.tag() == "SVG")))
}

fn svg_search_text(
    tree: &DomTree,
    node: &DomNode,
    observe: &mut dyn FnMut(Option<&str>, &str),
) -> String {
    let mut parts = Vec::new();
    for part in ["aria-label", "title"]
        .into_iter()
        .filter_map(|name| node.attr(name))
        .map(squash)
        .chain(svg_metadata(tree, node).map(|child| subtree_text(tree, child)))
    {
        if !part.is_empty() && !parts.contains(&part) {
            observe(None, &part);
            parts.push(part);
        }
    }
    parts.join(" ")
}

fn skipped_nodes(tree: &DomTree) -> HashSet<usize> {
    let mut skipped = HashSet::new();
    for (index, node) in tree.nodes.iter().enumerate() {
        let parent_skipped = node.parent.is_some_and(|parent| skipped.contains(&parent));
        let own = match node.tag() {
            "IMG" | "INPUT" => false,
            "SVG" => skip_svg(tree, node),
            "DESC" if is_svg_content(tree, index) => true,
            _ => is_skipped(node),
        };
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
