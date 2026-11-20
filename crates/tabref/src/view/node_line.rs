use std::fmt::Write as _;

use tabref_core::AccessibilityNode;
pub(crate) use tabref_page::escape_line;

/// One node as a line: the ref first, then the role, the quoted name, then
/// `value=` and `desc=` when they add something, then `[states]`. Static
/// text prints as `text`. Names longer than `name_limit` characters end in
/// `...`.
pub(crate) fn node_line(node: &AccessibilityNode, name_limit: usize) -> String {
    let mut line = String::new();
    if let Some(ref_id) = &node.ref_id {
        line.push_str(ref_id);
        line.push(' ');
    }
    line.push_str(role_label(&node.role));
    let name = node.display_name();
    if let Some(name) = name {
        line.push(' ');
        line.push_str(&quote_text(&cut(name, name_limit)));
    }
    if let Some(value) = node
        .identity
        .value
        .as_deref()
        .filter(|value| Some(*value) != name)
    {
        let _ = write!(line, " value={}", quote_text(&cut(value, name_limit)));
    }
    if let Some(description) = node
        .identity
        .description
        .as_deref()
        .filter(|description| Some(description.trim()) != name.map(str::trim))
    {
        let _ = write!(line, " desc={}", quote_text(&cut(description, name_limit)));
    }
    if !node.presentation.states.is_empty() {
        let _ = write!(line, " [{}]", node.presentation.states.join(", "));
    }
    line
}

pub(crate) fn role_label(role: &str) -> &str {
    match role {
        "statictext" => "text",
        other => other,
    }
}

fn cut(text: &str, limit: usize) -> String {
    if text.chars().count() <= limit {
        return text.to_owned();
    }
    let mut kept: String = text.chars().take(limit).collect();
    kept.push_str("...");
    kept
}

/// Quotes `text` on one line: `"` and `\` are escaped, line breaks and tabs
/// become `\n`, `\r` and `\t`, other control characters `\uXXXX`, and
/// bidirectional marks are dropped.
pub(crate) fn quote_text(text: &str) -> String {
    format!("\"{}\"", escape_line(text, true))
}

#[cfg(test)]
#[path = "node_line_tests.rs"]
mod tests;
