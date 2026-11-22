//! Previews new refs, keeping controls in a redrawn row or card identifiable.

use std::collections::{HashMap, HashSet};

use tabref_core::AccessibilityNode;
use tabref_page::{DomNode, DomTree, PageChanges, TextChange, cut_chars, subtree_text};

use super::action_report::styled;
use crate::session::captured_snapshot::CapturedSnapshot;
use crate::view::action_changes::{location, prepare};
use crate::view::node_line::{escape_line, node_line, quote_text};
use crate::view::rows_replaced;

#[path = "redrawn_rows/headings.rs"]
mod headings;
use headings::{ControlKey, HeadingControls, HeadingLabel, control_key, ref_nodes};

const NEW_SHOWN: usize = 8;
const NEW_LINE_CHARS: usize = 80;

pub(super) struct ControlReport {
    pub(super) changes: Vec<TextChange>,
    pub(super) lines: Vec<String>,
    pub(super) new: Vec<String>,
}

struct NewControl<'a> {
    node: &'a AccessibilityNode,
    context: Option<String>,
    changed: bool,
}

impl<'a> NewControl<'a> {
    fn key(&self) -> ControlKey<'a> {
        control_key(self.node)
    }
}

#[derive(Default)]
struct ControlPreview {
    labels: Vec<Option<String>>,
    shown: Vec<usize>,
    summary: Vec<String>,
}

pub(super) fn control_report(
    snapshot: &CapturedSnapshot,
    known: &HashSet<String>,
    page: &PageChanges,
) -> ControlReport {
    let documents = |target: &str| snapshot.dom(target);
    let mut preview = ControlPreview::default();
    let prepared = prepare(
        &snapshot.tree,
        known,
        rows_replaced::other_changes(page),
        |target| documents(target).zip(snapshot.dom_reply(target)),
        |nodes| {
            preview = preview_controls(&page.changes, nodes, documents, ref_nodes(&snapshot.tree));
            preview
                .shown
                .iter()
                .filter_map(|at| nodes[*at].identity.retained_object.clone())
                .collect()
        },
    );
    let new: Vec<_> = prepared
        .controls
        .iter()
        .zip(preview.labels)
        .map(|(node, label)| control_line(node, label.as_deref()))
        .collect();
    let lines = preview
        .shown
        .into_iter()
        .map(|at| format!("new {}", new[at]))
        .chain(preview.summary)
        .collect();
    ControlReport {
        changes: prepared.changes,
        lines,
        new,
    }
}

fn preview_controls<'n, 'd>(
    changes: &[TextChange],
    nodes: &[&'n AccessibilityNode],
    documents: impl Fn(&str) -> Option<&'d [DomTree]>,
    all_controls: impl IntoIterator<Item = &'n AccessibilityNode>,
) -> ControlPreview {
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
    let heading_controls = HeadingControls::new(nodes, all_controls, &documents);
    for node in nodes {
        let context = context(node, &documents, &reordered, &heading_controls);
        let control = NewControl {
            node,
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
    ControlPreview {
        labels: distinct_labels(&found),
        shown: outside,
        summary,
    }
}

fn distinct_labels(controls: &[NewControl<'_>]) -> Vec<Option<String>> {
    let labels: Vec<_> = controls
        .iter()
        .map(|control| {
            control.context.as_ref().map(|label| {
                let text = cut_chars(label, NEW_LINE_CHARS);
                (quote_text(&text), text)
            })
        })
        .collect();
    let mut by_control = HashMap::<_, HashMap<&str, usize>>::new();
    for (control, label) in controls.iter().zip(&labels) {
        if let Some((quoted, _)) = label {
            *by_control
                .entry(control.key())
                .or_default()
                .entry(quoted)
                .or_default() += 1;
        }
    }
    controls
        .iter()
        .zip(&labels)
        .map(|(control, label)| {
            let (quoted, text) = label.as_ref()?;
            let counts = by_control.get(&control.key())?;
            (counts.len() > 1 && counts.get(quoted.as_str()) == Some(&1)).then(|| text.clone())
        })
        .collect()
}

/// Keep uncommon controls and changed containers ahead of repeated ones.
/// A line reorder loses the individual move, so give its last named
/// container a slot. This keeps a card moved to the end of a column visible
/// when the diff describes the intervening cards as part of the same run.
fn order_preview(
    found: &[NewControl<'_>],
    outside: &mut [usize],
    counts: &HashMap<ControlKey<'_>, usize>,
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
        let context = format!(" in {}", quote_text(label));
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
    heading_controls: &HeadingControls<'_, 'a>,
) -> Context {
    let Some((doc, index)) = location(control, documents) else {
        return Context::default();
    };
    let name = control.display_name().unwrap_or_default().trim();
    let mut result = Context::default();
    let mut heading_scope_found = false;
    let ancestors = doc.ancestors(index);
    for &ancestor in &ancestors {
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
            if ancestor != index && !heading_scope_found {
                result.label = row_label(doc, node, name, &text, &ancestors);
            }
            break;
        }
        if ancestor != index
            && !heading_scope_found
            && matches!(node.tag(), "DIV" | "ARTICLE" | "SECTION")
            && let HeadingLabel::InScope(label) = heading_controls.label(control, doc, node, index)
        {
            result.label = label;
            heading_scope_found = true;
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

fn row_label(
    doc: &DomTree,
    row: &DomNode,
    name: &str,
    text: &str,
    ancestors: &[usize],
) -> Option<String> {
    if row.tag() == "LI" || role(row) == Some("listitem") || row.display() == "list-item" {
        return useful_label(text, name).then(|| text.to_owned());
    }
    let mut cells = row.children.iter().copied().filter(|index| {
        doc.node(*index).is_some_and(|child| {
            child.is_shown()
                && (matches!(child.tag(), "TD" | "TH")
                    || matches!(
                        role(child),
                        Some("cell" | "gridcell" | "rowheader" | "columnheader")
                    ))
        })
    });
    let first_cell = cells.next()?;
    let header = std::iter::once(first_cell).chain(cells).find(|index| {
        doc.node(*index).is_some_and(|child| {
            !ancestors.contains(index)
                && (role(child) == Some("rowheader")
                    || (child.tag() == "TH" && child.attr("scope") == Some("row")))
        })
    });
    let label = header.or_else(|| {
        (row.tag() == "TR"
            && !ancestors.contains(&first_cell)
            && doc
                .node(first_cell)
                .is_some_and(|child| child.tag() == "TD"))
        .then_some(first_cell)
    })?;
    let text = subtree_text(doc, label);
    useful_label(&text, name).then_some(text)
}

fn useful_label(text: &str, name: &str) -> bool {
    !text.is_empty() && text != name
}

#[cfg(test)]
#[path = "redrawn_rows_tests.rs"]
mod tests;
