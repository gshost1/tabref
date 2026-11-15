use serde_json::{Value, json};
use tabref_core::{AppError, Deadline};

use super::dom_parse::{CAPTURED_STYLES, parse_capture};
use super::dom_tree::DomTree;
use crate::tree::target_read::frame_owner;
use crate::{CdpConnection, PageSession};

/// Captures every document the session's target renders (its main document
/// first, then its in-process iframes) with one `DOMSnapshot.captureSnapshot`,
/// with each rendered node's display, visibility, position and box.
pub fn capture_dom(
    connection: &mut CdpConnection,
    session: &PageSession,
    deadline: Deadline,
) -> Result<Vec<DomTree>, AppError> {
    let reply = connection.call(
        "DOMSnapshot.captureSnapshot",
        json!({ "computedStyles": CAPTURED_STYLES }),
        Some(&session.session_id),
        deadline,
    )?;
    Ok(parse_capture(&reply, &session.target_id))
}

/// The backend node id of the `<iframe>` in `parent`'s documents that
/// shows the frame `frame_id`, or `None` when `parent` does not hold it.
pub fn frame_owner_in(
    connection: &mut CdpConnection,
    parent: &PageSession,
    frame_id: &str,
    deadline: Deadline,
) -> Result<Option<i64>, AppError> {
    frame_owner(connection, parent, frame_id, deadline)
}

/// The layout viewport's size in CSS pixels, `[width, height]`.
pub fn read_viewport(
    connection: &mut CdpConnection,
    session: &PageSession,
    deadline: Deadline,
) -> Result<[f64; 2], AppError> {
    let reply = connection.call(
        "Page.getLayoutMetrics",
        json!({}),
        Some(&session.session_id),
        deadline,
    )?;
    let size = |key: &str| {
        reply
            .pointer(&format!("/cssLayoutViewport/{key}"))
            .and_then(Value::as_f64)
            .unwrap_or(0.0)
    };
    Ok([size("clientWidth"), size("clientHeight")])
}
