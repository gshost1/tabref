use tabref_core::{AccessibilityNode, AppError, is_ref_able};

use super::closest::closest;
use super::indexed_node::{IndexedNode, deepest, index_tree};
use super::locate_errors::{ambiguous, no_scope, not_found, not_in_record, past_last};
use super::locate_intent::LocateIntent;
use super::locate_scope::{anchors, beside_anchors, near_anchors};
use super::located::Located;
use super::locator::Locator;
use super::name_match::{NAME_WORDS, RANKS, normalize, rank};
use super::role_words::role_word;
use super::row_scope::row_anchors;
use super::scope_kind::ScopeKind;
use crate::ElementToken;

const CONTEXT_CHARS: usize = 40;
const CONTEXT_CLIMB: usize = 6;

/// Resolves `locator` against a snapshot tree. Matches are ranked: an exact
/// name, then an exact description, then a name starting with the target's
/// words, then one containing them, then a description containing them,
/// and, for clicks and reads, text the page shows standing for the control
/// around it. The best rank with a match decides. One match is the target;
/// several are `AMBIGUOUS_TARGET` unless `nth` or `in` settles it; none is
/// `ELEMENT_NOT_FOUND` with the three closest names. Nothing is guessed:
/// a target either resolves by these rules or fails.
pub fn locate(
    tree: &AccessibilityNode,
    locator: &Locator,
    intent: LocateIntent,
) -> Result<Located, AppError> {
    let nodes = index_tree(tree);
    let roles = locator.role.as_deref().and_then(role_word);
    let acting: Vec<bool> = nodes
        .iter()
        .map(|entry| acts(entry.node, intent, roles))
        .collect();
    let actionable: Vec<usize> = (0..nodes.len()).filter(|&index| acting[index]).collect();
    let scope_anchors = match &locator.scope {
        Some(scope) => {
            let mut found = anchors(&nodes, &normalize(scope));
            if found.is_empty() && locator.scope_kind == ScopeKind::Within {
                found = row_anchors(&nodes, scope);
            }
            if found.is_empty() {
                return Err(no_scope(scope));
            }
            Some(found)
        }
        None => None,
    };
    let mut record_miss = false;
    for tier in tiers(&nodes, (&actionable, &acting), locator, intent) {
        let tier = deepest(&nodes, &tier);
        let picked = match &scope_anchors {
            Some(found) if locator.scope_kind == ScopeKind::Within => {
                match near_anchors(&nodes, &tier, found, locator.nth) {
                    Some(picked) => picked,
                    None => {
                        record_miss = true;
                        continue;
                    }
                }
            }
            Some(found) => {
                let after = locator.scope_kind == ScopeKind::After;
                beside_anchors(&nodes, &tier, found, (after, locator.nth))
            }
            None => match locator.nth {
                Some(n) => vec![
                    *tier
                        .get(n - 1)
                        .ok_or_else(|| past_last(locator, tier.len()))?,
                ],
                None => tier,
            },
        };
        match picked.as_slice() {
            [] => continue,
            [only] => return located(&nodes, *only),
            many => return Err(ambiguous(locator, &labels(&nodes, many), many.len())),
        }
    }
    if record_miss {
        return Err(not_in_record(locator, intent));
    }
    let reachable: Vec<usize> = (0..nodes.len())
        .filter(|&index| acts(nodes[index].node, intent, None))
        .collect();
    let pool: Vec<(usize, String)> = deepest(&nodes, &reachable)
        .into_iter()
        .filter_map(|index| {
            let name = nodes[index].node.display_name()?;
            Some((index, name.to_owned()))
        })
        .collect();
    let query = locator
        .name
        .clone()
        .or_else(|| locator.role.clone())
        .unwrap_or_default();
    let close = closest(&query, &pool, 3);
    Err(not_found(locator, intent, &labels(&nodes, &close)))
}

fn acts(node: &AccessibilityNode, intent: LocateIntent, roles: Option<&[&str]>) -> bool {
    node.identity.retained_object.is_some()
        && is_ref_able(node)
        && intent.accepts(&node.role)
        && roles.is_none_or(|roles| roles.contains(&node.role.as_str()))
}

fn tiers(
    nodes: &[IndexedNode<'_>],
    (actionable, acting): (&[usize], &[bool]),
    locator: &Locator,
    intent: LocateIntent,
) -> Vec<Vec<usize>> {
    let Some(name) = &locator.name else {
        return vec![actionable.to_vec()];
    };
    let query = normalize(name);
    let mut tiers: Vec<Vec<usize>> = RANKS
        .iter()
        .map(|&wanted| {
            actionable
                .iter()
                .copied()
                .filter(|&index| rank(nodes[index].node, &query) == Some(wanted))
                .collect()
        })
        .collect();
    if intent.reaches_text() {
        let mut owners = Vec::new();
        for index in 0..nodes.len() {
            if acting[index]
                || rank(nodes[index].node, &query).is_none_or(|found| found > NAME_WORDS)
            {
                continue;
            }
            if let Some(owner) = owning_control(nodes, acting, index)
                && !owners.contains(&owner)
            {
                owners.push(owner);
            }
        }
        tiers.push(owners);
    }
    tiers
}

fn owning_control(nodes: &[IndexedNode<'_>], acting: &[bool], index: usize) -> Option<usize> {
    let mut at = nodes[index].parent;
    while let Some(parent) = at {
        if acting[parent] {
            return Some(parent);
        }
        at = nodes[parent].parent;
    }
    None
}

fn located(nodes: &[IndexedNode<'_>], index: usize) -> Result<Located, AppError> {
    let node = nodes[index].node;
    let token = node
        .identity
        .retained_object
        .as_deref()
        .map(ElementToken::parse)
        .transpose()?
        .ok_or_else(|| AppError::internal("a located node carries no element token"))?;
    Ok(Located {
        token,
        role: node.role.clone(),
        name: node.display_name().map(str::to_owned),
        ref_id: node.ref_id.clone(),
        context: context(nodes, index),
    })
}

fn labels(nodes: &[IndexedNode<'_>], indices: &[usize]) -> Vec<String> {
    indices
        .iter()
        .map(|&index| {
            let node = nodes[index].node;
            let mut label = node.role.clone();
            if let Some(name) = node.display_name() {
                label.push_str(&format!(" {:?}", cut(name)));
            }
            if let Some(around) = context(nodes, index) {
                label.push_str(&format!(" in {around:?}"));
            }
            label
        })
        .collect()
}

/// The name of the nearest ancestor that names something other than the
/// node itself, such as the row a link sits in.
fn context(nodes: &[IndexedNode<'_>], index: usize) -> Option<String> {
    let own = nodes[index].node.display_name().map(normalize);
    let mut at = nodes[index].parent;
    for _ in 0..CONTEXT_CLIMB {
        let parent = at?;
        let node = nodes[parent].node;
        if node.role == "webarea" {
            return None;
        }
        if let Some(name) = node.display_name()
            && Some(normalize(name)) != own
        {
            return Some(cut(name));
        }
        at = nodes[parent].parent;
    }
    None
}

fn cut(text: &str) -> String {
    let squeezed = text.split_whitespace().collect::<Vec<_>>().join(" ");
    if squeezed.chars().count() <= CONTEXT_CHARS {
        return squeezed;
    }
    let mut kept: String = squeezed.chars().take(CONTEXT_CHARS).collect();
    kept.push('…');
    kept
}

#[cfg(test)]
#[path = "locate_node_tests.rs"]
mod tests;
