use std::collections::HashMap;
use std::collections::hash_map::Entry;

use serde_json::{Value, json};
use tabref_page::TextChange;

const CHANGE_BUDGET: usize = 600;
const RELOAD_BUDGET: usize = 200;
const MIN_ROOM: usize = 60;
const ELLIPSIS: &str = " …";
const JSON_CHANGE_LIMIT: usize = 50;

#[derive(Clone, Copy)]
pub(super) struct CountedChange<'a> {
    pub(super) change: &'a TextChange,
    pub(super) count: usize,
}

impl CountedChange<'_> {
    fn suffix(&self) -> String {
        if self.count > 1 {
            format!(" ×{}", self.count)
        } else {
            String::new()
        }
    }
}

/// Count whole changes before display truncation and retain their
/// first-seen order.
pub(super) fn counted(changes: &[TextChange]) -> Vec<CountedChange<'_>> {
    let mut positions: HashMap<&TextChange, usize> = HashMap::new();
    let mut counts: Vec<CountedChange<'_>> = Vec::new();
    for change in changes {
        match positions.entry(change) {
            Entry::Occupied(entry) => counts[*entry.get()].count += 1,
            Entry::Vacant(entry) => {
                entry.insert(counts.len());
                counts.push(CountedChange { change, count: 1 });
            }
        }
    }
    counts
}

pub(super) fn change_data(changes: &[CountedChange<'_>]) -> Value {
    let limit = changes.len().min(JSON_CHANGE_LIMIT);
    let mut lines = Vec::with_capacity(limit);
    let mut counts = Vec::with_capacity(limit);
    for change in &changes[..limit] {
        let line = change.change.to_string();
        lines.push(format!("{line}{}", change.suffix()));
        counts.push(json!({ "line": line, "count": change.count }));
    }
    json!({
        "changes": lines,
        "change_counts": counts,
        "distinct_changes_omitted": changes.len() - limit,
    })
}

/// Show the text lines that fit the byte budget. A cut keeps the repeat
/// count, and a rewritten line is shown whole or counted as omitted.
pub(super) fn within_budget(changes: &[CountedChange<'_>]) -> Vec<String> {
    let mut shown = preview(changes, CHANGE_BUDGET);
    if shown.len() < changes.len() {
        shown.push(format!(
            "+{} more: tabref snap",
            changes.len() - shown.len()
        ));
    }
    shown
}

/// A reload's preview shows only added lines. JSON still includes other changes.
pub(super) fn reload_head(changes: &[CountedChange<'_>]) -> Vec<String> {
    let added: Vec<_> = changes
        .iter()
        .copied()
        .filter(|entry| matches!(entry.change, TextChange::Added(_)))
        .collect();
    let mut shown = preview(&added, RELOAD_BUDGET);
    if shown.len() < added.len() {
        shown.push(format!(
            "+{} more added lines, in the view below",
            added.len() - shown.len()
        ));
    }
    shown
}

fn preview(changes: &[CountedChange<'_>], mut room: usize) -> Vec<String> {
    let mut shown = Vec::new();
    for change in changes {
        let text = change.change.to_string();
        let suffix = change.suffix();
        if text.len() + suffix.len() <= room {
            room -= text.len() + suffix.len();
            shown.push(format!("{text}{suffix}"));
            continue;
        }
        if room >= MIN_ROOM && !matches!(change.change, TextChange::Changed { .. }) {
            let text = cut_to_bytes(&text, room - suffix.len());
            shown.push(format!("{text}{suffix}"));
        }
        break;
    }
    shown
}

/// Cut after the last whole word if one ends in the latter half of the
/// available space, otherwise cut mid-word at a character boundary.
fn cut_to_bytes(text: &str, room: usize) -> String {
    let limit = room - ELLIPSIS.len();
    let mut end = limit.min(text.len());
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    let kept = &text[..end];
    match kept.rfind(' ') {
        Some(space) if space >= limit / 2 => format!("{}{ELLIPSIS}", kept[..space].trim_end()),
        _ => format!("{kept}{ELLIPSIS}"),
    }
}

/// The first change as a `do` step's line carries it, with the count and
/// how many distinct changes followed.
pub(super) fn step_change(changes: &[TextChange]) -> Option<String> {
    let counts = counted(changes);
    let first = counts.first()?;
    let line = format!("{}{}", first.change.brief(), first.suffix());
    if counts.len() > 1 {
        return Some(format!("{line} (+{} more)", counts.len() - 1));
    }
    Some(line)
}
