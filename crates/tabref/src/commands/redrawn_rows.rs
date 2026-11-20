//! The new controls of an action that drew a list again in a new order:
//! the ones inside the reordered lines are the rows' own links and
//! buttons made again, said in one line with the warning that refs taken
//! from those rows before are stale; any other new control (a dialog, an
//! Undo button) is listed as usual.

use std::collections::HashSet;

use tabref_page::{DomTree, ElementToken, TextChange, subtree_text};

pub(super) struct NewControl<'a> {
    pub(super) line: String,
    pub(super) name: Option<&'a str>,
    pub(super) token: &'a str,
}

/// The line for the controls made again inside a reorder, if any, and the
/// lines of the other new controls. Without a reorder every control is
/// listed.
pub(super) fn redrawn<'a>(
    changes: &[TextChange],
    found: &[NewControl<'_>],
    documents: impl Fn(&str) -> Option<&'a [DomTree]>,
) -> (Option<String>, Vec<String>) {
    let Some((run, table)) = changes.iter().find_map(|change| match change {
        TextChange::Reordered { run, table, .. } => Some((run, *table)),
        _ => None,
    }) else {
        return (
            None,
            found.iter().map(|control| control.line.clone()).collect(),
        );
    };
    let run: HashSet<&str> = run.iter().map(String::as_str).collect();
    let mut in_rows = 0;
    let mut outside = Vec::new();
    for control in found {
        if control.name.is_some_and(|name| !name.trim().is_empty())
            && belongs_to_run(control.token, &documents, &run)
        {
            in_rows += 1;
        } else {
            outside.push(control.line.clone());
        }
    }
    let noun = if table { "rows" } else { "lines" };
    let note = (in_rows > 0).then(|| {
        format!(
            "{noun} drawn again: {in_rows} new refs, earlier refs in them are stale: tabref snap"
        )
    });
    (note, outside)
}

/// A matching label alone says nothing about which part of the page owns
/// a control. Require a row or list item in the run to contain the node.
fn belongs_to_run<'a>(
    token: &str,
    documents: impl Fn(&str) -> Option<&'a [DomTree]>,
    run: &HashSet<&str>,
) -> bool {
    let Ok(token) = ElementToken::parse(token) else {
        return false;
    };
    let Some(doc) = documents(&token.target_id)
        .and_then(|docs| docs.iter().find(|doc| doc.frame_id == token.frame_id))
    else {
        return false;
    };
    let Some(index) = doc.find_backend(token.backend_node_id) else {
        return false;
    };
    for ancestor in doc.ancestors(index) {
        let Some(node) = doc.node(ancestor) else {
            break;
        };
        let role = node
            .attr("role")
            .and_then(|role| role.split_whitespace().next());
        if node.tag() == "DIALOG" || matches!(role, Some("dialog" | "alertdialog")) {
            break;
        }
        if matches!(node.tag(), "TR" | "LI")
            || matches!(role, Some("row" | "listitem"))
            || matches!(node.display(), "table-row" | "list-item")
        {
            return run.contains(subtree_text(doc, ancestor).as_str());
        }
    }
    false
}

#[cfg(test)]
#[path = "redrawn_rows_tests.rs"]
mod tests;
