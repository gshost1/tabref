//! Previews new refs, keeping controls in a redrawn row or card identifiable.

use std::collections::{HashMap, HashSet};

use tabref_core::AccessibilityNode;
use tabref_page::{
    DomNode, DomTree, ElementToken, PageChanges, TextChange, cut_chars, subtree_text,
};

use super::action_report::styled;
use crate::session::captured_snapshot::CapturedSnapshot;
use crate::view::action_changes::{new_controls, without_control_text};
use crate::view::node_line::{escape_line, node_line, quote_text};
use crate::view::rows_replaced;

const NEW_SHOWN: usize = 8;
const NEW_LINE_CHARS: usize = 80;

pub(super) struct ControlReport {
    pub(super) changes: Vec<TextChange>,
    pub(super) lines: Vec<String>,
    pub(super) new: Vec<String>,
}

struct NewControl<'a> {
    node: &'a AccessibilityNode,
    line: String,
    context: Option<String>,
    changed: bool,
}

impl<'a> NewControl<'a> {
    fn key(&self) -> (&'a str, Option<&'a str>) {
        (&self.node.role, self.node.display_name())
    }
}

pub(super) fn control_report(
    snapshot: &CapturedSnapshot,
    known: &HashSet<String>,
    page: &PageChanges,
) -> ControlReport {
    let documents = |target: &str| snapshot.dom(target);
    let nodes = new_controls(&snapshot.tree, known, |target| {
        documents(target).zip(snapshot.dom_reply(target))
    });
    let (found, shown, summary) = preview_controls(&page.changes, &nodes, documents);
    let displayed: Vec<_> = shown.iter().map(|at| found[*at].node).collect();
    let changes = without_control_text(
        &snapshot.tree,
        rows_replaced::other_changes(page),
        documents,
        &displayed,
    );
    let lines = shown
        .into_iter()
        .map(|at| format!("new {}", found[at].line))
        .chain(summary)
        .collect();
    let new = found.into_iter().map(|control| control.line).collect();
    ControlReport {
        changes,
        lines,
        new,
    }
}

fn preview_controls<'a, 'd>(
    changes: &[TextChange],
    nodes: &[&'a AccessibilityNode],
    documents: impl Fn(&str) -> Option<&'d [DomTree]>,
) -> (Vec<NewControl<'a>>, Vec<usize>, Vec<String>) {
    let (run, table) = changes
        .iter()
        .find_map(|change| match change {
            TextChange::Reordered { run, table, .. } => Some((run.as_slice(), *table)),
            _ => None,
        })
        .unwrap_or_default();
    let reordered: HashSet<_> = run.iter().map(String::as_str).collect();
    let changed: HashSet<_> = changes
        .iter()
        .filter_map(|change| match change {
            TextChange::Added(line) | TextChange::Changed { after: line, .. } => {
                Some(line.as_str())
            }
            _ => None,
        })
        .collect();
    let mut found = Vec::new();
    let mut outside = Vec::new();
    let mut counts = HashMap::new();
    let mut in_rows = 0;
    for node in nodes {
        let context = context(node, &documents, &reordered);
        let line = control_line(node, context.label.as_deref());
        let control = NewControl {
            node,
            line,
            changed: context
                .label
                .as_ref()
                .is_some_and(|label| changed.contains(escape_line(label, false).as_str()))
                || context
                    .row_text
                    .is_some_and(|row| changed.contains(row.as_str())),
            context: context.label,
        };
        if context.in_reorder {
            in_rows += 1;
        } else {
            *counts.entry(control.key()).or_insert(0) += 1;
            outside.push(found.len());
        }
        found.push(control);
    }
    if outside.len() > NEW_SHOWN {
        order_preview(&found, &mut outside, &counts, if table { &[] } else { run });
    }
    let mut summary = Vec::new();
    if outside.len() > NEW_SHOWN {
        summary.push(format!(
            "+{} more new: tabref snap",
            outside.len() - NEW_SHOWN
        ));
        outside.truncate(NEW_SHOWN);
    }
    if in_rows > 0 {
        let noun = if table { "rows" } else { "lines" };
        summary.push(format!(
            "{noun} drawn again: {in_rows} new refs, earlier refs in them are stale: tabref snap"
        ));
    }
    (found, outside, summary)
}

/// Keep uncommon controls and changed containers ahead of repeated ones.
/// A line reorder loses the individual move, so give its last named
/// container a slot. This keeps a card moved to the end of a column visible
/// when the diff describes the intervening cards as part of the same run.
fn order_preview(
    found: &[NewControl<'_>],
    outside: &mut [usize],
    counts: &HashMap<(&str, Option<&str>), usize>,
    run: &[String],
) {
    let labels: HashSet<_> = outside
        .iter()
        .filter_map(|at| found[*at].context.as_ref())
        .map(|label| escape_line(label, false))
        .collect();
    let tail = run.iter().rev().find(|line| labels.contains(*line));
    let mut first = HashMap::new();
    for &at in outside.iter() {
        first.entry(found[at].key()).or_insert(at);
    }
    outside.sort_by_cached_key(|at| {
        let control = &found[*at];
        let key = control.key();
        if counts[&key] == 1 {
            0
        } else if control.changed
            || control
                .context
                .as_ref()
                .is_some_and(|label| tail.is_some_and(|line| *line == escape_line(label, false)))
        {
            1
        } else if first[&key] == *at {
            2
        } else {
            3
        }
    });
}

fn control_line(node: &AccessibilityNode, label: Option<&str>) -> String {
    let mut line = styled(&node_line(node, NEW_LINE_CHARS));
    if let Some(label) = label {
        let context = format!(" in {}", quote_text(&cut_chars(label, NEW_LINE_CHARS)));
        if !node.presentation.states.is_empty()
            && let Some(at) = line.rfind(" [")
        {
            line.insert_str(at, &context);
        } else {
            line.push_str(&context);
        }
    }
    line
}

#[derive(Default)]
struct Context {
    label: Option<String>,
    row_text: Option<String>,
    in_reorder: bool,
}

fn context<'a>(
    control: &AccessibilityNode,
    documents: &impl Fn(&str) -> Option<&'a [DomTree]>,
    reordered: &HashSet<&str>,
) -> Context {
    let Some(token) = control
        .identity
        .retained_object
        .as_deref()
        .and_then(|token| ElementToken::parse(token).ok())
    else {
        return Context::default();
    };
    let Some(doc) = documents(&token.target_id)
        .and_then(|docs| docs.iter().find(|doc| doc.frame_id == token.frame_id))
    else {
        return Context::default();
    };
    let Some(index) = doc.find_backend(token.backend_node_id) else {
        return Context::default();
    };
    let name = control.display_name().unwrap_or_default().trim();
    let mut result = Context::default();
    for ancestor in doc.ancestors(index).into_iter().skip(1) {
        let Some(node) = doc.node(ancestor) else {
            break;
        };
        let ancestor_role = role(node);
        if node.tag() == "DIALOG"
            || matches!(
                ancestor_role,
                Some("dialog" | "alertdialog" | "listbox" | "menu")
            )
        {
            break;
        }
        if is_row(node, ancestor_role) {
            let text = subtree_text(doc, ancestor);
            result.in_reorder = control
                .identity
                .name
                .as_deref()
                .is_some_and(|name| !name.trim().is_empty())
                && reordered.contains(text.as_str());
            result.row_text = Some(escape_line(&text, false));
            if result.label.is_none() {
                result.label = row_label(doc, node, name, &text);
            }
            break;
        }
        if result.label.is_none() && matches!(node.tag(), "DIV" | "ARTICLE" | "SECTION") {
            result.label = child_label(doc, node, name, |child| {
                matches!(child.tag(), "H1" | "H2" | "H3" | "H4" | "H5" | "H6")
                    || role(child) == Some("heading")
            });
        }
    }
    result
}

fn role(node: &DomNode) -> Option<&str> {
    node.attr("role")
        .and_then(|role| role.split_whitespace().next())
}

fn is_row(node: &DomNode, role: Option<&str>) -> bool {
    matches!(node.tag(), "TR" | "LI")
        || matches!(role, Some("row" | "listitem"))
        || matches!(node.display(), "table-row" | "list-item")
}

fn row_label(doc: &DomTree, row: &DomNode, name: &str, text: &str) -> Option<String> {
    child_label(doc, row, name, |child| {
        matches!(child.tag(), "TD" | "TH")
            || matches!(role(child), Some("cell" | "gridcell" | "rowheader"))
    })
    .or_else(|| useful_label(text, name).then(|| text.to_owned()))
}

fn child_label(
    doc: &DomTree,
    node: &DomNode,
    name: &str,
    kind: impl Fn(&DomNode) -> bool,
) -> Option<String> {
    node.children
        .iter()
        .copied()
        .filter(|at| {
            doc.node(*at)
                .is_some_and(|child| child.is_shown() && kind(child))
        })
        .map(|at| subtree_text(doc, at))
        .find(|text| useful_label(text, name))
}

fn useful_label(text: &str, name: &str) -> bool {
    !text.is_empty() && text != name
}

#[cfg(test)]
#[path = "redrawn_rows_tests.rs"]
mod tests;
