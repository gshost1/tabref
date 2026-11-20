//! Matches the cells of a pasted table row against one live AX row.

use tabref_core::{AccessibilityNode, RefId, is_ref_able};

use super::indexed_node::{IndexedNode, index_tree};
use super::name_match::normalize;
use super::role_words::role_word;

/// Treat two or more pipe-separated fields as consecutive cells. A
/// displayed control is matched by role and name within its cell. The
/// copied ref is only markup and never used to resolve an element.
pub(super) fn row_anchors(nodes: &[IndexedNode<'_>], scope: &str) -> Vec<usize> {
    if !scope.contains(" | ") {
        return Vec::new();
    }
    let fields: Vec<_> = scope.split(" | ").map(Field::parse).collect();
    nodes
        .iter()
        .enumerate()
        .filter(|(_, entry)| entry.node.role == "row")
        .filter(|(_, entry)| {
            let cells: Vec<_> = entry
                .node
                .children
                .iter()
                .filter(|node| node.role == "cell")
                .collect();
            cells.windows(fields.len()).any(|cells| {
                fields
                    .iter()
                    .zip(cells)
                    .all(|(field, cell)| field.matches(cell))
            })
        })
        .map(|(index, _)| index)
        .collect()
}

enum Field {
    Text(String),
    Controls(Vec<String>),
}

impl Field {
    fn parse(field: &str) -> Self {
        let mut rest = field.trim();
        let mut controls = Vec::new();
        while let Some((control, tail)) = take_control(rest) {
            controls.push(control);
            rest = tail.trim_start();
        }
        if !controls.is_empty() && rest.is_empty() {
            return Self::Controls(controls);
        }
        Self::Text(unescape(field))
    }

    fn matches(&self, cell: &AccessibilityNode) -> bool {
        match self {
            Self::Text(text) => *text == normalize(&cell_text(cell)),
            Self::Controls(labels) => labels_match(labels, cell),
        }
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
    (is_short_ref(reference) || RefId::looks_like_ref(reference))
        .then(|| (unescape(&label.replace("\\|", "|")), &body[close + 1..]))
}

fn is_short_ref(text: &str) -> bool {
    text.strip_prefix('e').is_some_and(|number| {
        !number.is_empty() && number.bytes().all(|byte| byte.is_ascii_digit())
    })
}

fn unescape(text: &str) -> String {
    normalize(&text.replace("\\[", "[").replace("\\]", "]"))
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

fn labels_match(labels: &[String], cell: &AccessibilityNode) -> bool {
    let nodes = index_tree(cell);
    let mut controls = nodes
        .iter()
        .skip(1)
        .map(|entry| entry.node)
        .filter(|node| is_ref_able(node) && node.identity.retained_object.is_some());
    labels.iter().all(|label| {
        controls
            .next()
            .is_some_and(|node| label_matches(label, node))
    }) && controls.next().is_none()
}

fn label_matches(label: &str, node: &AccessibilityNode) -> bool {
    let Some(name) = node.display_name().map(normalize) else {
        return false;
    };
    if node.role == "link" {
        return label == &name
            || label
                .rsplit_once(" -> ")
                .is_some_and(|(label, _)| normalize(label) == name);
    }
    label.split_once(' ').is_some_and(|(role, label)| {
        role_word(role).is_some_and(|roles| roles.contains(&node.role.as_str())) && label == name
    })
}
