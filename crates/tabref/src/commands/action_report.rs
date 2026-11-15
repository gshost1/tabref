use std::collections::HashSet;

use serde_json::{Value, json};
use tabref_core::{AccessibilityNode, AppError};

use super::acted::Acted;
use super::page_url::short_url;
use super::page_view::current_page_view;
use crate::output::outcome::Outcome;
use crate::session::Session;
use crate::session::action_effects::ActionEffects;
use crate::view::node_line::{node_line, quote_text};
use crate::view::ref_style::RefStyle;

const CHANGE_BUDGET: usize = 600;
const NOTHING_SEEN: &str = " (no visible change)";
const NEW_SHOWN: usize = 8;
const NEW_LINE_CHARS: usize = 80;

/// An action as a single command prints it: `clicked "Save"`, then what it
/// set off, capped near 150 tokens: dialogs, the new address, the page's
/// changed lines, the controls that appeared with their refs, or, when it
/// led to another page, a snapshot of that page.
pub(crate) fn full_report(session: &mut Session, acted: Acted) -> Result<Outcome, AppError> {
    let mut first = format!("{} {}", past(acted.verb), acted.summary);
    if nothing_seen(&acted) {
        first.push_str(NOTHING_SEEN);
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
    if nothing_seen(&acted) {
        line.push_str(NOTHING_SEEN);
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
    for dialog in effects.dialogs() {
        line.push_str(&format!("; {}", dialog_line(dialog)));
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
        if !leaves && let Some(first) = page.changes.first() {
            line.push_str(&format!("; {first}"));
            if page.changes.len() > 1 {
                line.push_str(&format!(" (+{} more)", page.changes.len() - 1));
            }
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
    let mut lines: Vec<String> = effects.dialogs().iter().map(dialog_line).collect();
    let mut data = json!({
        "dialogs": effects.dialogs(),
        "navigated": effects.settled.navigated,
    });
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
    let changes: Vec<String> = page.changes.iter().map(ToString::to_string).collect();
    lines.extend(within_budget(&changes));
    merge(&mut data, json!({ "changes": changes }));
    if effects.settled.navigated {
        lines.push("the page reloaded, so earlier refs are stale; tabref snap for new ones".into());
    } else if page.changed_anything() || effects.settled.same_document {
        let captured = session.capture()?;
        let mut appeared = Vec::new();
        new_controls(&captured.tree, known, &mut appeared);
        let shown = appeared.len().min(NEW_SHOWN);
        lines.extend(
            appeared
                .iter()
                .take(shown)
                .map(|line| format!("new {line}")),
        );
        if appeared.len() > shown {
            lines.push(format!("+{} more new: tabref snap", appeared.len() - shown));
        }
        merge(&mut data, json!({ "new": appeared }));
    }
    Ok((lines, data))
}

/// Whether a click, key press or hover showed no effect at all: no
/// navigation, tab, dialog, DOM mutation or text change. Changes inside a
/// closed shadow root or drawn on a canvas look like this too.
fn nothing_seen(acted: &Acted) -> bool {
    let Some(effects) = &acted.effects else {
        return false;
    };
    matches!(acted.verb, "click" | "press" | "hover")
        && !effects.settled.navigated
        && !effects.settled.same_document
        && effects.new_tab.is_none()
        && effects.dialogs().is_empty()
        && effects
            .page
            .as_ref()
            .is_some_and(|page| page.mutations == Some(0) && page.changes.is_empty())
}

fn within_budget(changes: &[String]) -> Vec<String> {
    let mut used = 0;
    let mut shown = Vec::new();
    for change in changes {
        if used + change.len() > CHANGE_BUDGET && !shown.is_empty() {
            break;
        }
        used += change.len();
        shown.push(change.clone());
    }
    if shown.len() < changes.len() {
        shown.push(format!(
            "+{} more: tabref snap",
            changes.len() - shown.len()
        ));
    }
    shown
}

fn new_controls(node: &AccessibilityNode, known: &HashSet<String>, found: &mut Vec<String>) {
    if node.ref_id.is_some()
        && node
            .identity
            .retained_object
            .as_ref()
            .is_some_and(|token| !known.contains(token))
    {
        found.push(styled(&node_line(node, NEW_LINE_CHARS)));
    }
    for child in &node.children {
        new_controls(child, known, found);
    }
}

fn dialog_line(dialog: &Value) -> String {
    let kind = dialog
        .get("type")
        .and_then(Value::as_str)
        .unwrap_or("dialog");
    let message = dialog
        .get("message")
        .and_then(Value::as_str)
        .unwrap_or_default();
    format!("dialog {kind} {} accepted", quote_text(message))
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
