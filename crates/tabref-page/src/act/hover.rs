use serde_json::json;
use tabref_core::{AppError, Deadline, Point};

use super::press_point::press_point;
use super::remote_element::prove_document;
use super::scroll::scroll_node;
use crate::{CdpConnection, ElementToken, PageSession};

/// Moves the page's pointer over the node: scrolls it into view, finds a
/// visible point of it that hit-tests to the node, as a click does, and
/// sends one `mouseMoved` there, which fires the page's `mouseover`,
/// `mouseenter` and `:hover` styles. A node another element covers is
/// refused before any input. Returns the viewport point.
pub fn hover(
    connection: &mut CdpConnection,
    session: &PageSession,
    token: &ElementToken,
    deadline: Deadline,
) -> Result<Point, AppError> {
    prove_document(connection, session, token, deadline)?;
    scroll_node(connection, session, token, deadline)?;
    let point = press_point(connection, session, token, deadline)?;
    connection.call(
        "Input.dispatchMouseEvent",
        json!({
            "type": "mouseMoved",
            "x": point.x,
            "y": point.y,
            "button": "none",
            "buttons": 0,
        }),
        Some(&session.session_id),
        deadline,
    )?;
    Ok(point)
}

#[cfg(test)]
#[path = "hover_tests.rs"]
mod tests;
