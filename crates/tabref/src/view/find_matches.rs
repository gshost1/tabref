use tabref_core::AccessibilityNode;

use super::node_line::node_line;

const NAME_LIMIT: usize = 300;
const CONTEXT_LIMIT: usize = 80;

const ROLE_ALIASES: &[(&str, &str)] = &[
    ("textbox", "textfield"),
    ("textarea", "textfield"),
    ("searchbox", "textfield"),
    ("input", "textfield"),
    ("radio", "radiobutton"),
    ("select", "combobox"),
    ("img", "image"),
    ("text", "statictext"),
    ("spinbutton", "incrementor"),
    ("gridcell", "cell"),
];

/// Lines for the nodes whose name, value or description contains `needle`,
/// ignoring case, deepest first in page order: a node is left out when one
/// of its descendants matches too, so a paragraph does not repeat the link
/// inside it. A match without a ref of its own names the nearest ancestor
/// that has one; one with neither names its nearest named ancestor.
pub(crate) fn find_matches(
    root: &AccessibilityNode,
    needle: &str,
    role: Option<&str>,
) -> Vec<String> {
    let needle = needle.to_lowercase();
    let role = role.map(normalize_role);
    let mut found = Vec::new();
    let mut ancestors = Vec::new();
    visit(root, &needle, role.as_deref(), &mut ancestors, &mut found);
    found
}

fn visit<'a>(
    node: &'a AccessibilityNode,
    needle: &str,
    role: Option<&str>,
    ancestors: &mut Vec<&'a AccessibilityNode>,
    found: &mut Vec<String>,
) {
    let before = found.len();
    ancestors.push(node);
    for child in &node.children {
        visit(child, needle, role, ancestors, found);
    }
    ancestors.pop();
    let own_match = role.is_none_or(|role| node.role == role) && contains(node, needle);
    if own_match && found.len() == before {
        found.push(describe(node, ancestors));
    }
}

fn contains(node: &AccessibilityNode, needle: &str) -> bool {
    [
        node.display_name(),
        node.identity.value.as_deref(),
        node.identity.description.as_deref(),
    ]
    .into_iter()
    .flatten()
    .any(|text| text.to_lowercase().contains(needle))
}

fn describe(node: &AccessibilityNode, ancestors: &[&AccessibilityNode]) -> String {
    let line = node_line(node, NAME_LIMIT);
    if node.ref_id.is_some() {
        return line;
    }
    if let Some(owner) = ancestors
        .iter()
        .rev()
        .find(|ancestor| ancestor.ref_id.is_some())
    {
        return format!("{line} in {}", node_line(owner, CONTEXT_LIMIT));
    }
    let context = ancestors
        .iter()
        .rev()
        .find(|ancestor| ancestor.role != "webarea" && ancestor.display_name().is_some());
    match context {
        Some(context) => format!("{line} in {}", node_line(context, CONTEXT_LIMIT)),
        None => line,
    }
}

fn normalize_role(role: &str) -> String {
    let lowered = role.trim().to_ascii_lowercase();
    ROLE_ALIASES
        .iter()
        .find(|(alias, _)| *alias == lowered)
        .map_or(lowered.clone(), |(_, canonical)| (*canonical).to_owned())
}

#[cfg(test)]
#[path = "find_matches_tests.rs"]
mod tests;
