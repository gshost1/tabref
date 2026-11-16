use serde_json::{Value, json};

use crate::{CdpConnection, CdpEvent};

/// The event a JavaScript dialog opening raises.
pub const DIALOG_OPENING: &str = "Page.javascriptDialogOpening";

/// A dialog as reports show it: its `type`, `message`, the `url` of the
/// frame that opened it, the `tab` (target id) it opened in when the
/// connection knows it, the `answer` the connection gave it (`{accept,
/// text}`; null when the connection did not answer it), and whether it
/// opened while no command was running (`late`).
pub fn dialog_json(event: &CdpEvent) -> Value {
    let field = |name: &str| event.params.get(name).cloned().unwrap_or(Value::Null);
    json!({
        "type": field("type"),
        "message": field("message"),
        "url": field("url"),
        "answer": field("answer"),
        "tab": field("tab"),
        "late": event.params.get("late").and_then(Value::as_bool).unwrap_or(false),
    })
}

/// The dialogs the connection holds events for, taken out of its buffer,
/// each marked when it opened in another tab than `tab` (the command's,
/// when known).
pub fn drain_dialogs(connection: &mut CdpConnection, tab: Option<&str>) -> Vec<Value> {
    connection
        .drain_events(|event| event.method == DIALOG_OPENING)
        .iter()
        .map(|event| {
            let mut dialog = dialog_json(event);
            mark_other_tab(&mut dialog, tab);
            dialog
        })
        .collect()
}

/// Marks a dialog `other_tab` when it opened in another tab than `tab`,
/// the command's own; nothing is marked when either tab is unknown.
pub fn mark_other_tab(dialog: &mut Value, tab: Option<&str>) {
    let elsewhere = match (dialog["tab"].as_str(), tab) {
        (Some(opened_in), Some(command)) => opened_in != command,
        _ => false,
    };
    if elsewhere {
        dialog["other_tab"] = Value::Bool(true);
    }
}

#[cfg(test)]
#[path = "dialog_json_tests.rs"]
mod tests;
