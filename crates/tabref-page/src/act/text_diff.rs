//! A line diff of a page's rendered text before and after an action. Lines
//! the two sides share at the start and the end are trimmed first, since an
//! action usually changes one spot; the middle is aligned by longest common
//! subsequence when it is small enough, and otherwise reported whole. A
//! middle that is mostly the same lines in a new order, too many moves for
//! a few `-`/`+` lines to show, is one reorder (see `text_reorder`).

use super::text_change::TextChange;
use super::text_reorder::{Cells, reorder};

const MAX_TABLE_CELLS: usize = 1_000_000;
const PLAIN_CHANGES: usize = 4;

/// The page's rendered text as diff lines: each line trimmed, runs of
/// whitespace collapsed to one space, and blank lines dropped. A control
/// character that is not whitespace (a terminal escape, a bell) becomes
/// `\uXXXX` and bidirectional marks are dropped, as `quote_text` prints
/// them, so page text cannot drive the terminal the result is read in.
pub(crate) fn text_lines(text: &str) -> Vec<String> {
    text.lines()
        .map(|line| printable(&line.split_whitespace().collect::<Vec<_>>().join(" ")))
        .filter(|line| !line.is_empty())
        .collect()
}

fn printable(line: &str) -> String {
    if !line.chars().any(|c| c.is_control() || is_bidirectional(c)) {
        return line.to_owned();
    }
    let mut kept = String::with_capacity(line.len() + 8);
    for character in line.chars().filter(|c| !is_bidirectional(*c)) {
        if character.is_control() {
            kept.push_str(&format!("\\u{:04x}", u32::from(character)));
        } else {
            kept.push(character);
        }
    }
    kept
}

fn is_bidirectional(character: char) -> bool {
    matches!(
        character,
        '\u{061C}' | '\u{200E}' | '\u{200F}' | '\u{202A}'..='\u{202E}' | '\u{2066}'..='\u{2069}'
    )
}

#[cfg(test)]
pub(crate) fn diff_lines(before: &[String], after: &[String]) -> Vec<TextChange> {
    diff_page(before, after, &Cells::new())
}

/// The changes that turn `before` into `after`, in page order, with the
/// table cells of `after`'s lines. A removed line followed by an added one
/// that shares its first or last word is one line rewritten in place.
pub(crate) fn diff_page(before: &[String], after: &[String], cells: &Cells) -> Vec<TextChange> {
    let head = before
        .iter()
        .zip(after)
        .take_while(|(left, right)| left == right)
        .count();
    let room = before.len().min(after.len()) - head;
    let tail = before
        .iter()
        .rev()
        .zip(after.iter().rev())
        .take(room)
        .take_while(|(left, right)| left == right)
        .count();
    let old = &before[head..before.len() - tail];
    let new = &after[head..after.len() - tail];
    let mut changes = Vec::new();
    for (removed, added) in hunks(old, new) {
        pair_up(removed, added, &mut changes);
    }
    if changes.len() > PLAIN_CHANGES
        && let Some(reordered) = reorder(before, after, head, tail, cells)
    {
        return reordered;
    }
    changes
}
fn hunks<'a>(old: &'a [String], new: &'a [String]) -> Vec<(&'a [String], &'a [String])> {
    if old.is_empty() || new.is_empty() || old.len().saturating_mul(new.len()) > MAX_TABLE_CELLS {
        return vec![(old, new)];
    }
    let width = new.len() + 1;
    let mut table = vec![0_u32; (old.len() + 1) * width];
    for i in (0..old.len()).rev() {
        for j in (0..new.len()).rev() {
            table[i * width + j] = if old[i] == new[j] {
                table[(i + 1) * width + j + 1] + 1
            } else {
                table[(i + 1) * width + j].max(table[i * width + j + 1])
            };
        }
    }
    let mut found = Vec::new();
    let (mut i, mut j) = (0, 0);
    let (mut start_i, mut start_j) = (0, 0);
    while i < old.len() && j < new.len() {
        if old[i] == new[j] {
            if start_i < i || start_j < j {
                found.push((&old[start_i..i], &new[start_j..j]));
            }
            i += 1;
            j += 1;
            start_i = i;
            start_j = j;
        } else if table[(i + 1) * width + j] >= table[i * width + j + 1] {
            i += 1;
        } else {
            j += 1;
        }
    }
    if start_i < old.len() || start_j < new.len() {
        found.push((&old[start_i..], &new[start_j..]));
    }
    found
}

pub(super) fn pair_up(removed: &[String], added: &[String], changes: &mut Vec<TextChange>) {
    if let ([before], [after]) = (removed, added) {
        changes.push(TextChange::Changed {
            before: before.clone(),
            after: after.clone(),
        });
        return;
    }
    let mut added_rest = added;
    for line in removed {
        match added_rest.first() {
            Some(next) if similar(line, next) => {
                changes.push(TextChange::Changed {
                    before: line.clone(),
                    after: next.clone(),
                });
                added_rest = &added_rest[1..];
            }
            _ => changes.push(TextChange::Removed(line.clone())),
        }
    }
    changes.extend(added_rest.iter().cloned().map(TextChange::Added));
}

fn similar(left: &str, right: &str) -> bool {
    let first = |text: &str| text.split(' ').next().map(str::to_owned);
    let last = |text: &str| text.rsplit(' ').next().map(str::to_owned);
    first(left) == first(right) || last(left) == last(right)
}

#[cfg(test)]
#[path = "text_diff_tests.rs"]
mod tests;
