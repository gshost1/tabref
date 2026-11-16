//! The JavaScript dialogs a command met, put into what it reports: each as
//! a line saying which dialog opened and how it was answered, and in
//! `data.dialogs`. `open` (and `do`'s open step) puts them first; any other
//! command gets the ones its own report did not take at the end, and an
//! error carries them too. A `beforeunload` dismissed on navigation keeps
//! the page, so `open` says it stayed instead of failing.

use serde_json::{Value, json};
use tabref_core::AppError;
use tabref_page::{PageInfo, drain_dialogs};

use super::action_report::dialog_line;
use crate::output::outcome::Outcome;
use crate::session::Session;

/// The dialogs the connection answered that no report took yet.
pub(crate) fn navigation_dialogs(session: &mut Session) -> Vec<Value> {
    drain_dialogs(session.connection())
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
    let mut lines: Vec<String> = dialogs.iter().map(dialog_line).collect();
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
    let lines: Vec<String> = dialogs.iter().map(dialog_line).collect();
    report.text = format!("{}\n{}", lines.join("\n"), report.text);
    add_data(report, dialogs);
}

/// Puts the dialogs after a report.
pub(crate) fn after_report(report: &mut Outcome, dialogs: &[Value]) {
    for dialog in dialogs {
        report.push_line(&dialog_line(dialog));
    }
    add_data(report, dialogs);
}

/// Adds the dialogs to an error, in its message and its details.
pub(crate) fn error_with_dialogs(mut error: AppError, dialogs: &[Value]) -> AppError {
    if dialogs.is_empty() {
        return error;
    }
    let lines: Vec<String> = dialogs.iter().map(dialog_line).collect();
    error.message = format!("{} ({})", error.message, lines.join("; "));
    let mut details = error.details.take().unwrap_or_else(|| json!({}));
    if let Some(object) = details.as_object_mut() {
        object.insert("dialogs".to_owned(), json!(dialogs));
    }
    error.details = Some(details);
    error
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
