//! The JavaScript dialogs an `open` met: a `beforeunload` the page it
//! leaves raised, answered by the connection (accepted, or as `--dialog`
//! chose). A dismissed one keeps the page, so the navigation was cancelled
//! on purpose, and `open` says so instead of failing.

use serde_json::{Value, json};

use super::action_report::dialog_line;
use crate::output::outcome::Outcome;
use crate::session::Session;

const DIALOG_OPENING: &str = "Page.javascriptDialogOpening";

/// The dialogs that opened while the navigation ran, as the action report
/// shows them (`type`, `message`, `url`, `answer`).
pub(crate) fn navigation_dialogs(session: &mut Session) -> Vec<Value> {
    session
        .connection()
        .drain_events(|event| event.method == DIALOG_OPENING)
        .into_iter()
        .map(|event| {
            json!({
                "type": event.params.get("type").cloned().unwrap_or(Value::Null),
                "message": event.params.get("message").cloned().unwrap_or(Value::Null),
                "url": event.params.get("url").cloned().unwrap_or(Value::Null),
                "answer": event.params.get("answer").cloned().unwrap_or(Value::Null),
            })
        })
        .collect()
}

/// The result when a dismissed `beforeunload` kept the page.
pub(crate) fn kept_page(dialogs: &[Value], url: &str) -> Option<Outcome> {
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
        json!({ "navigated": false, "kept_page": true, "dialogs": dialogs }),
    ))
}

/// Puts the dialogs ahead of an `open` report.
pub(crate) fn with_dialogs(report: &mut Outcome, dialogs: &[Value]) {
    if dialogs.is_empty() {
        return;
    }
    let lines: Vec<String> = dialogs.iter().map(dialog_line).collect();
    report.text = format!("{}\n{}", lines.join("\n"), report.text);
    report.data["dialogs"] = json!(dialogs);
}
