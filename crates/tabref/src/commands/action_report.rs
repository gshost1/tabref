use std::collections::HashSet;

use serde_json::{Value, json};
use tabref_core::{AccessibilityNode, AppError};

use super::acted::Acted;
use super::change_lines::{distinct_changes, printed_names, step_change, without_added};
use super::download_lines::{download_json, download_line};
use super::page_url::short_url;
use super::page_view::current_page_view;
use super::redrawn_rows::redrawn;
use crate::output::outcome::Outcome;
use crate::session::Session;
use crate::session::action_effects::ActionEffects;
use crate::view::node_line::{node_line, quote_text};
use crate::view::ref_style::RefStyle;

const CHANGE_BUDGET: usize = 600;
const RELOAD_BUDGET: usize = 200;
const NOTHING_SEEN: &str = " (no visible change)";
const NOTHING_READ: &str = " (no visible change; a frame from another site was not checked)";
const NEW_SHOWN: usize = 8;
const NEW_LINE_CHARS: usize = 80;
const MIN_ROOM: usize = 60;
const ELLIPSIS: &str = " …";

/// An action as a single command prints it: `clicked "Save"`, then what it
/// set off, capped near 150 tokens: dialogs, the new address, the page's
/// changed lines, the controls that appeared with their refs, or, when it
/// led to another page, a snapshot of that page. A reload of the same
/// address prints its changed lines and then the snapshot, since every
/// ref is new and a form sent back with errors shows what it kept.
pub(crate) fn full_report(session: &mut Session, acted: Acted) -> Result<Outcome, AppError> {
    let mut first = format!("{} {}", past(acted.verb), acted.summary);
    if let Some(note) = nothing_seen(&acted) {
        first.push_str(note);
    }
    let mut outcome = Outcome::new(first, acted.data);
    if let Some(effects) = &acted.effects {
        let (lines, data) = effect_block(session, effects, &acted.known)?;
        for line in lines {
            outcome.push_line(&line);
        }
        merge(&mut outcome.data, data);
    }
    Ok(outcome)
}

/// A `do` step's line: `ok fill "Price ($)" "67.00" -> "62.00"`, with
/// where a navigation led and the first change on the same line. The last
/// step of a batch adds the full block a single command prints.
pub(crate) fn step_report(
    session: &mut Session,
    acted: Acted,
    last: bool,
) -> Result<(String, Value), AppError> {
    let mut line = format!("ok {} {}", acted.verb, acted.summary);
    if let Some(note) = nothing_seen(&acted) {
        line.push_str(note);
    }
    let mut data = acted.data;
    let Some(effects) = &acted.effects else {
        return Ok((line, data));
    };
    if last {
        let (lines, more) = effect_block(session, effects, &acted.known)?;
        for extra in lines {
            line.push('\n');
            line.push_str(&extra);
        }
        merge(&mut data, more);
        return Ok((line, data));
    }
    for dialog in dialog_lines(effects.dialogs()) {
        line.push_str(&format!("; {dialog}"));
    }
    for download in &effects.downloads {
        line.push_str(&format!("; {}", download_line(download)));
    }
    if effects.new_tab.is_some() {
        line.push_str(" -> a new tab, now the current tab");
    } else if let Some(page) = &effects.page {
        if let Some(url) = effects.moved_to() {
            line.push_str(&format!(" -> {}", short_url(&effects.url_before, url)));
        }
        if effects.settled.navigated {
            line.push_str(&format!(" {}", quote_text(&page.title)));
        }
        let leaves = effects.settled.navigated && effects.url_before != page.url;
        if !leaves && let Some(change) = step_change(&page.changes) {
            line.push_str(&format!("; {change}"));
        }
    }
    merge(&mut data, json!({ "navigated": effects.settled.navigated }));
    Ok((line, data))
}

/// The lines after an action's first line, and their `--json` data.
fn effect_block(
    session: &mut Session,
    effects: &ActionEffects,
    known: &HashSet<String>,
) -> Result<(Vec<String>, Value), AppError> {
    let mut lines: Vec<String> = dialog_lines(effects.dialogs());
    lines.extend(effects.downloads.iter().map(download_line));
    let mut data = json!({
        "dialogs": effects.dialogs(),
        "navigated": effects.settled.navigated,
    });
    if !effects.downloads.is_empty() {
        let downloads: Vec<Value> = effects.downloads.iter().map(download_json).collect();
        merge(&mut data, json!({ "downloads": downloads }));
    }
    if let Some(tab) = &effects.new_tab {
        lines.push("opened a new tab, now the current tab".to_owned());
        let report = current_page_view(session)?;
        lines.push(report.text);
        merge(&mut data, json!({ "new_tab": tab, "page": report.data }));
        return Ok((lines, data));
    }
    let Some(page) = &effects.page else {
        return Ok((lines, data));
    };
    merge(&mut data, json!({ "url": page.url, "title": page.title }));
    if effects.settled.navigated && effects.url_before != page.url {
        let report = current_page_view(session)?;
        lines.push(report.text);
        merge(&mut data, json!({ "page": report.data }));
        return Ok((lines, data));
    }
    if let Some(url) = effects.moved_to() {
        lines.push(format!("-> {}", short_url(&effects.url_before, url)));
    }
    let mut new_lines = Vec::new();
    let mut named = Vec::new();
    if effects.settled.navigated {
        let report = current_page_view(session)?;
        new_lines.push(report.text);
        merge(&mut data, json!({ "page": report.data }));
    } else if page.changed_anything() || effects.settled.same_document {
        let captured = session.capture()?;
        let mut found = Vec::new();
        new_controls(&captured.tree, known, &mut found);
        let (in_rows, outside) = redrawn(&page.changes, &found);
        let shown = outside.len().min(NEW_SHOWN);
        named = printed_names(&found, &outside[..shown]);
        new_lines.extend(outside.iter().take(shown).map(|line| format!("new {line}")));
        if outside.len() > shown {
            new_lines.push(format!("+{} more new: tabref snap", outside.len() - shown));
        }
        new_lines.extend(in_rows);
        let appeared: Vec<&String> = found.iter().map(|(line, _)| line).collect();
        merge(&mut data, json!({ "new": appeared }));
    }
    let changes = distinct_changes(&page.changes);
    merge(&mut data, json!({ "changes": changes }));
    if effects.settled.navigated {
        lines.extend(reload_head(&page.changes));
    } else {
        let printed = distinct_changes(&without_added(&page.changes, &named));
        lines.extend(within_budget(&printed));
    }
    lines.extend(new_lines);
    Ok((lines, data))
}

/// What a click, double-click, key press, hover or focus that showed no
/// effect at all says: no navigation, tab, dialog, download, DOM mutation or
/// text change in the document, its open shadow roots or its same-origin
/// frames. A shown frame from another site may hold the change, so that is
/// said; a change inside a closed shadow root or drawn on a canvas looks like
/// nothing. A focus the page moved away already says so. `None` when the
/// action showed an effect.
fn nothing_seen(acted: &Acted) -> Option<&'static str> {
    if acted.data.get("focused") == Some(&Value::Bool(false)) {
        return None;
    }
    let effects = acted.effects.as_ref()?;
    let quiet = matches!(
        acted.verb,
        "click" | "double-click" | "press" | "hover" | "focus"
    ) && !effects.settled.navigated
        && !effects.settled.same_document
        && effects.new_tab.is_none()
        && effects.dialogs().is_empty()
        && effects.downloads.is_empty();
    let page = effects.page.as_ref()?;
    if !quiet || page.mutations != Some(0) || !page.changes.is_empty() {
        return None;
    }
    Some(if page.unread_frames > 0 {
        NOTHING_READ
    } else {
        NOTHING_SEEN
    })
}

/// The added lines a same-address reload prints before the view, about
/// 200 bytes of them: they say what the reload brought (an error, a flash
/// message), while the view holds them all again and the removed lines are
/// only the old page.
fn reload_head(changes: &[tabref_page::TextChange]) -> Vec<String> {
    let added: Vec<tabref_page::TextChange> = changes
        .iter()
        .filter(|change| matches!(change, tabref_page::TextChange::Added(_)))
        .cloned()
        .collect();
    fit_lines(
        &distinct_changes(&added),
        RELOAD_BUDGET,
        "+{} more added lines, in the view below",
    )
}

/// The changed lines that fit 600 bytes, the rest counted with a pointer
/// to `snap`.
fn within_budget(changes: &[String]) -> Vec<String> {
    fit_lines(changes, CHANGE_BUDGET, "+{} more: tabref snap")
}

/// The lines that fit `budget` bytes, in order: one that does not fit
/// whole is cut to the room left when that is 60 bytes or more, the first
/// line too, and the rest are counted with `more` (`{}` is the count).
fn fit_lines(changes: &[String], budget: usize, more: &str) -> Vec<String> {
    let mut used = 0;
    let mut shown = Vec::new();
    for change in changes {
        let room = budget - used;
        if change.len() <= room {
            used += change.len();
            shown.push(change.clone());
            continue;
        }
        if room >= MIN_ROOM {
            shown.push(cut_to_bytes(change, room));
        }
        break;
    }
    if shown.len() < changes.len() {
        shown.push(more.replace("{}", &(changes.len() - shown.len()).to_string()));
    }
    shown
}

/// `text` within `room` bytes, ending in `…`: after its last whole word
/// when one ends in the second half of the room, else mid-word.
fn cut_to_bytes(text: &str, room: usize) -> String {
    let limit = room.saturating_sub(ELLIPSIS.len());
    let mut end = 0;
    for (at, character) in text.char_indices() {
        if at + character.len_utf8() > limit {
            break;
        }
        end = at + character.len_utf8();
    }
    let kept = &text[..end];
    match kept.rfind(' ') {
        Some(space) if space >= limit / 2 => format!("{}{ELLIPSIS}", kept[..space].trim_end()),
        _ => format!("{kept}{ELLIPSIS}"),
    }
}

fn new_controls(
    node: &AccessibilityNode,
    known: &HashSet<String>,
    found: &mut Vec<(String, Option<String>)>,
) {
    if node.ref_id.is_some()
        && node
            .identity
            .retained_object
            .as_ref()
            .is_some_and(|token| !known.contains(token))
    {
        found.push((
            styled(&node_line(node, NEW_LINE_CHARS)),
            node.identity.name.clone(),
        ));
    }
    for child in &node.children {
        new_controls(child, known, found);
    }
}

const MAX_DIALOG_LINES: usize = 10;

/// One dialog as a line: its type, its message when it has one, and how
/// it was answered.
pub(crate) fn dialog_line(dialog: &Value) -> String {
    let kind = dialog
        .get("type")
        .and_then(Value::as_str)
        .unwrap_or("dialog");
    let message = dialog
        .get("message")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let mut line = if message.is_empty() {
        format!("dialog {kind} {}", answered(dialog))
    } else {
        format!("dialog {kind} {} {}", quote_text(message), answered(dialog))
    };
    if dialog["late"] == Value::Bool(true) {
        line.push_str(" (opened after the last command)");
    }
    if dialog["other_tab"] == Value::Bool(true) {
        line.push_str(" (in another tab)");
    }
    line
}

/// The dialogs as lines, in the order they opened: the same line repeated
/// back to back is one line with `×N`, and past `MAX_DIALOG_LINES` the
/// rest is counted.
pub(crate) fn dialog_lines(dialogs: &[Value]) -> Vec<String> {
    let mut counted: Vec<(String, usize)> = Vec::new();
    for dialog in dialogs {
        let line = dialog_line(dialog);
        match counted.last_mut() {
            Some((last, count)) if *last == line => *count += 1,
            _ => counted.push((line, 1)),
        }
    }
    let more = counted.len().saturating_sub(MAX_DIALOG_LINES);
    let mut lines: Vec<String> = counted
        .into_iter()
        .take(MAX_DIALOG_LINES)
        .map(|(line, count)| {
            if count > 1 {
                format!("{line} \u{d7}{count}")
            } else {
                line
            }
        })
        .collect();
    match more {
        0 => {}
        1 => lines.push("1 more dialog".to_owned()),
        more => lines.push(format!("{more} more dialogs")),
    }
    lines
}

/// How the connection answered a dialog, from its `answer`: "dismissed",
/// "answered "TEXT"" for a prompt, or "accepted".
pub(crate) fn answered(dialog: &Value) -> String {
    let answer = &dialog["answer"];
    if answer["accept"] == Value::Bool(false) {
        return "dismissed".to_owned();
    }
    match answer["text"].as_str() {
        Some(text) => format!("answered {}", quote_text(text)),
        None => "accepted".to_owned(),
    }
}

fn past(verb: &str) -> String {
    if verb.ends_with('e') {
        format!("{verb}d")
    } else {
        format!("{verb}ed")
    }
}

/// Adds the keys of `more` to the object `data`.
pub(crate) fn merge(data: &mut Value, more: Value) {
    if let (Some(object), Value::Object(more)) = (data.as_object_mut(), more) {
        object.extend(more);
    }
}

/// Sets `key` on the object `data`.
pub(crate) fn set(data: &mut Value, key: &str, value: Value) {
    if let Some(object) = data.as_object_mut() {
        object.insert(key.to_owned(), value);
    }
}

/// A node line with its leading ref printed in the agent's ref style, so
/// `@s8f3k2p9:e31 textfield` reads `e31 textfield` under short refs.
pub(crate) fn styled(line: &str) -> String {
    match line.split_once(' ') {
        Some((ref_id, rest)) if ref_id.starts_with('@') => {
            format!("{} {rest}", RefStyle::from_env().print(ref_id))
        }
        _ => line.to_owned(),
    }
}

#[cfg(test)]
#[path = "action_report_tests.rs"]
mod tests;
