use std::collections::{HashMap, HashSet};

use tabref_core::AccessibilityNode;
use tabref_page::{DomTree, ElementToken, NodeKind, TextChange, escape_line, squash, subtree_text};

pub(crate) struct ReportChanges<'a> {
    pub(crate) controls: Vec<&'a AccessibilityNode>,
    pub(crate) changes: Vec<TextChange>,
}

pub(crate) fn prepare<'a>(
    tree: &'a AccessibilityNode,
    known: &HashSet<String>,
    changes: &[TextChange],
    documents: impl Fn(&str) -> Option<&'a [DomTree]>,
) -> ReportChanges<'a> {
    let mut controls = Vec::new();
    collect(tree, known, &mut controls);
    controls.retain(|node| !native_option(node, &documents));
    let owners: Vec<_> = controls
        .iter()
        .filter_map(|node| owner(node, &documents))
        .collect();
    if owners.is_empty() {
        return ReportChanges {
            controls,
            changes: changes.to_vec(),
        };
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
    let changes = changes
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
        .collect();
    ReportChanges { controls, changes }
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
