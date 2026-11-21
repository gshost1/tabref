//! Watches what an action changes on the page. Before the action, one
//! evaluation reads the address, title and rendered text, open shadow roots
//! and same-origin frames included, and starts a `MutationObserver` on the
//! document and each of them; after it, one more reads them again, the
//! observer's count and how many shown frames from other sites it could not
//! read. The observer hangs off a registered symbol of
//! `window`, which page scripts that list the window's keys never see. A
//! document that replaced the watched one has no observer, which is how a
//! navigation shows.

use std::collections::HashSet;

use serde_json::{Value, json};
use tabref_core::{AppError, Deadline};

use super::page_changes::PageChanges;
use super::page_mark::{BlockText, PageMark, TextBlocks};
use super::popup_close::{closed_popups, popups};
use super::text_change::TextChange;
use super::text_diff::{diff_page, text_lines};
use super::text_reorder::{Cells, cells_of};
use super::text_replaced_rows::replaced_rows;
use super::watch_scripts::{COUNT_WATCH, READ_WATCH, SCAN, START_WATCH};
use crate::{CdpConnection, PageSession};

const TEXT_LIMIT: usize = 200_000;

#[cfg(test)]
#[path = "text_replaced_rows_tests.rs"]
mod replacement_tests;

/// Marks the page before an action and starts counting its DOM mutations.
pub fn mark_page(
    connection: &mut CdpConnection,
    session: &PageSession,
    deadline: Deadline,
) -> Result<PageMark, AppError> {
    let reply = run(connection, session, START_WATCH, deadline)?;
    Ok(PageMark {
        url: text_field(&reply, "url"),
        title: text_field(&reply, "title"),
        blocks: text_blocks(&reply),
        popups: popups(&reply),
        tables: reply["tables"].as_array().cloned().unwrap_or_default(),
    })
}

/// The page now against `mark`, and stops the mutation count. Only text
/// blocks that could be read are compared.
pub fn page_changes(
    connection: &mut CdpConnection,
    session: &PageSession,
    mark: &PageMark,
    deadline: Deadline,
) -> Result<PageChanges, AppError> {
    let reply = run(connection, session, READ_WATCH, deadline)?;
    let (changes, unchecked_text_blocks, replaced_rows) =
        compare_watched(&mark.blocks, &mark.popups, &mark.tables, &reply);
    Ok(PageChanges {
        url: text_field(&reply, "url"),
        title: text_field(&reply, "title"),
        mutations: reply.get("mutations").and_then(Value::as_u64),
        unread_frames: reply.get("unread").and_then(Value::as_u64).unwrap_or(0),
        unchecked_text_blocks,
        changes,
        replaced_rows,
    })
}

fn text_blocks(reply: &Value) -> TextBlocks {
    reply["blocks"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|block| {
            let text = match &block[1] {
                Value::String(text) => BlockText::Readable(text.clone()),
                Value::Bool(false) => BlockText::CrossOrigin,
                _ => BlockText::OverBudget,
            };
            Some((block[0].as_u64()?, text))
        })
        .collect()
}

fn compare_blocks(before: &TextBlocks, after: TextBlocks) -> (Vec<TextChange>, usize) {
    if before.is_empty() || after.is_empty() {
        return (Vec::new(), 0);
    }
    let unchecked: HashSet<u64> = before
        .iter()
        .chain(&after)
        .filter_map(|(id, text)| (!matches!(text, BlockText::Readable(_))).then_some(*id))
        .collect();
    let over_budget: HashSet<u64> = before
        .iter()
        .chain(&after)
        .filter_map(|(id, text)| matches!(text, BlockText::OverBudget).then_some(*id))
        .collect();
    let readable = |blocks: &TextBlocks| {
        let mut lines = Vec::new();
        let mut cells = Cells::new();
        for (id, block) in blocks {
            if !unchecked.contains(id) {
                if let BlockText::Readable(text) = block {
                    lines.extend(text_lines(text));
                    cells.extend(cells_of(text));
                }
            }
        }
        (lines, cells)
    };
    let (before, _) = readable(before);
    let (after, cells) = readable(&after);
    (diff_page(&before, &after, &cells), over_budget.len())
}

#[cfg(test)]
fn compare_replies(before: &Value, after: &Value) -> Vec<TextChange> {
    compare_watched(
        &text_blocks(before),
        &popups(before),
        before["tables"].as_array().map_or(&[], Vec::as_slice),
        after,
    )
    .0
}

fn compare_watched(
    before: &TextBlocks,
    popups_before: &[Value],
    tables_before: &[Value],
    after: &Value,
) -> (Vec<TextChange>, usize, Vec<(usize, Vec<String>)>) {
    let mut after_blocks = text_blocks(after);
    if after_blocks.is_empty() {
        let (changes, unchecked) = compare_blocks(before, after_blocks);
        return (changes, unchecked, Vec::new());
    }
    let mut before = before.clone();
    let mut closed = closed_popups(&mut before, &after_blocks, popups_before, &popups(after));
    let (replaced, notices) = replaced_rows(
        &mut before,
        &mut after_blocks,
        tables_before,
        after["tables"].as_array().map_or(&[], Vec::as_slice),
    );
    closed.extend(notices);
    let (changes, unchecked) = compare_blocks(&before, after_blocks);
    closed.extend(changes);
    closed.extend(
        replaced
            .iter()
            .flat_map(|(_, rows)| rows.iter().cloned().map(TextChange::Added)),
    );
    (closed, unchecked, replaced)
}

/// How many DOM mutations the watch has counted so far; `None` when the
/// watched document is gone or was never watched.
pub(crate) fn mutation_count(
    connection: &mut CdpConnection,
    session: &PageSession,
    deadline: Deadline,
) -> Result<Option<u64>, AppError> {
    Ok(run(connection, session, COUNT_WATCH, deadline)?.as_u64())
}

fn run(
    connection: &mut CdpConnection,
    session: &PageSession,
    script: &str,
    deadline: Deadline,
) -> Result<Value, AppError> {
    let expression = script
        .replace("SCAN", SCAN)
        .replace("LIMIT", &TEXT_LIMIT.to_string());
    let mut reply = connection.call(
        "Runtime.evaluate",
        json!({ "expression": expression, "returnByValue": true }),
        Some(&session.session_id),
        deadline,
    )?;
    if reply.get("exceptionDetails").is_some() {
        return Ok(Value::Null);
    }
    Ok(reply
        .pointer_mut("/result/value")
        .map_or(Value::Null, Value::take))
}

fn text_field(reply: &Value, key: &str) -> String {
    reply
        .get(key)
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_owned()
}

#[cfg(test)]
mod tests {
    use super::{TextChange, compare_blocks, json, text_blocks};

    #[test]
    fn a_frame_over_budget_on_either_side_is_unchecked_and_other_changes_survive() {
        let readable = text_blocks(
            &json!({"blocks": [[0, "Status idle"], [1, "Inside frame"], [2, "Unchanged"]]}),
        );
        let oversized =
            text_blocks(&json!({"blocks": [[0, "Status ready"], [2, "Unchanged"], [1, null]]}));
        let (changes, unchecked) = compare_blocks(&readable, oversized.clone());
        assert_eq!(unchecked, 1);
        assert_eq!(
            changes,
            [TextChange::Changed {
                before: "Status idle".into(),
                after: "Status ready".into()
            }]
        );
        let (changes, unchecked) = compare_blocks(&oversized, readable);
        assert_eq!(unchecked, 1);
        assert_eq!(
            changes,
            [TextChange::Changed {
                before: "Status ready".into(),
                after: "Status idle".into()
            }]
        );
    }
}

#[cfg(test)]
#[path = "popup_close_tests.rs"]
mod popup_close_tests;
