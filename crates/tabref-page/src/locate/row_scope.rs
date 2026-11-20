//! Matches the printed cells of a pasted table row against one live AX row.

use tabref_core::{AccessibilityNode, RefId, is_ref_able};

use super::indexed_node::IndexedNode;
use super::name_match::normalize;
use super::role_words::role_word;
use crate::squash;

/// Fields start at the first visible cell. The view drops blank cells
/// and joins the remaining cells with pipes. A copied ref is markup,
/// so a control is identified by its displayed label in the live cell.
pub(super) fn row_anchors(nodes: &[IndexedNode<'_>], scope: &str) -> Vec<usize> {
    if !scope.contains(" | ") {
        return Vec::new();
    }
    let fields = fields(scope);
    if fields.len() < 2 || fields.iter().any(Vec::is_empty) {
        return Vec::new();
    }
    nodes
        .iter()
        .enumerate()
        .filter(|(_, entry)| entry.node.role == "row")
        .filter(|(_, entry)| {
            let mut cells = entry
                .node
                .children
                .iter()
                .filter(|node| node.role == "cell")
                .map(Cell::new)
                .filter(|cell| !cell.text.is_empty() || !cell.controls.is_empty());
            fields
                .iter()
                .all(|parts| cells.next().is_some_and(|cell| cell.matches(parts)))
        })
        .map(|(index, _)| index)
        .collect()
}

enum Part {
    Text(String),
    Control(String),
}

fn fields(scope: &str) -> Vec<Vec<Part>> {
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

fn push_text(parts: &mut Vec<Part>, text: &str) {
    if !text.trim().is_empty() {
        parts.push(Part::Text(unescape(text.trim())));
    }
}

fn take_control(input: &str) -> Option<(String, &str)> {
    let body = input.strip_prefix('[')?;
    let mut chars = body.char_indices();
    let close = loop {
        let (index, ch) = chars.next()?;
        match ch {
            '\\' => {
                chars.next();
            }
            ']' => break index,
            '[' => return None,
            _ => {}
        }
    };
    let (reference, label) = body[..close].split_once(' ')?;
    (is_short_ref(reference) || RefId::looks_like_ref(reference)).then(|| {
        (
            normalize(&unescape(&label.replace("\\|", "|"))),
            &body[close + 1..],
        )
    })
}

fn is_short_ref(text: &str) -> bool {
    text.strip_prefix('e').is_some_and(|number| {
        !number.is_empty() && number.bytes().all(|byte| byte.is_ascii_digit())
    })
}

fn unescape(text: &str) -> String {
    text.replace("\\[", "[").replace("\\]", "]")
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

    fn matches(&self, parts: &[Part]) -> bool {
        let mut controls = self.controls.iter();
        let mut text = Vec::new();
        for part in parts {
            match part {
                Part::Text(run) => text.push(run.as_str()),
                Part::Control(label) => {
                    let Some(node) = controls.next() else {
                        return false;
                    };
                    let name = node.display_name().unwrap_or_default();
                    if !label_matches(label, node, name) {
                        return false;
                    }
                    text.push(name);
                }
            }
        }
        controls.next().is_none() && self.text == squash(&text.join(" ")).to_lowercase()
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
    let name = normalize(name);
    if node.role == "link" {
        return label == name
            || label
                .rsplit_once(" -> ")
                .is_some_and(|(label, _)| normalize(label) == name);
    }
    let (role, label) = label.split_once(' ').unwrap_or((label, ""));
    role_word(role).is_some_and(|roles| roles.contains(&node.role.as_str())) && label == name
}
