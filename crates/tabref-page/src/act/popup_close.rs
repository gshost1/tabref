use std::collections::HashSet;

use serde_json::Value;

use super::page_mark::{BlockText, TextBlocks};
use super::text_change::TextChange;
use super::text_diff::text_lines;

pub(super) fn popups(reply: &Value) -> Vec<Value> {
    reply["popups"].as_array().cloned().unwrap_or_default()
}

pub(super) fn closed_popups(
    blocks: &mut TextBlocks,
    after_blocks: &TextBlocks,
    before: &[Value],
    after: &[Value],
) -> Vec<TextChange> {
    let mut closed = Vec::new();
    for popup in before {
        if after.iter().any(|other| same_popup(popup, other)) {
            continue;
        }
        let Some(block) = popup["block"].as_u64() else {
            continue;
        };
        if after_blocks
            .iter()
            .any(|(id, text)| *id == block && !matches!(text, BlockText::Readable(_)))
        {
            continue;
        }
        let Some((_, BlockText::Readable(text))) = blocks.iter_mut().find(|(id, _)| *id == block)
        else {
            continue;
        };
        let role = popup["role"].as_str().unwrap_or_default();
        let dialog = matches!(role, "dialog" | "alertdialog");
        if !dialog && !matches!(role, "listbox" | "menu") {
            continue;
        }
        let Some(item_text) = popup["text"].as_str() else {
            continue;
        };
        let items = text_lines(item_text);
        if items.is_empty() && !dialog {
            continue;
        }
        if !items.is_empty() {
            let mut lines = text_lines(text);
            let Some(start) = lines.windows(items.len()).position(|run| run == items) else {
                continue;
            };
            lines.drain(start..start + items.len());
            *text = lines.join("\n");
        }
        let name = popup["name"].as_str().unwrap_or_default();
        let name = if name.is_empty() {
            String::new()
        } else {
            format!(" \"{}\"", crate::escape_line(name, true))
        };
        if dialog {
            closed.push(TextChange::Removed(format!("{role}{name} closed")));
            continue;
        }
        let count = popup["count"].as_u64().unwrap_or(0);
        let noun = if role == "listbox" {
            "options"
        } else {
            "items"
        };
        closed.push(TextChange::Removed(format!(
            "{role}{name} closed ({count} {noun})"
        )));
    }
    closed
}

fn same_popup(before: &Value, after: &Value) -> bool {
    if before["role"] != after["role"] {
        return false;
    }
    let identity = before["watch"].as_str().is_some()
        && before["watch"] == after["watch"]
        && before["id"] == after["id"];
    let position = before["position"].as_str().is_some()
        && before["position"] == after["position"]
        && before["block"] == after["block"];
    let name = before["name"].as_str().unwrap_or_default();
    let items = before["text"].as_str().is_some() && before["text"] == after["text"];
    let count = before["count"].as_u64().is_some() && before["count"] == after["count"];
    let shared = position
        && before["text"]
            .as_str()
            .zip(after["text"].as_str())
            .is_some_and(|(old, new)| {
                let remaining: HashSet<_> = text_lines(new).into_iter().collect();
                text_lines(old).iter().any(|line| remaining.contains(line))
            });
    identity
        || shared
        || (position && before["name"] == after["name"] && (items || (!name.is_empty() && count)))
        || (before["block"] == after["block"] && items)
        || (matches!(before["role"].as_str(), Some("dialog" | "alertdialog"))
            && !name.is_empty()
            && before["name"] == after["name"]
            && before["block"] == after["block"])
}
