use serde_json::{Value, json};
use tabref_core::{AppError, Deadline, DeliverySemantics, ErrorCode, Point};

use super::drawn_element::drawn_child;
use super::element_errors::node_error;
use super::remote_element::prove_document;
use crate::{CdpConnection, ElementToken, PageSession};
use tabref_cdp::{is_node_gone, is_protocol_error};

/// Scrolls the node into the viewport if it is not already visible.
pub fn scroll_into_view(
    connection: &mut CdpConnection,
    session: &PageSession,
    token: &ElementToken,
    deadline: Deadline,
) -> Result<(), AppError> {
    prove_document(connection, session, token, deadline)?;
    scroll_node(connection, session, token, deadline)
}

/// The centre of the node's border box in CSS pixels of the main frame's
/// viewport, the space `Input.dispatchMouseEvent` takes.
pub fn box_center(
    connection: &mut CdpConnection,
    session: &PageSession,
    token: &ElementToken,
    deadline: Deadline,
) -> Result<Point, AppError> {
    prove_document(connection, session, token, deadline)?;
    node_center(connection, session, token, deadline)
}

/// Scrolls the node into view, or the first element it draws through when
/// it has no box of its own (a `display: contents` host).
pub(crate) fn scroll_node(
    connection: &mut CdpConnection,
    session: &PageSession,
    token: &ElementToken,
    deadline: Deadline,
) -> Result<(), AppError> {
    let error = match scroll_backend(connection, session, token.backend_node_id, deadline) {
        Ok(()) => return Ok(()),
        Err(error) => not_rendered(error, token),
    };
    if error.kind() != Some(NOT_RENDERED) {
        return Err(error);
    }
    let Some(child) = drawn_child(connection, session, token, deadline)? else {
        return Err(error);
    };
    scroll_backend(connection, session, child.backend_node_id, deadline).map_err(|_| error)
}

fn scroll_backend(
    connection: &mut CdpConnection,
    session: &PageSession,
    backend_node_id: i64,
    deadline: Deadline,
) -> Result<(), AppError> {
    connection
        .call(
            "DOM.scrollIntoViewIfNeeded",
            json!({ "backendNodeId": backend_node_id }),
            Some(&session.session_id),
            deadline,
        )
        .map(|_| ())
}

fn node_center(
    connection: &mut CdpConnection,
    session: &PageSession,
    token: &ElementToken,
    deadline: Deadline,
) -> Result<Point, AppError> {
    let reply = connection
        .call(
            "DOM.getBoxModel",
            json!({ "backendNodeId": token.backend_node_id }),
            Some(&session.session_id),
            deadline,
        )
        .map_err(|error| not_rendered(error, token))?;
    quad_center(reply.pointer("/model/border"))
        .ok_or_else(|| not_rendered_error(token.backend_node_id, "the node has an empty box"))
}

pub(crate) fn quad_center(quad: Option<&Value>) -> Option<Point> {
    let numbers: Vec<f64> = quad?.as_array()?.iter().filter_map(Value::as_f64).collect();
    if numbers.len() != 8 {
        return None;
    }
    let xs = [numbers[0], numbers[2], numbers[4], numbers[6]];
    let ys = [numbers[1], numbers[3], numbers[5], numbers[7]];
    let width =
        xs.iter().copied().fold(f64::MIN, f64::max) - xs.iter().copied().fold(f64::MAX, f64::min);
    let height =
        ys.iter().copied().fold(f64::MIN, f64::max) - ys.iter().copied().fold(f64::MAX, f64::min);
    if width <= 0.0 || height <= 0.0 {
        return None;
    }
    Some(Point {
        x: xs.iter().sum::<f64>() / 4.0,
        y: ys.iter().sum::<f64>() / 4.0,
    })
}

pub(crate) fn not_rendered(error: AppError, token: &ElementToken) -> AppError {
    if is_node_gone(&error) || !is_protocol_error(&error) {
        return node_error(error, token);
    }
    let detail = error.platform_detail.clone().unwrap_or_default();
    not_rendered_error(token.backend_node_id, &detail)
}

pub(crate) const NOT_RENDERED: &str = "cdp_node_not_rendered";

pub(crate) fn not_rendered_error(backend: i64, detail: &str) -> AppError {
    AppError::new(
        ErrorCode::ActionFailed,
        format!("DOM node {backend} has no rendered box to act on"),
    )
    .with_suggestion("The element may be hidden or detached from layout; refresh the snapshot")
    .with_platform_detail(detail)
    .with_details(json!({ "kind": NOT_RENDERED, "backend_node_id": backend }))
    .with_disposition(DeliverySemantics::not_delivered())
}

#[cfg(test)]
#[path = "scroll_tests.rs"]
mod tests;
