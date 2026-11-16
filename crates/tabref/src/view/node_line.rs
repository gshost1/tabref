use std::fmt::Write as _;

use tabref_core::AccessibilityNode;

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
/// bidirectional marks are dropped, as agent-desktop's `--lines` does.
pub(crate) fn quote_text(text: &str) -> String {
    format!("\"{}\"", escape_line(text, true))
}

/// `text` on one line as [`quote_text`] writes it, without the quotes and,
/// unless `quoted`, without escaping `"` and `\`.
pub(crate) fn escape_line(text: &str, quoted: bool) -> String {
    let mut line = String::with_capacity(text.len());
    for character in text.chars().filter(|c| !is_bidirectional_control(*c)) {
        match character {
            '"' if quoted => line.push_str("\\\""),
            '\\' if quoted => line.push_str("\\\\"),
            '\n' => line.push_str("\\n"),
            '\r' => line.push_str("\\r"),
            '\t' => line.push_str("\\t"),
            c if c.is_control() || matches!(c, '\u{2028}' | '\u{2029}') => {
                let _ = write!(line, "\\u{:04x}", u32::from(c));
            }
            c => line.push(c),
        }
    }
    line
}

fn is_bidirectional_control(character: char) -> bool {
    matches!(
        character,
        '\u{061C}' | '\u{200E}' | '\u{200F}' | '\u{202A}'..='\u{202E}' | '\u{2066}'..='\u{2069}'
    )
}

#[cfg(test)]
#[path = "node_line_tests.rs"]
mod tests;
