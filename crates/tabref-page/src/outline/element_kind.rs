use std::collections::{HashMap, HashSet};

use super::block_kind::BlockKind;
use super::control::Control;
use super::control_build::control_of;
use super::dom_node::DomNode;
use super::dom_tree::DomTree;
use super::node_kind::NodeKind;
use super::outline_input::OutlineInput;
use super::search_blocks::{find_folded, fold};
use super::text_match::TextMatch;
use super::text_runs::{squash, subtree_text};

const SKIPPED_TAGS: &[&str] = &[
    "HEAD", "SCRIPT", "STYLE", "NOSCRIPT", "TEMPLATE", "META", "LINK", "TITLE", "CANVAS", "VIDEO",
    "AUDIO", "OBJECT", "EMBED", "SOURCE", "TRACK", "MAP", "IMG", "PICTURE", "HR", "WBR", "PARAM",
    "DATALIST",
];

const SECTIONING: &[&str] = &["ARTICLE", "ASIDE", "MAIN", "NAV", "SECTION"];

pub(crate) const SEARCH_ATTRIBUTES: &[&str] =
    &["aria-label", "title", "alt", "placeholder", "value"];

pub(crate) fn pre_control_text<'a>(
    tree: &'a DomTree,
    index: usize,
    control: &'a Control,
) -> impl Iterator<Item = String> + 'a {
    [
        control.name.clone(),
        control.value.clone().unwrap_or_default(),
        subtree_text(tree, index),
    ]
    .into_iter()
    .chain(attribute_text(&tree.nodes[index]))
}

pub(crate) fn attribute_text(node: &DomNode) -> impl Iterator<Item = String> + '_ {
    SEARCH_ATTRIBUTES
        .iter()
        .filter_map(|name| node.attr(name))
        .map(squash)
}

pub(crate) fn matching_field(
    texts: impl Iterator<Item = String>,
    needle: &[char],
    mut alternative: impl FnMut(&str),
) -> String {
    let mut matches = texts.filter(|text| find_folded(&fold(text), needle).is_some());
    let first = matches.next().unwrap_or_default();
    for text in matches {
        alternative(&text);
    }
    first
}

pub(crate) fn search_control(
    input: &OutlineInput,
    tree: &DomTree,
    index: usize,
) -> Option<Control> {
    let node = tree.node(index)?;
    control_of(
        tree,
        index,
        input.refs.get(&(tree.target_id.clone(), node.backend_id)),
    )
}

pub(crate) fn pre_has_break(tree: &DomTree, root: usize, skipped: &HashSet<usize>) -> bool {
    let mut pending = vec![root];
    while let Some(index) = pending.pop() {
        if skipped.contains(&index) {
            continue;
        }
        let node = &tree.nodes[index];
        if node.tag() == "BR" || matches!(&node.kind, NodeKind::Text(text) if text.contains('\n')) {
            return true;
        }
        pending.extend(node.children.iter().copied());
    }
    false
}

pub(crate) fn heading_path(
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

pub(crate) fn add_pre_pointers(
    found: &mut [(String, i64, TextMatch)],
    pointers: Vec<Option<String>>,
    sources: HashMap<String, usize>,
    mut unique: impl FnMut(&str) -> bool,
) {
    if !pointers.iter().any(Option::is_some) {
        return;
    }
    let sources: Vec<_> = sources
        .into_iter()
        .map(|(text, count)| (fold(&text).into_iter().collect::<String>(), count))
        .collect();
    for ((_, _, hit), pointer) in found.iter_mut().zip(pointers) {
        let Some(query) = pointer else { continue };
        let folded = fold(&query).into_iter().collect::<String>();
        let mut occurrences = 0;
        for (source, count) in &sources {
            if source.contains(&folded) {
                occurrences += count;
                if occurrences > 1 {
                    break;
                }
            }
        }
        if occurrences == 1 && unique(&query) {
            let separator = if query.starts_with('-') { "-- " } else { "" };
            hit.line.push_str(&format!(
                " (tabref find {separator}'{}')",
                query.replace('\'', "'\\''")
            ));
        }
    }
}

pub(crate) fn version_badge(tree: &DomTree, heading: usize) -> Option<String> {
    if !tree.ancestors(heading).iter().any(|ancestor| {
        tree.node(*ancestor)
            .is_some_and(|node| node.tag() == "SUMMARY")
    }) {
        return None;
    }
    let parent = tree.node(tree.node(heading)?.parent?)?;
    let mut pending: Vec<usize> = parent.children.iter().rev().copied().collect();
    for _ in 0..400 {
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

/// Whether the element and everything inside it stay out of the outline:
/// metadata, media, scripts, and what the page hides from assistive
/// technology.
pub(crate) fn is_skipped(node: &DomNode) -> bool {
    is_non_content(node) || (node.tag() == "SVG" && !has_svg_name(node))
}

fn is_non_content(node: &DomNode) -> bool {
    node.form.pseudo
        || SKIPPED_TAGS.contains(&node.tag())
        || node.attr("aria-hidden") == Some("true")
        || (node.tag() == "INPUT"
            && node
                .attr("type")
                .is_some_and(|kind| kind.eq_ignore_ascii_case("hidden")))
}

pub(crate) fn has_svg_name(node: &DomNode) -> bool {
    matches!(
        role_of(node).as_deref(),
        Some("img" | "graphics-document" | "graphics-symbol")
    ) && (has_label(node)
        || node
            .attr("title")
            .is_some_and(|title| !title.trim().is_empty()))
}

pub(crate) fn skip_svg(tree: &DomTree, node: &DomNode) -> bool {
    is_non_content(node)
        || (!has_svg_name(node)
            && !svg_metadata(tree, node).any(|child| !subtree_text(tree, child).is_empty())
            && !node.children.iter().any(|child| {
                tree.node(*child)
                    .is_some_and(|child| !is_non_content(child) && has_svg_name(child))
            }))
}

pub(crate) fn svg_metadata<'a>(
    tree: &'a DomTree,
    node: &'a DomNode,
) -> impl Iterator<Item = usize> + 'a {
    node.children.iter().copied().filter(|child| {
        tree.node(*child).is_some_and(|node| {
            matches!(node.tag(), "TITLE" | "DESC")
                && node.attr("aria-hidden") != Some("true")
                && !node.form.pseudo
        })
    })
}

/// The element's first ARIA role token, lower-cased.
pub(crate) fn role_of(node: &DomNode) -> Option<String> {
    node.attr("role")
        .and_then(|role| role.split_whitespace().next())
        .map(str::to_ascii_lowercase)
}

/// The block an element opens by what it means, before its layout is
/// looked at. `in_section` says whether a sectioning element holds it, which
/// stops `<header>` and `<footer>` from being page landmarks.
pub(crate) fn semantic_kind(node: &DomNode, in_section: bool) -> Option<BlockKind> {
    let role = role_of(node);
    if let Some(kind) = role.as_deref().and_then(|role| kind_of_role(role, node)) {
        return Some(kind);
    }
    let landmark = |name: &str| Some(BlockKind::Landmark(name.to_owned()));
    match node.tag() {
        "H1" | "H2" | "H3" | "H4" | "H5" | "H6" => {
            let level = node.tag()[1..].parse().unwrap_or(2);
            Some(BlockKind::Heading(level))
        }
        "P" | "FIGCAPTION" | "CAPTION" | "LEGEND" | "ADDRESS" => Some(BlockKind::Paragraph),
        "UL" | "OL" | "MENU" => Some(BlockKind::List),
        "LI" => Some(BlockKind::ListItem),
        "DL" => Some(BlockKind::List),
        "DT" => Some(BlockKind::Term),
        "DD" => Some(BlockKind::Description),
        "TABLE" if role.as_deref() != Some("presentation") && role.as_deref() != Some("none") => {
            Some(BlockKind::Table)
        }
        "TR" => Some(BlockKind::Row),
        "TD" => Some(BlockKind::Cell),
        "TH" => Some(BlockKind::HeaderCell),
        "BLOCKQUOTE" => Some(BlockKind::Quote),
        "PRE" => Some(BlockKind::Pre),
        "SVG" => landmark("img"),
        "DETAILS" => Some(BlockKind::Details),
        "MAIN" => landmark("main"),
        "NAV" => landmark("nav"),
        "ASIDE" => landmark("aside"),
        "FORM" => landmark("form"),
        "SEARCH" => landmark("search"),
        "HEADER" if !in_section => landmark("header"),
        "FOOTER" if !in_section => landmark("footer"),
        "SECTION" if has_label(node) => landmark("region"),
        _ => None,
    }
}

fn kind_of_role(role: &str, node: &DomNode) -> Option<BlockKind> {
    let landmark = |name: &str| Some(BlockKind::Landmark(name.to_owned()));
    match role {
        "heading" => {
            let level = node
                .attr("aria-level")
                .and_then(|level| level.trim().parse().ok())
                .unwrap_or(2);
            Some(BlockKind::Heading(level))
        }
        "paragraph" => Some(BlockKind::Paragraph),
        "list" => Some(BlockKind::List),
        "listitem" => Some(BlockKind::ListItem),
        "table" | "grid" | "treegrid" => Some(BlockKind::Table),
        "row" => Some(BlockKind::Row),
        "cell" | "gridcell" => Some(BlockKind::Cell),
        "columnheader" | "rowheader" => Some(BlockKind::HeaderCell),
        "main" => landmark("main"),
        "navigation" => landmark("nav"),
        "banner" => landmark("header"),
        "contentinfo" => landmark("footer"),
        "complementary" => landmark("aside"),
        "form" => landmark("form"),
        "search" => landmark("search"),
        "region" if has_label(node) => landmark("region"),
        "dialog" | "alertdialog" => Some(BlockKind::Dialog),
        _ => None,
    }
}

fn has_label(node: &DomNode) -> bool {
    node.attr("aria-label")
        .is_some_and(|label| !label.trim().is_empty())
        || node.attr("aria-labelledby").is_some()
}

pub(crate) fn is_sectioning(node: &DomNode) -> bool {
    SECTIONING.contains(&node.tag())
}

/// Whether a computed `display` starts a box of its own rather than
/// sitting in the line around it.
pub(crate) fn is_block_display(display: &str) -> bool {
    !(display.is_empty()
        || display.starts_with("inline")
        || display == "contents"
        || display == "none"
        || display == "ruby"
        || display.starts_with("table-column"))
}

/// Whether the element is a dialog by markup: `role=dialog` or
/// `alertdialog`, an open `<dialog>`, or `aria-modal="true"`.
pub(crate) fn is_dialog_markup(node: &DomNode) -> bool {
    matches!(role_of(node).as_deref(), Some("dialog" | "alertdialog"))
        || (node.tag() == "DIALOG" && node.is_shown())
        || node.attr("aria-modal") == Some("true")
}

#[cfg(test)]
#[path = "element_kind_tests.rs"]
mod tests;

pub(crate) fn svg_search_text(
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

pub(crate) fn skipped_nodes(tree: &DomTree, svg: &[bool]) -> HashSet<usize> {
    let mut skipped = HashSet::new();
    for (index, node) in tree.nodes.iter().enumerate() {
        let parent_skipped = node.parent.is_some_and(|parent| skipped.contains(&parent));
        let own = match node.tag() {
            "IMG" | "INPUT" => false,
            "SVG" => skip_svg(tree, node),
            "DESC" if svg[index] => true,
            _ => is_skipped(node),
        };
        if parent_skipped || own {
            skipped.insert(index);
        }
    }
    skipped
}

pub(crate) fn node_outside_svg(tree: &DomTree, mut index: usize) -> bool {
    loop {
        let node = &tree.nodes[index];
        if node.tag() == "SVG" {
            return false;
        }
        let Some(parent) = node.parent else {
            return true;
        };
        index = parent;
    }
}

pub(crate) fn is_heading_text(tree: &DomTree, index: usize) -> bool {
    tree.ancestors(index)
        .iter()
        .filter_map(|ancestor| tree.node(*ancestor))
        .any(|node| matches!(semantic_kind(node, false), Some(BlockKind::Heading(_))))
}

pub(crate) fn search_coverage(tree: &DomTree, index: usize, block: usize) -> usize {
    if matches!(tree.nodes[block].tag(), "" | "HTML" | "BODY") {
        let parent = tree.nodes[index].parent.unwrap_or(index);
        if parent == block { index } else { parent }
    } else {
        block
    }
}
