//! `in "<text>"`: keeps a target to the row or region around some text.
//! From each node that shows the text, the search climbs to the nearest
//! ancestor that holds a match, never past a table, list, menu or landmark,
//! so a row without the control finds nothing instead of a neighbour's.
//! Within that ancestor the match nearest the text wins, and on a tie the
//! first one after it. Text that sits in no row at all, as in a page of
//! comments laid out as plain paragraphs, leads to the first match after
//! it in reading order, within the same table, list or landmark.

use super::indexed_node::{IndexedNode, deepest, distance, within};
use super::name_match::shows_text;

const BOUNDARY_ROLES: &[&str] = &[
    "table",
    "grid",
    "list",
    "listbox",
    "menu",
    "tablist",
    "radiogroup",
    "main",
    "navigation",
    "banner",
    "contentinfo",
    "complementary",
    "webarea",
    "document",
];

const ROW_ROLES: &[&str] = &["row", "article", "cell", "option", "treeitem"];

/// The nodes that show the normalized `scope` text, deepest only.
pub(crate) fn anchors(nodes: &[IndexedNode<'_>], scope: &str) -> Vec<usize> {
    let showing: Vec<usize> = (0..nodes.len())
        .filter(|&index| shows_text(nodes[index].node, scope))
        .collect();
    deepest(nodes, &showing)
}

/// The candidates each anchor leads to, without repeats. With `nth`, the
/// nth candidate inside each anchor's ancestor; `None` when an anchor's
/// ancestor holds fewer.
pub(crate) fn near_anchors(
    nodes: &[IndexedNode<'_>],
    candidates: &[usize],
    anchors: &[usize],
    nth: Option<usize>,
) -> Option<Vec<usize>> {
    let mut picks = Vec::new();
    for &anchor in anchors {
        let Some(inside) = holder_matches(nodes, candidates, anchor)
            .or_else(|| following(nodes, candidates, anchor))
        else {
            continue;
        };
        let chosen = match nth {
            Some(n) => vec![*inside.get(n - 1)?],
            None => nearest(nodes, anchor, &inside),
        };
        for pick in chosen {
            if !picks.contains(&pick) {
                picks.push(pick);
            }
        }
    }
    Some(picks)
}

fn following(nodes: &[IndexedNode<'_>], candidates: &[usize], anchor: usize) -> Option<Vec<usize>> {
    let mut at = nodes[anchor].parent;
    let mut container = None;
    while let Some(parent) = at {
        let role = nodes[parent].node.role.as_str();
        if ROW_ROLES.contains(&role) {
            return None;
        }
        if BOUNDARY_ROLES.contains(&role) {
            container = Some(parent);
            break;
        }
        at = nodes[parent].parent;
    }
    let end = container.map_or(nodes.len(), |container| nodes[container].end);
    candidates
        .iter()
        .copied()
        .find(|&candidate| candidate > anchor && candidate < end)
        .map(|candidate| vec![candidate])
}

fn holder_matches(
    nodes: &[IndexedNode<'_>],
    candidates: &[usize],
    anchor: usize,
) -> Option<Vec<usize>> {
    let mut at = anchor;
    loop {
        let inside: Vec<usize> = candidates
            .iter()
            .copied()
            .filter(|&candidate| within(nodes, at, candidate))
            .collect();
        if !inside.is_empty() {
            return Some(inside);
        }
        let parent = nodes[at].parent?;
        if BOUNDARY_ROLES.contains(&nodes[parent].node.role.as_str()) {
            return None;
        }
        at = parent;
    }
}

fn nearest(nodes: &[IndexedNode<'_>], anchor: usize, inside: &[usize]) -> Vec<usize> {
    let best = inside
        .iter()
        .map(|&candidate| distance(nodes, anchor, candidate))
        .min()
        .unwrap_or(usize::MAX);
    let tied: Vec<usize> = inside
        .iter()
        .copied()
        .filter(|&candidate| distance(nodes, anchor, candidate) == best)
        .collect();
    match tied.iter().find(|&&candidate| candidate > anchor) {
        Some(&after) if tied.len() > 1 => vec![after],
        _ => tied,
    }
}
