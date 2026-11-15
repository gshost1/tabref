//! Out-of-process iframes are their own targets. Turning auto-attach on for a
//! target's session makes the browser attach a flattened session to every
//! iframe target directly beneath it and report each one before it answers;
//! turning it off again detaches them.

use serde_json::{Value, json};

use super::child_target::ChildTarget;
use crate::{CdpConnection, CdpEvent, PageSession};

const ATTACHED: &str = "Target.attachedToTarget";
const DETACHED: &str = "Target.detachedFromTarget";
pub(crate) const AUTO_ATTACH: &str = "Target.setAutoAttach";

pub(crate) fn auto_attach_params(enabled: bool) -> Value {
    json!({ "autoAttach": enabled, "waitForDebuggerOnStart": false, "flatten": true })
}

/// Takes the iframe targets the browser reported as attached beneath
/// `parent`, in the order it reported them.
pub(crate) fn take_attached_children(
    connection: &mut CdpConnection,
    parent: &PageSession,
) -> Vec<ChildTarget> {
    let parent = parent.session_id.as_str();
    connection
        .drain_events(|event| {
            event.method == ATTACHED
                && event.session_id.as_deref() == Some(parent)
                && event
                    .params
                    .pointer("/targetInfo/type")
                    .and_then(Value::as_str)
                    == Some("iframe")
        })
        .iter()
        .filter_map(child_target)
        .collect()
}

/// Drops the attach bookkeeping events of `parent`'s children.
pub(crate) fn discard_attach_events(connection: &mut CdpConnection, parent: &PageSession) {
    let parent = parent.session_id.as_str();
    connection.discard_events(|event| {
        matches!(event.method.as_str(), ATTACHED | DETACHED)
            && event.session_id.as_deref() == Some(parent)
    });
}

fn child_target(event: &CdpEvent) -> Option<ChildTarget> {
    let text = |pointer: &str| {
        event
            .params
            .pointer(pointer)
            .and_then(Value::as_str)
            .filter(|value| !value.is_empty())
            .map(str::to_owned)
    };
    Some(ChildTarget {
        session: PageSession {
            target_id: text("/targetInfo/targetId")?,
            session_id: text("/sessionId")?,
        },
        parent_frame_id: text("/targetInfo/parentFrameId"),
        url: text("/targetInfo/url").unwrap_or_default(),
        waiting: event
            .params
            .get("waitingForDebugger")
            .and_then(Value::as_bool)
            == Some(true),
    })
}
