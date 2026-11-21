use tabref_page::{PageChanges, TextChange};

pub(crate) fn other_changes(page: &PageChanges) -> &[TextChange] {
    let rows: usize = page.replaced_rows.iter().map(|(_, rows)| rows.len()).sum();
    &page.changes[..page.changes.len().saturating_sub(rows)]
}

pub(crate) fn changes(page: &PageChanges) -> Vec<TextChange> {
    let mut changes: Vec<_> = page
        .replaced_rows
        .iter()
        .map(|(old, rows)| TextChange::Removed(format!("{old} rows replaced, {} new", rows.len())))
        .collect();
    changes.extend(page.changes.iter().cloned());
    changes
}

pub(crate) fn preview(page: &PageChanges) -> Vec<String> {
    let mut lines = Vec::new();
    for (old, rows) in &page.replaced_rows {
        lines.push(format!("- {old} rows replaced, {} new", rows.len()));
        for row in rows.iter().take(5) {
            let mut chars = row.chars();
            let mut text: String = chars.by_ref().take(120).collect();
            if chars.next().is_some() {
                text.push('…');
            }
            lines.push(format!("  {text}"));
        }
        if rows.len() > 5 {
            lines.push(format!("  (+{} more: tabref snap)", rows.len() - 5));
        }
    }
    lines
}

pub(crate) fn omitted<'a>(
    page: &PageChanges,
    shown: impl Iterator<Item = &'a TextChange>,
) -> usize {
    let shown: Vec<_> = shown.collect();
    page.replaced_rows
        .iter()
        .flat_map(|(_, rows)| rows)
        .filter(|row| {
            !shown
                .iter()
                .any(|change| matches!(change, TextChange::Added(line) if line == *row))
        })
        .count()
}
