use std::collections::{HashMap, HashSet};

use super::block_kind::BlockKind;
use super::control::Control;
use super::dom_tree::DomTree;
use super::element_kind::{
    add_pre_pointers, attribute_text, has_svg_name, heading_path, is_heading_text, matching_field,
    node_outside_svg, pre_control_text, ref_near, search_control, search_coverage, semantic_kind,
    skipped_nodes, svg_metadata, svg_search_text, version_badge,
};
use super::expander::{expander_for, is_hidden_at};
use super::node_kind::NodeKind;
use super::outline_input::OutlineInput;
use super::search_blocks::{block_of, find_folded, fold, is_search_block, own_text, window};
use super::text_match::{CappedRefs, PreControl, TextMatch, line_control};
use super::text_runs::{cut_chars, squash, subtree_text};

const CONTROL_SCAN: usize = 400;
enum Source {
    Both,
    Coverage(bool),
    Pointer,
}
type PreLookup<'a> = (
    &'a mut dyn FnMut(usize) -> Option<PreControl>,
    &'a mut HashSet<usize>,
);

/// Searches captured documents in page order, including hidden text.
/// Preformatted blocks display up to three lines and a summary of the rest.
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
        let (found, total) = Self::search_located(
            input,
            needle,
            keep,
            |tree, index, text| observe(ref_near(input, tree, index), text),
            |_, _, _| true,
        );
        let hits = found.into_iter().map(|(_, _, hit)| hit).collect();
        (hits, total)
    }

    /// Matches beside their source node's target and backend id. Normalized coverage
    /// includes blocks beyond the display cap, for accessibility merging.
    pub fn search_located<'a>(
        input: &'a OutlineInput,
        needle: &str,
        keep: usize,
        mut observe: impl FnMut(&'a DomTree, usize, &str),
        mut unique_pointer: impl FnMut(&str, &str, Option<&str>) -> bool,
    ) -> (Vec<(String, i64, Self)>, usize) {
        let needle = fold(&squash(needle));
        if needle.is_empty() {
            return (Vec::new(), 0);
        }
        let mut found = Vec::new();
        let mut pointers = Vec::new();
        let mut sources = HashMap::new();
        let (mut total, mut pre_sources) = (0, HashMap::new());
        for tree in input.trees.iter().chain(input.frames.values().flatten()) {
            let (more, count) = search_tree(
                input,
                tree,
                &needle,
                keep.saturating_sub(found.len()),
                &mut |tree, index, text, kind| {
                    if keep > 0 {
                        match kind {
                            Source::Coverage(true) => {
                                let key = (tree.target_id.as_str(), tree.nodes[index].backend_id);
                                pre_sources
                                    .insert(key, (text.to_owned(), ref_near(input, tree, index)));
                            }
                            Source::Coverage(false) => {}
                            _ => *sources.entry(text.to_owned()).or_insert(0) += 1,
                        }
                    }
                    if !matches!(kind, Source::Pointer) {
                        observe(tree, index, text);
                    }
                },
            );
            found.extend(more.into_iter().map(|(index, hit, pointer)| {
                pointers.push(pointer);
                (tree.target_id.clone(), tree.nodes[index].backend_id, hit)
            }));
            total += count;
        }
        add_pre_pointers(&mut found, pointers, sources, |target, backend, query| {
            pre_sources
                .get(&(target, backend))
                .is_some_and(|(raw, reference)| unique_pointer(query, raw, *reference))
        });
        (found, total)
    }
}

fn search_tree<'a>(
    input: &OutlineInput,
    tree: &'a DomTree,
    needle: &[char],
    room: usize,
    observe: &mut dyn FnMut(&'a DomTree, usize, &str, Source),
) -> (Vec<(usize, TextMatch, Option<String>)>, usize) {
    let mut found = Vec::new();
    let mut total = 0usize;
    let mut headings: Vec<(u8, String)> = Vec::new();
    let (mut evaluated, mut matched) = (HashSet::new(), HashSet::new());
    let mut represented = HashSet::new();
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
    let mut controls = HashMap::new();
    let mut pre_control_at = |index| {
        controls
            .entry(index)
            .or_insert_with(|| PreControl::at(input, tree, index, &skipped, needle))
            .clone()
    };
    for (index, node) in tree.nodes.iter().enumerate() {
        if skipped.contains(&index) {
            continue;
        }
        if let Some(BlockKind::Heading(level)) = semantic_kind(node, false) {
            headings.retain(|(known, _)| *known < level);
            headings.push((level, subtree_text(tree, index)));
        }
        if pre[index] == Some(index) {
            let section = if found.len() >= room {
                Vec::new()
            } else {
                heading_path(tree, index, &headings, needle)
            };
            let context = (input, tree, index, room.saturating_sub(found.len()));
            let lookup: PreLookup<'_> = (&mut pre_control_at, &mut represented);
            let (lines, count) =
                pre_matches(context, needle, (&skipped, &section), observe, lookup);
            let located = lines
                .into_iter()
                .map(|(hit, pointer)| (index, hit, pointer));
            found.extend(located);
            total += count;
        }
        let block = blocks[index];
        let svg_content = svg[index];
        let svg_label = matches!(&node.kind, NodeKind::Element(_))
            && svg_content
            && (node.tag() == "SVG"
                || has_svg_name(node)
                || svg_metadata(tree, node).next().is_some());
        let control = pre[index]
            .filter(|_| matches!(node.kind, NodeKind::Element(_)))
            .and_then(|_| pre_control_at(index));
        let pre_control = control
            .as_ref()
            .filter(|control| !control.multiline && pre[index] != Some(index));
        let alternative = |text: &str| observe(tree, index, text, Source::Pointer);
        let text = if let Some(control) = pre_control {
            matching_field(
                pre_control_text(tree, index, &control.control),
                needle,
                alternative,
            )
        } else {
            match &node.kind {
                NodeKind::Text(text) if pre[index].is_none() && !text.trim().is_empty() => {
                    if !evaluated.insert(block) {
                        continue;
                    }
                    own_text(tree, block)
                }
                NodeKind::Element(_) if svg_label => svg_search_text(tree, node, &mut |_, text| {
                    observe(tree, index, text, Source::Both)
                }),
                NodeKind::Element(_) => matching_field(attribute_text(node), needle, alternative),
                _ => continue,
            }
        };
        if find_folded(&fold(&text), needle).is_none() {
            continue;
        }
        if represented.contains(&index)
            || control
                .as_ref()
                .is_some_and(|control| control.multiline && control.text_matches)
        {
            observe(tree, index, &text, Source::Pointer);
            continue;
        }
        let block = if svg_label || control.is_some() {
            index
        } else {
            block
        };
        observe(
            tree,
            search_coverage(tree, index, block),
            &text,
            Source::Both,
        );
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
            let hidden = is_hidden_at(tree, index);
            found.push((
                index,
                TextMatch {
                    hits: 1,
                    section: heading_path(tree, index, &headings, needle),
                    line: snippet,
                    control: control_near(input, tree, index, block),
                    hidden,
                    expander: hidden.then(|| expander_for(input, tree, index)).flatten(),
                },
                None,
            ));
        }
    }
    (found, total)
}

fn pre_matches<'a>(
    (input, tree, pre, room): (&OutlineInput, &'a DomTree, usize, usize),
    needle: &[char],
    (skipped, section): (&HashSet<usize>, &[String]),
    observe: &mut dyn FnMut(&'a DomTree, usize, &str, Source),
    (pre_control_at, represented): PreLookup<'_>,
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
            _ => pending.extend(node.children.iter().rev().copied()),
        }
        if raw.len() > start {
            origins.push((start, raw.len(), index));
        }
    }
    let (mut offset, mut total) = (0, 0);
    let mut overflow = CappedRefs::default();
    let mut found = Vec::new();
    for raw_line in raw.split_inclusive('\n') {
        offset += raw_line.len();
        let line = squash(raw_line);
        if find_folded(&fold(&line), needle).is_none() {
            continue;
        }
        let start = offset - raw_line.trim_start().len();
        let end = start + raw_line.trim().len();
        let first = origins.partition_point(|(_, end, _)| *end <= start);
        let last = origins.partition_point(|(start, _, _)| *start < end);
        let mut nodes = origins[first..last].iter().map(|(_, _, index)| *index);
        let control = line_control(tree, pre, nodes.clone(), pre_control_at);
        let mut folded = None;
        if let Some((index, control)) = &control {
            if *index != pre
                && !control.multiline
                && control.text_matches
                && nodes
                    .clone()
                    .all(|node| tree.ancestors(node).contains(index))
            {
                continue;
            }
            if control.multiline || control.text_matches {
                folded = control.control.ref_text.as_deref();
            }
            if *index != pre && !control.multiline && folded.is_some() {
                represented.insert(*index);
            }
        }
        observe(tree, pre, &line, Source::Both);
        total += 1;
        if total > 3 || found.len() == room {
            if total > 3 && found.len() < room {
                overflow.record(folded, false);
            }
            continue;
        }
        if room > 3 {
            overflow.record(
                control
                    .as_ref()
                    .and_then(|(_, control)| control.control.ref_text.as_deref()),
                true,
            );
        }
        let hidden_node = nodes.find(|index| is_hidden_at(tree, *index));
        let pointer =
            (line.chars().count() <= 160 && fold(&line) != needle && !line.contains(['[', ']']))
                .then(|| line.clone());
        found.push((
            TextMatch {
                hits: 1,
                section: section.to_vec(),
                line: window(&line, needle),
                control: control.map(|(_, control)| (*control.control).clone()),
                hidden: hidden_node.is_some(),
                expander: hidden_node.and_then(|index| expander_for(input, tree, index)),
            },
            pointer,
        ));
    }
    if total == 0 {
        return (found, 0);
    }
    let has_pointer = found.iter().any(|(_, pointer)| pointer.is_some());
    observe(tree, pre, &squash(&raw), Source::Coverage(has_pointer));
    if total > 3 && found.len() < room {
        found.push((overflow.row(total - 3, section), None));
    }
    (found, total)
}

/// The control the match sits in, or else the first control of its block.
fn control_near(
    input: &OutlineInput,
    tree: &DomTree,
    index: usize,
    block: usize,
) -> Option<Control> {
    for ancestor in tree.ancestors(index) {
        if let Some(control) = search_control(input, tree, ancestor) {
            return Some(control);
        }
        if ancestor == block && node_outside_svg(tree, index) {
            break;
        }
    }
    let mut pending = vec![block];
    for _ in 0..CONTROL_SCAN {
        let current = pending.pop()?;
        if current != block {
            if let Some(control) =
                search_control(input, tree, current).filter(|control| control.ref_text.is_some())
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
