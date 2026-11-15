//! `in "<text>"`: keeps a target to the row or region around some text.
//! From each node that shows the text, the search climbs to the nearest
//! ancestor that holds a match, never past a table, list, menu or landmark,
//! so a row without the control finds nothing instead of a neighbour's.
//! Within that ancestor the match nearest the text wins, and on a tie the
//! first one after it. `after` and `before` take the nearest match on that
//! side of the text in reading order instead, for records laid out as
//! plain paragraphs, where no row says which control is whose.

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
        let Some(inside) = holder_matches(nodes, candidates, anchor) else {
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

/// The candidate nearest each anchor on one side in reading order: the
/// first that starts after the anchor's subtree, or the last that ends
/// before the anchor. With `nth`, the nth one counting away from the anchor.
pub(crate) fn beside_anchors(
    nodes: &[IndexedNode<'_>],
    candidates: &[usize],
    anchors: &[usize],
    (after, nth): (bool, Option<usize>),
) -> Vec<usize> {
    let mut picks = Vec::new();
    for &anchor in anchors {
        let mut side: Vec<usize> = candidates
            .iter()
            .copied()
            .filter(|&candidate| {
                if after {
                    candidate >= nodes[anchor].end
                } else {
                    nodes[candidate].end <= anchor
                }
            })
            .collect();
        if !after {
            side.reverse();
        }
        if let Some(&pick) = side.get(nth.unwrap_or(1) - 1)
            && !picks.contains(&pick)
        {
            picks.push(pick);
        }
    }
    picks
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
