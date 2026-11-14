use tabref_core::AccessibilityNode;

use super::node_line::node_line;
use super::view_mode::ViewMode;

const INDENT: &str = "  ";

const ALWAYS_SHOWN: &[&str] = &["alert", "alertdialog", "dialog"];

const CONTEXT_ROLES: &[&str] = &[
    "article",
    "banner",
    "complementary",
    "contentinfo",
    "dialog",
    "form",
    "grid",
    "list",
    "listbox",
    "main",
    "menu",
    "navigation",
    "radiogroup",
    "region",
    "row",
    "search",
    "table",
    "tablist",
    "tabpanel",
    "toolbar",
    "webarea",
];

/// Renders `root` one line per shown node, indented two spaces per shown
/// ancestor. A node that is not shown is spliced out and its children take
/// its place, so unnamed wrappers cost nothing. A page's root web area is
/// left out: the caller prints a header for it instead.
pub(crate) fn tree_lines(root: &AccessibilityNode, mode: ViewMode) -> Vec<String> {
    let shown = if root.role == "webarea" {
        root.children
            .iter()
            .flat_map(|child| shown_lines(child, mode))
            .collect()
    } else {
        shown_lines(root, mode)
    };
    shown
        .into_iter()
        .map(|(depth, line)| format!("{}{line}", INDENT.repeat(depth)))
        .collect()
}

fn shown_lines(node: &AccessibilityNode, mode: ViewMode) -> Vec<(usize, String)> {
    let mut inner: Vec<(usize, String)> = node
        .children
        .iter()
        .flat_map(|child| shown_lines(child, mode))
        .collect();
    if !shows(node, mode, !inner.is_empty()) {
        return inner;
    }
    for line in &mut inner {
        line.0 += 1;
    }
    let mut lines = Vec::with_capacity(inner.len() + 1);
    lines.push((0, node_line(node, mode.name_limit())));
    lines.extend(inner);
    lines
}

fn shows(node: &AccessibilityNode, mode: ViewMode, holds_shown: bool) -> bool {
    if node.ref_id.is_some() || ALWAYS_SHOWN.contains(&node.role.as_str()) {
        return true;
    }
    let named = node.display_name().is_some();
    if CONTEXT_ROLES.contains(&node.role.as_str()) {
        return named && holds_shown;
    }
    match mode {
        ViewMode::Interactive => node.role == "heading" && named,
        ViewMode::All => {
            named
                || node
                    .identity
                    .value
                    .as_deref()
                    .is_some_and(|value| !value.trim().is_empty())
        }
    }
}

#[cfg(test)]
#[path = "tree_lines_tests.rs"]
mod tests;
