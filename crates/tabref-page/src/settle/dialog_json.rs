use serde_json::{Value, json};

use crate::{CdpConnection, CdpEvent};

/// The event a JavaScript dialog opening raises.
pub const DIALOG_OPENING: &str = "Page.javascriptDialogOpening";

/// A dialog as reports show it: its `type`, `message`, the `url` of the
/// frame that opened it, and the `answer` the connection gave it
/// (`{accept, text}`; null when the connection did not answer it).
pub fn dialog_json(event: &CdpEvent) -> Value {
    let field = |name: &str| event.params.get(name).cloned().unwrap_or(Value::Null);
    json!({
        "type": field("type"),
        "message": field("message"),
        "url": field("url"),
        "answer": field("answer"),
    })
}

/// The dialogs the connection holds events for, taken out of its buffer.
pub fn drain_dialogs(connection: &mut CdpConnection) -> Vec<Value> {
    connection
        .drain_events(|event| event.method == DIALOG_OPENING)
        .iter()
        .map(dialog_json)
        .collect()
}
