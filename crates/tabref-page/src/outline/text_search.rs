use std::collections::HashSet;

use super::block_kind::BlockKind;
use super::control_build::control_of;
use super::dom_tree::DomTree;
use super::element_kind::{
    has_svg_name, is_heading_text, is_skipped, node_outside_svg, search_coverage, semantic_kind,
    skipped_nodes, svg_metadata, svg_search_text,
};
use super::expander::{expander_for, is_hidden_at};
use super::node_kind::NodeKind;
use super::outline_input::OutlineInput;
use super::search_blocks::{block_of, find_folded, fold, is_search_block, own_text, window};
use super::text_match::TextMatch;
use super::text_runs::{cut_chars, squash, subtree_text};

const CONTROL_SCAN: usize = 400;
const ATTRIBUTES: &[&str] = &["aria-label", "title", "alt", "placeholder", "value"];

/// Searches captured documents in page order, including hidden text.
/// Preformatted blocks contribute at most three matching lines and one
/// summary of the remaining lines. Other blocks count once.
pub fn search_text(input: &OutlineInput, needle: &str, keep: usize) -> (Vec<TextMatch>, usize) {
    TextMatch::search_with(input, needle, keep, |_, _| {})
}

impl TextMatch {
    /// Reports DOM text and its enclosing ref, including undisplayed hits.
    pub fn search_with(
        input: &OutlineInput,
        needle: &str,
        keep: usize,
        mut observe: impl FnMut(Option<&str>, &str),
    ) -> (Vec<Self>, usize) {
        let (found, total) = Self::search_located(input, needle, keep, |tree, index, text| {
            let mut at = Some(index);
            let mut reference = None;
            while let Some(index) = at {
                let node = &tree.nodes[index];
                reference = input.refs.get(&(tree.target_id.clone(), node.backend_id));
                if reference.is_some() {
                    break;
                }
                at = node.parent;
            }
            observe(reference.map(|label| label.ref_text.as_str()), text);
        });
        (found.into_iter().map(|(_, _, hit)| hit).collect(), total)
    }

    /// Matches beside their source node's target and backend id. Normalized coverage
    /// includes blocks beyond the display cap, for accessibility merging.
    pub fn search_located<'a>(
        input: &'a OutlineInput,
        needle: &str,
        keep: usize,
        mut observe: impl FnMut(&'a DomTree, usize, &str),
    ) -> (Vec<(String, i64, Self)>, usize) {
        let needle = fold(&squash(needle));
        if needle.is_empty() {
            return (Vec::new(), 0);
        }
        let mut found = Vec::new();
        let mut total = 0;
        for tree in input.trees.iter().chain(input.frames.values().flatten()) {
            let (more, count) = search_tree(
                input,
                tree,
                &needle,
                keep.saturating_sub(found.len()),
                &mut observe,
            );
            found.extend(
                more.into_iter().map(|(index, hit)| {
                    (tree.target_id.clone(), tree.nodes[index].backend_id, hit)
                }),
            );
            total += count;
        }
        (found, total)
    }
}

fn search_tree<'a>(
    input: &OutlineInput,
    tree: &'a DomTree,
    needle: &[char],
    room: usize,
    observe: &mut dyn FnMut(&'a DomTree, usize, &str),
) -> (Vec<(usize, TextMatch)>, usize) {
    let mut found = Vec::new();
    let mut total = 0usize;
    let mut headings: Vec<(u8, String)> = Vec::new();
    let mut evaluated: HashSet<usize> = HashSet::new();
    let mut matched: HashSet<usize> = HashSet::new();
    let mut evaluated_pre = HashSet::new();
    let mut pre = vec![None; tree.nodes.len()];
    let mut svg = vec![false; tree.nodes.len()];
    let mut blocks = vec![0; tree.nodes.len()];
    if let Some(root) = blocks.first_mut() {
        *root = block_of(tree, 0);
    }
    for (index, node) in tree.nodes.iter().enumerate() {
        if let Some(parent) = node.parent {
            pre[index] = pre[parent];
            svg[index] = svg[parent];
            blocks[index] = if is_search_block(tree, parent) {
                parent
            } else {
                blocks[parent]
            };
        }
        if node.tag() == "PRE" {
            pre[index] = Some(index);
        }
        svg[index] |= node.tag() == "SVG";
    }
    let skipped = skipped_nodes(tree, &svg);
    for (index, node) in tree.nodes.iter().enumerate() {
        if skipped.contains(&index) {
            continue;
        }
        if let Some(BlockKind::Heading(level)) = semantic_kind(node, false) {
            headings.retain(|(known, _)| *known < level);
            headings.push((level, subtree_text(tree, index)));
        }
        let block = blocks[index];
        let svg_content = svg[index];
        let svg_label = matches!(&node.kind, NodeKind::Element(_))
            && svg_content
            && (node.tag() == "SVG"
                || has_svg_name(node)
                || svg_metadata(tree, node).next().is_some());
        let text = match &node.kind {
            NodeKind::Text(text) if !text.trim().is_empty() => {
                if let Some(pre) = pre[index] {
                    if !evaluated_pre.insert(pre) {
                        continue;
                    }
                    let room = room.saturating_sub(found.len());
                    let section = if room == 0 {
                        Vec::new()
                    } else {
                        heading_path(tree, index, &headings, needle)
                    };
                    let (lines, count) = pre_matches(
                        (input, tree, pre, room),
                        needle,
                        &skipped,
                        &section,
                        observe,
                    );
                    found.extend(lines.into_iter().map(|hit| (pre, hit)));
                    total += count;
                    continue;
                }
                if !evaluated.insert(block) {
                    continue;
                }
                own_text(tree, block)
            }
            NodeKind::Element(_) if svg_label => {
                svg_search_text(tree, node, &mut |_, text| observe(tree, index, text))
            }
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
        let block = if svg_label { index } else { block };
        observe(tree, search_coverage(tree, index, block), &text);
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
            found.push((
                index,
                describe(
                    input,
                    tree,
                    (index, block),
                    heading_path(tree, index, &headings, needle),
                    snippet,
                ),
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
    (index, block): (usize, usize),
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

fn pre_matches<'a>(
    (input, tree, pre, room): (&OutlineInput, &'a DomTree, usize, usize),
    needle: &[char],
    skipped: &HashSet<usize>,
    section: &[String],
    observe: &mut dyn FnMut(&'a DomTree, usize, &str),
) -> (Vec<TextMatch>, usize) {
    let mut raw = String::new();
    let mut origins = Vec::new();
    let mut pending = vec![pre];
    while let Some(index) = pending.pop() {
        if skipped.contains(&index) {
            continue;
        }
        let node = &tree.nodes[index];
        let start = raw.len();
        match &node.kind {
            NodeKind::Text(text) => raw.push_str(text),
            NodeKind::Element(tag) if tag == "BR" => raw.push('\n'),
            _ => pending.extend(node.children.iter().rev().copied()),
        }
        if raw.len() > start {
            origins.push((start, raw.len(), index));
        }
    }
    let locator = section.last().map_or_else(
        || "snap --full".to_owned(),
        |heading| {
            format!(
                "read --section \"{}\"",
                heading.replace('\\', "\\\\").replace('"', "\\\"")
            )
        },
    );
    let multiline = raw.trim_end_matches(['\r', '\n']).contains('\n');
    let mut offset = 0;
    let mut total = 0;
    let mut found = Vec::new();
    for line in raw.split_inclusive('\n') {
        let start = offset;
        offset += line.len();
        let line = squash(line);
        if find_folded(&fold(&line), needle).is_none() {
            continue;
        }
        observe(tree, pre, &line);
        total += 1;
        if total > 3 || found.len() == room {
            continue;
        }
        let hidden_node = origins
            .iter()
            .filter(|(a, b, _)| *a < offset && *b > start)
            .map(|(_, _, index)| *index)
            .find(|index| is_hidden_at(tree, *index));
        let line = window(&line, needle);
        found.push(TextMatch {
            section: section.to_vec(),
            line: if multiline {
                format!("{line} (tabref {locator})")
            } else {
                line
            },
            control: None,
            hidden: hidden_node.is_some(),
            expander: hidden_node.and_then(|index| expander_for(input, tree, index)),
        });
    }
    if total > 0 {
        observe(tree, pre, &squash(&raw));
    }
    if total > 3 && found.len() < room {
        found.push(TextMatch {
            section: section.to_vec(),
            line: format!("… {} more in this block (tabref {locator})", total - 3),
            control: None,
            hidden: false,
            expander: None,
        });
    }
    (found, total.min(3) + usize::from(total > 3))
}

fn version_badge(tree: &DomTree, heading: usize) -> Option<String> {
    if !tree.ancestors(heading).iter().any(|ancestor| {
        tree.node(*ancestor)
            .is_some_and(|node| node.tag() == "SUMMARY")
    }) {
        return None;
    }
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
        if ancestor == block && node_outside_svg(tree, index) {
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
