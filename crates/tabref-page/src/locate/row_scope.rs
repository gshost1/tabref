//! Matches the printed cells of a pasted table row against one live AX row.

use tabref_core::{AccessibilityNode, RefId, is_ref_able};

use super::indexed_node::IndexedNode;
use super::role_words::role_word;
use crate::outline::control_build::is_printed_control_state;
use crate::{
    CONTROL_NAME_LIMIT, CONTROL_VALUE_LIMIT, cut_chars, escape_control_label, escape_line, squash,
};

/// Fields start at the first visible cell. The view drops blank cells
/// and joins the remaining cells with pipes. A copied ref is markup,
/// so a control is identified by its displayed label in the live cell.
pub(super) fn row_anchors(nodes: &[IndexedNode<'_>], scope: &str) -> Vec<usize> {
    if !scope.contains(" | ") {
        return Vec::new();
    }
    let printed_fields = fields(scope);
    let before_hint = without_cut_hint(scope).map(fields);
    nodes
        .iter()
        .enumerate()
        .filter(|(_, entry)| entry.node.role == "row")
        .filter(|(_, entry)| {
            matches_row(entry.node, &printed_fields)
                || before_hint
                    .as_ref()
                    .is_some_and(|fields| matches_row(entry.node, fields))
        })
        .map(|(index, _)| index)
        .collect()
}

fn matches_row(row: &AccessibilityNode, fields: &[Vec<Part<'_>>]) -> bool {
    let mut cells = row
        .children
        .iter()
        .filter(|node| node.role == "cell")
        .map(Cell::new)
        .filter(|cell| !cell.text.is_empty() || !cell.controls.is_empty());
    fields.len() >= 2
        && fields
            .iter()
            .all(|parts| !parts.is_empty() && cells.next().is_some_and(|cell| cell.matches(parts)))
}

fn without_cut_hint(scope: &str) -> Option<&str> {
    let (row, hint) = scope.rsplit_once(" …(cut: tabref snap")?;
    let (_, line) = hint.strip_suffix(')')?.rsplit_once(" --full --from ")?;
    line.parse::<usize>().ok().map(|_| row)
}

enum Part<'a> {
    Text(String),
    Control(&'a str),
}

fn fields(scope: &str) -> Vec<Vec<Part<'_>>> {
    let mut fields = Vec::new();
    let mut field = Vec::new();
    let mut start = 0;
    for (at, ch) in scope.char_indices() {
        if at < start || scope[..at].ends_with('\\') {
            continue;
        }
        if ch == '['
            && let Some((label, tail)) = take_control(&scope[at..])
        {
            push_text(&mut field, &scope[start..at]);
            field.push(Part::Control(label));
            start = scope.len() - tail.len();
        } else if ch == '|' && scope[..at].ends_with(' ') && scope[at + 1..].starts_with(' ') {
            push_text(&mut field, &scope[start..at]);
            fields.push(std::mem::take(&mut field));
            start = at + 1;
        }
    }
    push_text(&mut field, &scope[start..]);
    fields.push(field);
    fields
}

fn push_text(parts: &mut Vec<Part<'_>>, text: &str) {
    if !text.trim().is_empty() {
        parts.push(Part::Text(
            text.trim()
                .replace("\\[", "[")
                .replace("\\]", "]")
                .replace("\\|", "|"),
        ));
    }
}

fn take_control(input: &str) -> Option<(&str, &str)> {
    let body = input.strip_prefix('[')?;
    let mut chars = body.char_indices();
    let mut skip = 0;
    let close = loop {
        let (index, ch) = chars.next()?;
        if index < skip {
            continue;
        }
        match ch {
            '\\' => {
                chars.next();
            }
            '"' if body[..index].ends_with(" = ") => {
                let (_, tail) = split_quoted_value(&body[index + 1..])?;
                skip = body.len() - tail.len();
            }
            ']' => break index,
            '[' => return None,
            _ => {}
        }
    };
    let (reference, label) = body[..close].split_once(' ')?;
    (is_short_ref(reference) || RefId::looks_like_ref(reference))
        .then_some((label, &body[close + 1..]))
}

fn is_short_ref(text: &str) -> bool {
    text.strip_prefix('e').is_some_and(|number| {
        !number.is_empty() && number.bytes().all(|byte| byte.is_ascii_digit())
    })
}

struct Cell<'a> {
    text: String,
    controls: Vec<&'a AccessibilityNode>,
}

impl<'a> Cell<'a> {
    fn new(node: &'a AccessibilityNode) -> Self {
        let mut controls = Vec::new();
        collect_controls(node, &mut controls);
        Self {
            text: squash(&cell_text(node)).to_lowercase(),
            controls,
        }
    }

    fn matches(&self, parts: &[Part<'_>]) -> bool {
        let mut controls = self.controls.iter();
        let mut text = Vec::new();
        let mut with_values = Vec::new();
        for part in parts {
            match part {
                Part::Text(run) => {
                    text.push(run.as_str());
                    with_values.push(run.as_str());
                }
                Part::Control(label) => {
                    let Some(node) = controls.next() else {
                        return false;
                    };
                    let name = node.display_name().unwrap_or_default();
                    if !label_matches(label, node, name) {
                        return false;
                    }
                    text.push(name);
                    with_values.push(node.identity.value.as_deref().unwrap_or(name));
                }
            }
        }
        controls.next().is_none()
            && (self.text == squash(&text.join(" ")).to_lowercase()
                || self.text == squash(&with_values.join(" ")).to_lowercase())
    }
}

fn cell_text(node: &AccessibilityNode) -> String {
    if let Some(name) = node.display_name() {
        return name.to_owned();
    }
    node.children
        .iter()
        .map(cell_text)
        .collect::<Vec<_>>()
        .join(" ")
}

fn collect_controls<'a>(node: &'a AccessibilityNode, controls: &mut Vec<&'a AccessibilityNode>) {
    for child in &node.children {
        if is_ref_able(child) && child.identity.retained_object.is_some() {
            controls.push(child);
        } else {
            collect_controls(child, controls);
        }
    }
}

fn label_matches(label: &str, node: &AccessibilityNode, name: &str) -> bool {
    let label = squash(label);
    let name = escape_control_label(name);
    let short = cut_chars(&name, CONTROL_NAME_LIMIT).to_lowercase();
    let name = name.to_lowercase();
    let link = node.role == "link";
    let value = node.identity.value.as_deref();
    let matches = |label: &str| {
        matches_name(label, &name, value, link)
            || (short != name && matches_name(label, &short, value, link))
    };
    if link && matches(&label) {
        return true;
    }
    let (role, label) = label.split_once(' ').unwrap_or((&label, ""));
    let role = role.to_lowercase();
    (role == node.role || role_word(&role).is_some_and(|roles| roles.contains(&node.role.as_str())))
        && matches(label)
}

fn matches_name(mut rest: &str, name: &str, live_value: Option<&str>, link: bool) -> bool {
    // Only fold the name and suffix keywords. Value text is case-sensitive.
    for word in name.split_whitespace() {
        let (printed, tail) = rest.split_once(' ').unwrap_or((rest, ""));
        if printed.to_lowercase() != word {
            return false;
        }
        rest = tail;
    }
    if let Some(value) = rest.strip_prefix("= \"") {
        let Some((printed, tail)) = split_quoted_value(value) else {
            return false;
        };
        if !live_value.is_some_and(|value| value_matches(printed, value)) {
            return false;
        }
        rest = tail.trim_start();
    }
    let rest = rest.to_lowercase();
    let mut rest = rest.as_str();
    if rest.starts_with("options: ") {
        return true;
    }
    while let Some((state, tail)) = rest
        .strip_prefix('(')
        .and_then(|state| state.split_once(')'))
    {
        if !is_printed_control_state(state) {
            return false;
        }
        rest = tail.trim_start();
    }
    rest.is_empty() || (link && rest.starts_with("-> "))
}

fn value_matches(printed: &str, live: &str) -> bool {
    // Table lines collapse spaces, and Chrome's AX value trims the edges.
    let printed = printed.trim();
    let matches = |value: &str| printed == squash(&escape_line(value, true));
    matches(live) || (printed.ends_with('…') && matches(&cut_chars(live, CONTROL_VALUE_LIMIT)))
}

fn split_quoted_value(value: &str) -> Option<(&str, &str)> {
    let mut chars = value.char_indices();
    while let Some((index, ch)) = chars.next() {
        match ch {
            '\\' => {
                chars.next();
            }
            '"' => return Some((&value[..index], &value[index + 1..])),
            _ => {}
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_pasted_value_matches_the_escaped_full_or_shortened_value() {
        let prefix = "x".repeat(CONTROL_VALUE_LIMIT - 5);
        let value = format!("{prefix} \"\\\tcontinued");
        assert!(matches_name(
            &format!(r#"CODE = "{prefix} \"\\\tcontinued" (disabled)"#),
            "code",
            Some(&value),
            false,
        ));
        assert!(matches_name(
            &format!(r#"Code = "{prefix} \"\\\t…""#),
            "code",
            Some(&value),
            false,
        ));
        assert!(!matches_name(r#"Code = "x""#, "code", None, false));
        assert!(matches_name("Code", "code", Some(&value), false));
    }
}
