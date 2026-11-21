use serde_json::{Value, json};
use tabref_page::{PageChanges, TextChange};

pub(crate) fn other_changes(page: &PageChanges) -> &[TextChange] {
    let rows: usize = page.replaced_rows.iter().map(|(_, rows)| rows.len()).sum();
    &page.changes[..page.changes.len().saturating_sub(rows)]
}

pub(crate) fn preview(page: &PageChanges) -> Vec<String> {
    let mut lines = Vec::new();
    for (old, rows) in &page.replaced_rows {
        lines.push(format!("- {old} rows replaced, {} new", rows.len()));
        for (_, row) in rows.iter().take(5) {
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

pub(crate) fn data(page: &PageChanges) -> Option<Value> {
    if page.replaced_rows.is_empty() {
        return None;
    }
    let old: usize = page.replaced_rows.iter().map(|(old, _)| old).sum();
    let new: usize = page.replaced_rows.iter().map(|(_, rows)| rows.len()).sum();
    let preview: Vec<_> = page
        .replaced_rows
        .iter()
        .flat_map(|(_, rows)| rows)
        .take(50)
        .map(|(id, text)| json!({"id":id,"text":text}))
        .collect();
    Some(json!({"old":old,"new":new,"omitted":new-preview.len(),"preview":preview}))
}
