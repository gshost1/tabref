use std::collections::{HashMap, HashSet};

use super::block_kind::BlockKind;
use super::control_build::control_of;
use super::dom_tree::DomTree;
use super::element_kind::{
    add_pre_pointers, has_svg_name, heading_path, is_heading_text, node_outside_svg,
    pre_control_text, search_coverage, semantic_kind, skipped_nodes, svg_metadata, svg_search_text,
    version_badge,
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
        let mut pointers = Vec::new();
        let mut sources = HashMap::new();
        let mut total = 0;
        for tree in input.trees.iter().chain(input.frames.values().flatten()) {
            let (more, count) = search_tree(
                input,
                tree,
                &needle,
                keep.saturating_sub(found.len()),
                &mut |tree, index, text, count_source| {
                    if count_source {
                        if let Some(count) = sources.get_mut(text) {
                            *count += 1;
                        } else {
                            sources.insert(text.to_owned(), 1usize);
                        }
                    }
                    observe(tree, index, text);
                },
            );
            found.extend(more.into_iter().map(|(index, hit, pointer)| {
                pointers.push(pointer);
                (tree.target_id.clone(), tree.nodes[index].backend_id, hit)
            }));
            total += count;
        }
        add_pre_pointers(&mut found, pointers, sources);
        (found, total)
    }
}

fn search_tree<'a>(
    input: &OutlineInput,
    tree: &'a DomTree,
    needle: &[char],
    room: usize,
    observe: &mut dyn FnMut(&'a DomTree, usize, &str, bool),
) -> (Vec<(usize, TextMatch, Option<String>)>, usize) {
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
        let pre_control = pre[index].and_then(|_| {
            matches!(node.kind, NodeKind::Element(_))
                .then(|| {
                    control_of(
                        tree,
                        index,
                        input.refs.get(&(tree.target_id.clone(), node.backend_id)),
                    )
                })
                .flatten()
        });
        let text = if let Some(control) = &pre_control {
            pre_control_text(tree, index, control, needle, ATTRIBUTES)
        } else {
            match &node.kind {
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
                        found.extend(lines.into_iter().map(|(hit, pointer)| (pre, hit, pointer)));
                        total += count;
                        continue;
                    }
                    if !evaluated.insert(block) {
                        continue;
                    }
                    own_text(tree, block)
                }
                NodeKind::Element(_) if svg_label => {
                    svg_search_text(tree, node, &mut |_, text| observe(tree, index, text, true))
                }
                NodeKind::Element(_) => ATTRIBUTES
                    .iter()
                    .filter_map(|name| node.attr(name))
                    .map(squash)
                    .find(|value| find_folded(&fold(value), needle).is_some())
                    .unwrap_or_default(),
                _ => continue,
            }
        };
        if find_folded(&fold(&text), needle).is_none() {
            continue;
        }
        let block = if svg_label || pre_control.is_some() {
            index
        } else {
            block
        };
        observe(tree, search_coverage(tree, index, block), &text, true);
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
                None,
            ));
        }
    }
    (found, total)
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
    observe: &mut dyn FnMut(&'a DomTree, usize, &str, bool),
) -> (Vec<(TextMatch, Option<String>)>, usize) {
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
            NodeKind::Element(_)
                if index != pre
                    && control_of(
                        tree,
                        index,
                        input.refs.get(&(tree.target_id.clone(), node.backend_id)),
                    )
                    .is_some() =>
            {
                raw.push(' ')
            }
            _ => pending.extend(node.children.iter().rev().copied()),
        }
        if raw.len() > start {
            origins.push((start, raw.len(), index));
        }
    }
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
        observe(tree, pre, &line, true);
        total += 1;
        if total > 3 || found.len() == room {
            continue;
        }
        let hidden_node = origins
            .iter()
            .filter(|(a, b, _)| *a < offset && *b > start)
            .map(|(_, _, index)| *index)
            .find(|index| is_hidden_at(tree, *index));
        let pointer =
            (line.chars().count() <= 160 && fold(&line) != needle && !line.contains(['[', ']']))
                .then(|| line.clone());
        found.push((
            TextMatch {
                section: section.to_vec(),
                line: window(&line, needle),
                control: None,
                hidden: hidden_node.is_some(),
                expander: hidden_node.and_then(|index| expander_for(input, tree, index)),
            },
            pointer,
        ));
    }
    if total > 0 {
        observe(tree, pre, &squash(&raw), false);
    }
    if total > 3 && found.len() < room {
        found.push((
            TextMatch {
                section: section.to_vec(),
                line: format!("… {} more in this block", total - 3),
                control: None,
                hidden: false,
                expander: None,
            },
            None,
        ));
    }
    (found, total.min(3) + usize::from(total > 3))
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

#[cfg(test)]
#[path = "text_search_pointer_tests.rs"]
mod pointer_tests;
