//! `in "<text>"`: keeps a target to the row or record around some text.
//! The text is looked for at its best rank only (the whole name, then whole
//! words, then part of a word), so `"Row 1"` never stands for `"Row 10"`.
//! From each node that shows it, the search climbs to the nearest ancestor
//! that holds a match. It never climbs past a table, list, menu, form,
//! dialog, article, region or landmark, and it stops where the matches it
//! meets belong to sibling records (other cards beside the one holding the
//! text), so a record without the control finds nothing instead of a
//! neighbour's. Every anchor must lead to a match. Within the holder the
//! match nearest the text wins; a tie is ambiguous. `after` and `before`
//! take the nearest match on that side of the text in reading order
//! instead, for records laid out as plain paragraphs.

use super::indexed_node::{IndexedNode, deepest, distance, within};
use super::name_match::scope_rank;

const NEVER_HOLDS: &[&str] = &[
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

const HOLDS_BUT_ENDS: &[&str] = &["region", "form", "dialog", "alertdialog", "article"];

/// The nodes that show the normalized `scope` text at the best rank any
/// node shows it, deepest only.
pub(crate) fn anchors(nodes: &[IndexedNode<'_>], scope: &str) -> Vec<usize> {
    let ranked: Vec<(usize, u8)> = (0..nodes.len())
        .filter_map(|index| scope_rank(nodes[index].node, scope).map(|rank| (index, rank)))
        .collect();
    let Some(best) = ranked.iter().map(|(_, rank)| *rank).min() else {
        return Vec::new();
    };
    let showing: Vec<usize> = ranked
        .into_iter()
        .filter(|(_, rank)| *rank == best)
        .map(|(index, _)| index)
        .collect();
    deepest(nodes, &showing)
}

/// The match each anchor leads to, without repeats; several at the same
/// distance from one anchor are all returned, which the caller reports as
/// ambiguous. `None` when an anchor's record holds no match, or fewer than
/// `nth`.
pub(crate) fn near_anchors(
    nodes: &[IndexedNode<'_>],
    candidates: &[usize],
    anchors: &[usize],
    nth: Option<usize>,
) -> Option<Vec<usize>> {
    let mut picks = Vec::new();
    for &anchor in anchors {
        let inside = holder_matches(nodes, candidates, anchor)?;
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
    let mut came_from: Option<usize> = None;
    loop {
        let inside: Vec<usize> = candidates
            .iter()
            .copied()
            .filter(|&candidate| within(nodes, at, candidate))
            .collect();
        if !inside.is_empty() {
            let own = own_record(nodes, at, came_from, &inside);
            return (!own.is_empty()).then_some(own);
        }
        let role = nodes[at].node.role.as_str();
        if at != anchor && HOLDS_BUT_ENDS.contains(&role) {
            return None;
        }
        let parent = nodes[at].parent?;
        if NEVER_HOLDS.contains(&nodes[parent].node.role.as_str()) {
            return None;
        }
        came_from = Some(at);
        at = parent;
    }
}

/// The matches in `holder` that belong to the anchor's own record: a match
/// inside another child of `holder` with the same role as the child the
/// anchor came up through sits in a sibling record (the next card of a
/// list) and does not count. A row's cells are fields of one record, so a
/// row keeps all its matches.
fn own_record(
    nodes: &[IndexedNode<'_>],
    holder: usize,
    came_from: Option<usize>,
    inside: &[usize],
) -> Vec<usize> {
    let Some(path) = came_from else {
        return inside.to_vec();
    };
    if nodes[holder].node.role == "row" {
        return inside.to_vec();
    }
    let path_role = nodes[path].node.role.as_str();
    inside
        .iter()
        .copied()
        .filter(|&candidate| {
            let mut child = candidate;
            while nodes[child].parent != Some(holder) {
                match nodes[child].parent {
                    Some(parent) => child = parent,
                    None => return true,
                }
            }
            child == path || nodes[child].node.role != path_role
        })
        .collect()
}

fn nearest(nodes: &[IndexedNode<'_>], anchor: usize, inside: &[usize]) -> Vec<usize> {
    let best = inside
        .iter()
        .map(|&candidate| distance(nodes, anchor, candidate))
        .min()
        .unwrap_or(usize::MAX);
    inside
        .iter()
        .copied()
        .filter(|&candidate| distance(nodes, anchor, candidate) == best)
        .collect()
}

#[cfg(test)]
#[path = "locate_scope_tests.rs"]
mod tests;
