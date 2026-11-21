use std::collections::{HashMap, HashSet};

use serde_json::Value;
use tabref_core::{AccessibilityNode, capability};
use tabref_page::{
    DomTree, ElementToken, NodeKind, TextChange, collect_clickable, escape_line, squash,
    subtree_text,
};

pub(crate) fn new_controls<'a, 'c>(
    tree: &'a AccessibilityNode,
    known: &HashSet<String>,
    captures: impl Fn(&str) -> Option<(&'a [DomTree], &'c Value)>,
) -> Vec<&'a AccessibilityNode> {
    let mut controls = Vec::new();
    collect(tree, known, &mut controls);
    let mut clickable = HashMap::new();
    let mut checked = HashSet::new();
    for node in &controls {
        if matches!(node.role.as_str(), "cell" | "row")
            && let Some(token) = token(node)
            && checked.insert(token.target_id.clone())
            && let Some((_, capture)) = captures(&token.target_id)
        {
            clickable.insert(token.target_id, collect_clickable(capture));
        }
    }
    let documents = |target: &str| captures(target).map(|(docs, _)| docs);
    controls.retain(|node| {
        !native_option(node, &documents) && !passive_table_part(node, &documents, &clickable)
    });
    controls
}

pub(crate) fn without_control_text<'a>(
    tree: &AccessibilityNode,
    changes: &[TextChange],
    documents: impl Fn(&str) -> Option<&'a [DomTree]>,
    displayed: &[&AccessibilityNode],
) -> Vec<TextChange> {
    let owners: Vec<_> = displayed
        .iter()
        .filter_map(|node| owner(node, &documents))
        .collect();
    if owners.is_empty() {
        return changes.to_vec();
    }
    let mut docs = Vec::new();
    let mut targets = HashSet::new();
    let mut pending = vec![tree];
    while let Some(node) = pending.pop() {
        if let Some(token) = token(node)
            && targets.insert(token.target_id.clone())
            && let Some(found) = documents(&token.target_id)
        {
            docs.extend(found);
        }
        pending.extend(&node.children);
    }
    let mut names = HashMap::<String, usize>::new();
    for source in &owners {
        *names.entry(source.name.clone()).or_default() += 1;
    }
    for doc in docs {
        for (index, node) in doc.nodes.iter().enumerate() {
            let NodeKind::Text(text) = &node.kind else {
                continue;
            };
            if !node.is_shown() {
                continue;
            }
            let ancestors = doc.ancestors(index);
            if owners
                .iter()
                .any(|source| std::ptr::eq(source.doc, doc) && ancestors.contains(&source.index))
            {
                continue;
            }
            names.remove(&normalized(text));
            if let Some(block) = ancestors.into_iter().find(|at| {
                doc.node(*at)
                    .is_some_and(|node| !matches!(node.display(), "inline" | "contents" | "none"))
            }) {
                names.remove(&normalized(&subtree_text(doc, block)));
            }
        }
    }
    changes
        .iter()
        .filter(|change| {
            if let TextChange::Added(line) = change
                && let Some(count) = names.get_mut(&normalized(line))
                && *count > 0
            {
                *count -= 1;
                return false;
            }
            true
        })
        .cloned()
        .collect()
}

fn token(node: &AccessibilityNode) -> Option<ElementToken> {
    ElementToken::parse(node.identity.retained_object.as_deref()?).ok()
}

fn location<'a>(
    node: &AccessibilityNode,
    documents: &impl Fn(&str) -> Option<&'a [DomTree]>,
) -> Option<(&'a DomTree, usize)> {
    let token = token(node)?;
    let doc = documents(&token.target_id)?
        .iter()
        .find(|doc| doc.frame_id == token.frame_id)?;
    Some((doc, doc.find_backend(token.backend_node_id)?))
}

fn native_option<'a>(
    node: &AccessibilityNode,
    documents: &impl Fn(&str) -> Option<&'a [DomTree]>,
) -> bool {
    let Some((doc, index)) = location(node, documents) else {
        return false;
    };
    doc.node(index).is_some_and(|node| node.tag() == "OPTION")
        && doc
            .ancestors(index)
            .iter()
            .skip(1)
            .any(|at| doc.node(*at).is_some_and(|node| node.tag() == "SELECT"))
}

fn passive_table_part<'a>(
    node: &AccessibilityNode,
    documents: &impl Fn(&str) -> Option<&'a [DomTree]>,
    clickable: &HashMap<String, HashSet<i64>>,
) -> bool {
    if !matches!(node.role.as_str(), "cell" | "row")
        || [
            capability::SET_FOCUS,
            capability::SET_VALUE,
            capability::TYPE_TEXT,
            capability::SELECT,
            capability::TOGGLE,
            capability::PRESS_KEY,
        ]
        .iter()
        .any(|action| capability::contains(&node.presentation.available_actions, action))
    {
        return false;
    }
    let Some((doc, index)) = location(node, documents) else {
        return false;
    };
    let Some(dom) = doc.node(index) else {
        return false;
    };
    if clickable
        .get(&doc.target_id)
        .is_none_or(|ids| ids.contains(&dom.backend_id))
        || dom
            .attr("id")
            .filter(|id| !id.is_empty())
            .is_some_and(|id| {
                doc.nodes.iter().any(|owner| {
                    owner
                        .attr("aria-activedescendant")
                        .is_some_and(|value| value.trim() == id)
                })
            })
    {
        return false;
    }
    dom.attr("tabindex")
        .and_then(|value| value.trim().parse::<i32>().ok())
        .is_none()
        && !dom.attr("contenteditable").is_some_and(|value| {
            matches!(
                value.to_ascii_lowercase().as_str(),
                "" | "true" | "plaintext-only"
            )
        })
        && dom.attr("onclick").is_none()
}

struct Owner<'a> {
    doc: &'a DomTree,
    index: usize,
    name: String,
}

fn owner<'a>(
    node: &AccessibilityNode,
    documents: &impl Fn(&str) -> Option<&'a [DomTree]>,
) -> Option<Owner<'a>> {
    if !matches!(
        node.role.as_str(),
        "button"
            | "menubutton"
            | "link"
            | "option"
            | "menuitem"
            | "menuitemcheckbox"
            | "menuitemradio"
            | "checkbox"
            | "radiobutton"
            | "switch"
            | "tab"
            | "treeitem"
    ) {
        return None;
    }
    let raw = node.identity.name.as_deref()?;
    if raw.chars().count() > 80 {
        return None;
    }
    let name = normalized(raw);
    let (doc, index) = location(node, documents)?;
    if name.is_empty()
        || !doc.node(index)?.is_shown()
        || name != normalized(&subtree_text(doc, index))
    {
        return None;
    }
    Some(Owner { doc, index, name })
}

fn normalized(text: &str) -> String {
    escape_line(&squash(text), false)
}

fn collect<'a>(
    node: &'a AccessibilityNode,
    known: &HashSet<String>,
    controls: &mut Vec<&'a AccessibilityNode>,
) {
    if node.ref_id.is_some()
        && let Some(token) = &node.identity.retained_object
        && !known.contains(token)
    {
        controls.push(node);
    }
    for child in &node.children {
        collect(child, known, controls);
    }
}
