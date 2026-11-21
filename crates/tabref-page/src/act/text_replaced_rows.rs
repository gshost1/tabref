use std::collections::{HashMap, HashSet};

use serde_json::Value;

use super::page_changes::ReplacedRows;
use super::page_mark::{BlockText, TextBlocks};
use super::text_change::TextChange;
use super::text_diff::{diff_page, text_lines};
use super::text_reorder::Cells;

pub(super) fn replaced_rows(
    before: &mut TextBlocks,
    after: &mut TextBlocks,
    old: &[Value],
    new: &[Value],
) -> (ReplacedRows, Vec<TextChange>) {
    let mut found = Vec::new();
    let mut notices_changed = Vec::new();
    for body in old {
        if body["watch"].as_str().is_none() {
            continue;
        }
        let mut matches = new.iter().filter(|other| {
            ["watch", "block", "table", "body"]
                .iter()
                .all(|key| body[key] == other[key])
        });
        let Some(other) = matches.next() else {
            continue;
        };
        if matches.next().is_some() {
            continue;
        }
        let Some(block) = body["block"].as_u64() else {
            continue;
        };
        if body["table"].as_u64().is_none() || body["body"].as_u64().is_none() {
            continue;
        }
        if body["body_id"].as_u64().is_none()
            || other["body_id"].as_u64().is_none()
            || old.iter().any(|candidate| {
                candidate["watch"] == body["watch"]
                    && candidate["block"] == body["block"]
                    && candidate["table"] == body["table"]
                    && candidate["body_id"] == other["body_id"]
                    && candidate["body_id"] != body["body_id"]
            })
            || new.iter().any(|candidate| {
                candidate["watch"] == body["watch"]
                    && candidate["block"] == body["block"]
                    && candidate["table"] == body["table"]
                    && candidate["body_id"] == body["body_id"]
                    && candidate["body"] != body["body"]
            })
        {
            continue;
        }
        let (Some(old_rows), Some(new_rows)) = (rows(body), rows(other)) else {
            continue;
        };
        let ids: HashSet<_> = new_rows.iter().map(|(id, _)| id).collect();
        let texts: HashSet<_> = new_rows.iter().map(|(_, text)| text).collect();
        if old_rows.is_empty()
            || mostly_edits(body, other, old_rows.len(), new_rows.len())
            || old_rows
                .iter()
                .any(|(id, text)| ids.contains(id) || texts.contains(text))
        {
            continue;
        }
        let Some((_, BlockText::Readable(before_text))) =
            before.iter_mut().find(|(id, _)| *id == block)
        else {
            continue;
        };
        let Some((_, BlockText::Readable(after_text))) =
            after.iter_mut().find(|(id, _)| *id == block)
        else {
            continue;
        };
        let old_lines: Vec<_> = old_rows
            .iter()
            .flat_map(|(_, lines)| lines.iter().cloned())
            .collect();
        let new_lines: Vec<_> = new_rows
            .iter()
            .flat_map(|(_, lines)| lines.iter().cloned())
            .collect();
        if old_lines.is_empty() {
            continue;
        }
        let (Some(left), Some(right)) = (
            without_rows(before_text, &old_lines),
            without_rows(after_text, &new_lines),
        ) else {
            continue;
        };
        *before_text = left;
        *after_text = right;
        notices_changed.extend(diff_page(&notices(body), &notices(other), &Cells::new()));
        found.push((
            old_rows.len(),
            new_rows
                .into_iter()
                .map(|(id, lines)| (id, lines.join(" ")))
                .collect(),
        ));
    }
    (found, notices_changed)
}

fn mostly_edits(before: &Value, after: &Value, old: usize, new: usize) -> bool {
    let left = keys(before);
    let right = keys(after);
    let pairs: Vec<_> = left
        .iter()
        .flat_map(|(key, ids)| ids.iter().zip(right.get(key).into_iter().flatten()))
        .collect();
    let old_ids: HashSet<_> = pairs.iter().map(|(id, _)| id).collect();
    let new_ids: HashSet<_> = pairs.iter().map(|(_, id)| id).collect();
    old_ids.len() * 2 > old && new_ids.len() * 2 > new
}

fn keys(body: &Value) -> HashMap<(usize, &str), Vec<u64>> {
    let mut keys: HashMap<_, Vec<_>> = HashMap::new();
    for row in body["rows"].as_array().into_iter().flatten() {
        for column in [2, 3] {
            let key = row[column]
                .as_str()
                .filter(|text| !text.trim().is_empty())
                .or_else(|| {
                    if column == 2 {
                        row[1]
                            .as_str()?
                            .split('\t')
                            .map(str::trim)
                            .find(|text| !text.is_empty())
                    } else {
                        None
                    }
                });
            if let Some((key, id)) = key.zip(row[0].as_u64()) {
                keys.entry((column, key)).or_default().push(id);
            }
        }
    }
    keys
}

fn notices(body: &Value) -> Vec<String> {
    body["notices"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .flat_map(text_lines)
        .collect()
}

fn rows(body: &Value) -> Option<Vec<(u64, Vec<String>)>> {
    body["rows"]
        .as_array()?
        .iter()
        .map(|row| Some((row[0].as_u64()?, text_lines(row[1].as_str()?))))
        .collect()
}

fn without_rows(text: &str, rows: &[String]) -> Option<String> {
    if rows.is_empty() {
        return Some(text.to_owned());
    }
    let mut lines = text_lines(text);
    let mut matches = lines
        .windows(rows.len())
        .enumerate()
        .filter(|(_, run)| *run == rows);
    let (start, _) = matches.next()?;
    if matches.next().is_some() {
        return None;
    }
    lines.drain(start..start + rows.len());
    Some(lines.join("\n"))
}
