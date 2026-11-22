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
    let mut found = Vec::new();
    visit_matches(root, needle, role, |_, _, line| found.push(line()));
    found
}

/// Visits each AX hit with its nearest ref and matching source text.
pub(crate) fn visit_matches(
    root: &AccessibilityNode,
    needle: &str,
    role: Option<&str>,
    mut hit: impl FnMut(Option<&str>, &[&str], &dyn Fn() -> String),
) {
    visit(
        root,
        &needle.to_lowercase(),
        role.map(normalize_role).as_deref(),
        &mut Vec::new(),
        &mut hit,
    );
}

fn visit<'a>(
    node: &'a AccessibilityNode,
    needle: &str,
    role: Option<&str>,
    ancestors: &mut Vec<&'a AccessibilityNode>,
    hit: &mut impl FnMut(Option<&str>, &[&str], &dyn Fn() -> String),
) -> bool {
    let mut descendant_matched = false;
    ancestors.push(node);
    for child in &node.children {
        descendant_matched |= visit(child, needle, role, ancestors, hit);
    }
    ancestors.pop();
    if descendant_matched || role.is_some_and(|role| node.role != role) {
        return descendant_matched;
    }
    let mut matching = [
        node.display_name(),
        node.identity.value.as_deref(),
        node.identity.description.as_deref(),
    ];
    matching.iter_mut().for_each(|text| {
        *text = text.filter(|text| text.to_lowercase().contains(needle));
    });
    let texts: Vec<&str> = matching.into_iter().flatten().collect();
    if texts.is_empty() {
        return false;
    }
    let reference = node.ref_id.as_deref().or_else(|| {
        ancestors
            .iter()
            .rev()
            .find_map(|ancestor| ancestor.ref_id.as_deref())
    });
    hit(reference, &texts, &|| describe(node, ancestors));
    true
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
