use std::collections::HashSet;

use serde_json::{Value, json};
use tabref_core::AppError;

use super::acted::Acted;
use super::action_report_changes::{change_data, counted, reload_head, step_change, within_budget};
use super::download_lines::{download_json, download_line};
use super::page_url::short_url;
use super::page_view::current_page_view;
use super::redrawn_rows::control_report;
use crate::output::outcome::Outcome;
use crate::session::Session;
use crate::session::action_effects::ActionEffects;
use crate::session::captured_snapshot::CapturedSnapshot;
use crate::view::bot_check::action_bot_check;
use crate::view::node_line::quote_text;
use crate::view::ref_style::RefStyle;
use crate::view::rows_replaced;

const NOTHING_SEEN: &str = " (no visible change)";
const NOTHING_READ: &str = " (no visible change; a frame from another site was not checked)";

/// An action as a single command prints it: `clicked "Save"`, then what it
/// set off, capped near 150 tokens: dialogs, the new address, the page's
/// changed lines, the controls that appeared with their refs, or, when it
/// led to another page, a snapshot of that page.
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
/// step and row replacements add the full block a single command prints.
/// Other steps include a bounded preview of newly available controls.
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
    if last
        || effects
            .page
            .as_ref()
            .is_some_and(|page| !page.replaced_rows.is_empty())
    {
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
        let captured = capture_control_changes(session, effects, page)?;
        let report = captured
            .as_ref()
            .map(|snapshot| control_report(snapshot, &acted.known, page));
        if effects.settled.navigated {
            line.push_str(&format!(" {}", quote_text(&page.title)));
            if effects.url_before == page.url {
                line.push_str("; page reloaded");
            }
        } else if let Some(change) = step_change(
            report
                .as_ref()
                .map_or(&page.changes, |report| &report.changes),
        ) {
            line.push_str(&format!("; {change}"));
        }
        if let Some(note) = text_check_note(page) {
            line.push_str(&format!("; {note}"));
        }
        merge(&mut data, text_check_data(page));
        if let Some(report) = report
            && !report.lines.is_empty()
        {
            for extra in report.lines {
                line.push('\n');
                line.push_str(&extra);
            }
            set(&mut data, "new", json!(report.new));
        }
        if let Some(check) = action_bot_check(session, &page.title, captured.as_ref()) {
            line.push('\n');
            line.push_str(&check.note());
            check.add_data(&mut data);
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
    merge(&mut data, text_check_data(page));
    if effects.settled.navigated && effects.url_before != page.url {
        let report = current_page_view(session)?;
        lines.push(report.text);
        merge(&mut data, json!({ "page": report.data }));
        return Ok((lines, data));
    }
    if let Some(url) = effects.moved_to() {
        lines.push(format!("-> {}", short_url(&effects.url_before, url)));
    }
    let captured = capture_control_changes(session, effects, page)?;
    let prepared = captured
        .as_ref()
        .map(|snapshot| control_report(snapshot, known, page));
    let changes = counted(
        prepared
            .as_ref()
            .map_or(rows_replaced::other_changes(page), |report| &report.changes),
    );
    if effects.settled.navigated {
        lines.extend(reload_head(&changes));
    } else {
        lines.extend(within_budget(&changes));
    }
    lines.extend(rows_replaced::preview(page));
    let json_counts = counted(rows_replaced::other_changes(page));
    let mut changes_data = change_data(&json_counts);
    if let Some(rows) = rows_replaced::data(page) {
        changes_data["rows_replaced"] = rows;
    }
    merge(&mut data, changes_data);
    lines.extend(text_check_note(page));
    if effects.settled.navigated {
        let report = current_page_view(session)?;
        lines.push(report.text);
        merge(&mut data, json!({ "page": report.data }));
        return Ok((lines, data));
    }
    if let Some(prepared) = prepared {
        lines.extend(prepared.lines);
        set(&mut data, "new", json!(prepared.new));
    }
    if let Some(check) = action_bot_check(session, &page.title, captured.as_ref()) {
        lines.insert(0, check.note());
        check.add_data(&mut data);
    }
    Ok((lines, data))
}

fn text_check_data(page: &tabref_page::PageChanges) -> Value {
    json!({
        "unread_frames": page.unread_frames,
        "unchecked_text_blocks": page.unchecked_text_blocks,
    })
}

fn capture_control_changes(
    session: &mut Session,
    effects: &ActionEffects,
    page: &tabref_page::PageChanges,
) -> Result<Option<CapturedSnapshot>, AppError> {
    (!effects.settled.navigated && (page.changed_anything() || effects.settled.same_document))
        .then(|| session.capture())
        .transpose()
}

fn text_check_note(page: &tabref_page::PageChanges) -> Option<String> {
    let count = page.unchecked_text_blocks;
    (count > 0).then(|| format!("Text blocks over budget and not checked: {count}"))
}

/// Reports no visible change after a click, key press, hover or focus when
/// the document, open shadow roots and same-origin frames showed no effect.
/// Names the gap when a visible cross-origin frame could not be checked.
/// A focus the page moved away already says so.
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
