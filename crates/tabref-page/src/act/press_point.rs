use serde_json::{Value, json};
use tabref_core::{AppError, Deadline, Point};

use super::drawn_element::drawn_child;
use super::hit_test::{confirm_hit, is_intercepted};
use super::scroll::{NOT_RENDERED, not_rendered, not_rendered_error, quad_center};
use super::viewport_metrics::ViewportMetrics;
use crate::{CdpConnection, ElementToken, PageSession};

const MAX_FRAGMENTS: usize = 4;

/// Finds the viewport point a press on the node would land on: the centre
/// of the visible part of each box `DOM.getContentQuads` reports, one per
/// line for a wrapped inline element, tried in order until one hit-tests to
/// the node. A node every tried point is covered at is the first refusal.
pub(crate) fn press_point(
    connection: &mut CdpConnection,
    session: &PageSession,
    token: &ElementToken,
    deadline: Deadline,
) -> Result<Point, AppError> {
    let viewport = ViewportMetrics::read(connection, session, deadline)?;
    let quads = match content_quads(connection, session, token.backend_node_id, deadline) {
        Ok(quads) if !quads.is_empty() => quads,
        Ok(_) => drawn_child_quads(connection, session, token, deadline)?.unwrap_or_default(),
        Err(error) => {
            let error = not_rendered(error, token);
            if error.kind() != Some(NOT_RENDERED) {
                return Err(error);
            }
            drawn_child_quads(connection, session, token, deadline)?.ok_or(error)?
        }
    };
    let mut points: Vec<Point> = quads
        .iter()
        .filter_map(|quad| viewport.visible_center(quad))
        .take(MAX_FRAGMENTS)
        .collect();
    if points.is_empty() {
        points.extend(quads.iter().find_map(|quad| quad_center(Some(quad))));
    }
    let mut refusal = None;
    for point in points {
        match confirm_hit(
            connection,
            session,
            token,
            &viewport.hit_point(point.clone()),
            deadline,
        ) {
            Ok(()) => return Ok(point),
            Err(error) if is_intercepted(&error) => {
                refusal.get_or_insert(error);
            }
            Err(error) => return Err(error),
        }
    }
    Err(refusal
        .unwrap_or_else(|| not_rendered_error(token.backend_node_id, "the node has an empty box")))
}

fn content_quads(
    connection: &mut CdpConnection,
    session: &PageSession,
    backend_node_id: i64,
    deadline: Deadline,
) -> Result<Vec<Value>, AppError> {
    let reply = connection.call(
        "DOM.getContentQuads",
        json!({ "backendNodeId": backend_node_id }),
        Some(&session.session_id),
        deadline,
    )?;
    Ok(reply
        .get("quads")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default())
}

/// The quads of the first element a node with no box of its own draws
/// through, as a `display: contents` host draws through its shadow
/// children. A hit on that element is a hit inside the node.
fn drawn_child_quads(
    connection: &mut CdpConnection,
    session: &PageSession,
    token: &ElementToken,
    deadline: Deadline,
) -> Result<Option<Vec<Value>>, AppError> {
    let Some(child) = drawn_child(connection, session, token, deadline)? else {
        return Ok(None);
    };
    Ok(
        content_quads(connection, session, child.backend_node_id, deadline)
            .ok()
            .filter(|quads| !quads.is_empty()),
    )
}

#[cfg(test)]
#[path = "press_point_tests.rs"]
mod tests;
