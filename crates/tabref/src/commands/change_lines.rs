//! How an action report prints changed lines: each line once, with how
//! many times the page showed it.

/// The changed lines as printed, each once with how many times it showed
/// (`+ Required ×2`): a page often shows the same message twice (a summary
/// list and inline under the field). Lines are compared whole, before any
/// cut.
pub(crate) fn distinct_changes(changes: &[tabref_page::TextChange]) -> Vec<String> {
    counted(changes)
        .into_iter()
        .map(|(change, count)| with_count(change.to_string(), count))
        .collect()
}

fn counted(changes: &[tabref_page::TextChange]) -> Vec<(&tabref_page::TextChange, usize)> {
    let mut counts: Vec<(&tabref_page::TextChange, usize)> = Vec::new();
    for change in changes {
        match counts.iter_mut().find(|(seen, _)| *seen == change) {
            Some((_, count)) => *count += 1,
            None => counts.push((change, 1)),
        }
    }
    counts
}

fn with_count(line: String, count: usize) -> String {
    if count > 1 {
        format!("{line} ×{count}")
    } else {
        line
    }
}

/// The first change as a `do` step's line carries it, cut to 100
/// characters, with how many more there were: `+ Saved (+2 more)`.
pub(crate) fn step_change(changes: &[tabref_page::TextChange]) -> Option<String> {
    let counts = counted(changes);
    let (first, count) = counts.first()?;
    let line = with_count(first.brief(), *count);
    if counts.len() > 1 {
        return Some(format!("{line} (+{} more)", counts.len() - 1));
    }
    Some(line)
}
