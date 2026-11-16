//! The new controls of an action that drew a list again in a new order:
//! the ones inside the reordered lines are the rows' own links and
//! buttons made again, said in one line with the warning that refs taken
//! from those rows before are stale; any other new control (a dialog, an
//! Undo button) is listed as usual.

use tabref_page::TextChange;

/// The line for the controls made again inside a reorder, if any, and the
/// lines of the other new controls. Without a reorder every control is
/// listed.
pub(crate) fn redrawn(
    changes: &[TextChange],
    found: &[(String, Option<String>)],
) -> (Option<String>, Vec<String>) {
    let Some((run, table)) = changes.iter().find_map(|change| match change {
        TextChange::Reordered { run, table, .. } => Some((run, *table)),
        _ => None,
    }) else {
        return (None, found.iter().map(|(line, _)| line.clone()).collect());
    };
    let mut in_rows = 0;
    let mut outside = Vec::new();
    for (line, name) in found {
        let name = name
            .as_deref()
            .map(|name| name.split_whitespace().collect::<Vec<_>>().join(" "))
            .unwrap_or_default();
        if name.is_empty() || run.iter().any(|row| row.contains(&name)) {
            in_rows += 1;
        } else {
            outside.push(line.clone());
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

#[cfg(test)]
#[path = "redrawn_rows_tests.rs"]
mod tests;
