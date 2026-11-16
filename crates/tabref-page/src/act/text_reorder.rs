//! A diff's middle that is mostly the same lines in a new order: a list
//! drawn again sorted. The run it reports starts at the list's real first
//! line, backed up over the trimmed lines above that have the same shape,
//! so a first row that kept its place is still shown, and it names the
//! line above. Lines that changed besides the order follow as `-`/`+`.

use std::collections::{HashMap, HashSet};

use super::text_change::TextChange;
use super::text_diff::pair_up;

const SHAPE_SAMPLE: usize = 5;
const MAX_BACKED: usize = 20;

/// How many cells a table drew on each line of the page's text, for lines
/// with two or more.
pub(crate) type Cells = HashMap<String, usize>;

/// The cells per line of rendered text, where a table row's cells are
/// separated by tabs.
pub(crate) fn cells_of(text: &str) -> Cells {
    text.lines()
        .filter(|line| line.contains('\t'))
        .map(|line| {
            let cells = line
                .split('\t')
                .filter(|cell| !cell.trim().is_empty())
                .count();
            (line.split_whitespace().collect::<Vec<_>>().join(" "), cells)
        })
        .filter(|(line, cells)| *cells >= 2 && !line.is_empty())
        .collect()
}

/// The reorder between `before` and `after`, whose first `head` and last
/// `tail` lines are the same, or `None` when the middle is not one.
pub(crate) fn reorder(
    before: &[String],
    after: &[String],
    head: usize,
    tail: usize,
    cells: &Cells,
) -> Option<Vec<TextChange>> {
    let old = &before[head..before.len() - tail];
    let new = &after[head..after.len() - tail];
    let moved = moved_lines(old, new)?;
    let first = head + new.iter().position(|line| moved.contains(line.as_str()))?;
    let end = head + 1 + new.iter().rposition(|line| moved.contains(line.as_str()))?;
    let shapes: Vec<Shape> = after[first..end]
        .iter()
        .take(SHAPE_SAMPLE)
        .map(|line| shape(line, cells))
        .collect();
    let of_run = |line: &&String| shapes.contains(&shape(line, cells));
    let backed = after[..first]
        .iter()
        .rev()
        .take(MAX_BACKED)
        .take_while(of_run)
        .count();
    let onward = after[end..]
        .iter()
        .take(MAX_BACKED)
        .take_while(of_run)
        .count();
    let start = first - backed;
    let run = after[start..end + onward].to_vec();
    let tabled = run.iter().filter(|line| cells.contains_key(*line)).count();
    let mut changes = vec![TextChange::Reordered {
        above: start.checked_sub(1).map(|at| after[at].clone()),
        table: tabled * 2 > run.len(),
        run,
    }];
    let (removed, added) = leftovers(old, new);
    pair_up(&removed, &added, &mut changes);
    Some(changes)
}

/// The lines that changed place among those that occur once on each side
/// and are on both, when those shared lines are at least four in five of
/// the once-lines and changed order: a list drawn again sorted. Shared
/// lines that kept their place at either end (a footer) are left out.
/// Lines repeated on a side ("Add to cart") say nothing about which list
/// it is, so a new page of the same cards is not a reorder.
fn moved_lines(old: &[String], new: &[String]) -> Option<HashSet<String>> {
    let once_old = once(old);
    let once_new = once(new);
    let shared = |side: &[String]| -> Vec<String> {
        side.iter()
            .filter(|line| once_old.contains(line.as_str()) && once_new.contains(line.as_str()))
            .cloned()
            .collect()
    };
    let shared_new = shared(new);
    if shared_new.len() < 3 || shared_new.len() * 5 < once_old.len().max(once_new.len()) * 4 {
        return None;
    }
    let shared_old = shared(old);
    let kept = |pairs: &mut dyn Iterator<Item = (&String, &String)>| {
        pairs.take_while(|(left, right)| left == right).count()
    };
    let front = kept(&mut shared_old.iter().zip(&shared_new));
    if front == shared_new.len() {
        return None;
    }
    let back = kept(&mut shared_old.iter().rev().zip(shared_new.iter().rev()));
    Some(
        shared_new[front..shared_new.len() - back]
            .iter()
            .cloned()
            .collect(),
    )
}

fn once(side: &[String]) -> HashSet<&str> {
    let mut counts: HashMap<&str, usize> = HashMap::new();
    for line in side {
        *counts.entry(line.as_str()).or_default() += 1;
    }
    counts
        .into_iter()
        .filter(|(_, count)| *count == 1)
        .map(|(line, _)| line)
        .collect()
}

/// The lines only one side holds, counting repeats: `old`'s in its order,
/// then `new`'s in its order.
fn leftovers(old: &[String], new: &[String]) -> (Vec<String>, Vec<String>) {
    (only_in(old, new), only_in(new, old))
}

fn only_in(side: &[String], other: &[String]) -> Vec<String> {
    let mut pool: HashMap<&str, usize> = HashMap::new();
    for line in other {
        *pool.entry(line.as_str()).or_default() += 1;
    }
    side.iter()
        .filter(|line| match pool.get_mut(line.as_str()) {
            Some(left) if *left > 0 => {
                *left -= 1;
                false
            }
            _ => true,
        })
        .cloned()
        .collect()
}

#[derive(PartialEq, Eq)]
struct Shape {
    cells: usize,
    words: Vec<bool>,
}

/// A line's shape: its table cells and, per word, whether it holds a digit.
fn shape(line: &str, cells: &Cells) -> Shape {
    Shape {
        cells: cells.get(line).copied().unwrap_or(0),
        words: line
            .split(' ')
            .map(|word| word.chars().any(|c| c.is_ascii_digit()))
            .collect(),
    }
}

#[cfg(test)]
#[path = "text_reorder_tests.rs"]
mod tests;
