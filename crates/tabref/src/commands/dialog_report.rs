//! The JavaScript dialogs a command met, put into what it reports: each as
//! a line saying which dialog opened and how it was answered, and in
//! `data.dialogs`. `open` (and `do`'s open step) puts them first; any other
//! command gets the ones its own report did not take at the end, and an
//! error carries them too. A `beforeunload` dismissed on navigation keeps
//! the page, so `open` says it stayed instead of failing.

use serde_json::{Value, json};
use tabref_core::AppError;
use tabref_page::{PageInfo, drain_dialogs, mark_other_tab};

use super::action_report::dialog_lines;
use crate::output::outcome::Outcome;
use crate::session::Session;

/// The dialogs the connection answered that no report took yet, each
/// marked when it opened in another tab than `tab`.
pub(crate) fn navigation_dialogs(session: &mut Session, tab: Option<&str>) -> Vec<Value> {
    drain_dialogs(session.connection(), tab)
}

/// The result when a dismissed `beforeunload` kept the page `info` shows,
/// rather than open `url`.
pub(crate) fn kept_page(dialogs: &[Value], url: &str, info: &PageInfo) -> Option<Outcome> {
    let dismissed = dialogs.iter().any(|dialog| {
        dialog["type"] == "beforeunload" && dialog["answer"]["accept"] == Value::Bool(false)
    });
    if !dismissed {
        return None;
    }
    let mut lines: Vec<String> = dialog_lines(dialogs);
    lines.push(format!(
        "stayed on the page: its beforeunload dialog was dismissed, so {url} was not opened"
    ));
    Some(Outcome::new(
        lines.join("\n"),
        json!({
            "navigated": false,
            "kept_page": true,
            "url": info.url,
            "title": info.title,
            "load_finished": true,
            "dialogs": dialogs,
        }),
    ))
}

/// Puts the dialogs ahead of a report.
pub(crate) fn with_dialogs(report: &mut Outcome, dialogs: &[Value]) {
    if dialogs.is_empty() {
        return;
    }
    let lines = dialog_lines(dialogs);
    report.text = format!("{}\n{}", lines.join("\n"), report.text);
    add_data(report, dialogs);
}

/// Puts the dialogs after a report.
pub(crate) fn after_report(report: &mut Outcome, dialogs: &[Value]) {
    for line in dialog_lines(dialogs) {
        report.push_line(&line);
    }
    add_data(report, dialogs);
}

/// Adds the dialogs to an error, in its message and its details.
pub(crate) fn error_with_dialogs(mut error: AppError, dialogs: &[Value]) -> AppError {
    if dialogs.is_empty() {
        return error;
    }
    let lines = dialog_lines(dialogs);
    error.message = format!("{} ({})", error.message, lines.join("; "));
    let mut details = error.details.take().unwrap_or_else(|| json!({}));
    if let Some(object) = details.as_object_mut() {
        object.insert("dialogs".to_owned(), json!(dialogs));
    }
    error.details = Some(details);
    error
}

/// Marks the dialogs that opened in another tab than the one this command
/// works in (`other_tab`), when the connection knew their tab.
/// The command's own tab is the one it chose; only when it chose none is the
/// tab listing asked, and a listing that fails marks nothing.
pub(crate) fn mark_other_tabs(session: &mut Session, mut dialogs: Vec<Value>) -> Vec<Value> {
    if !dialogs.iter().any(|dialog| dialog["tab"].is_string()) {
        return dialogs;
    }
    let current = match session.known_tab() {
        Some(tab) => Some(tab.to_owned()),
        None => session
            .current_tab_unclaimed()
            .ok()
            .flatten()
            .map(|tab| tab.target_id),
    };
    for dialog in &mut dialogs {
        mark_other_tab(dialog, current.as_deref());
    }
    dialogs
}

fn add_data(report: &mut Outcome, dialogs: &[Value]) {
    if dialogs.is_empty() {
        return;
    }
    let Some(object) = report.data.as_object_mut() else {
        return;
    };
    match object.get_mut("dialogs").and_then(Value::as_array_mut) {
        Some(known) => known.extend(dialogs.iter().cloned()),
        None => {
            object.insert("dialogs".to_owned(), json!(dialogs));
        }
    }
}

#[cfg(test)]
#[path = "dialog_report_tests.rs"]
mod tests;
