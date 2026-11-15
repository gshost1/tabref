use serde_json::{Value, json};
use tabref_core::{AppError, Deadline, DeliverySemantics, ErrorCode, Point};

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
    let mut steered = false;
    for point in points {
        match confirm_hit(
            connection,
            session,
            token,
            &viewport.hit_point(point.clone()),
            deadline,
        ) {
            Ok(false) => return Ok(point),
            Ok(true) => steered = true,
            Err(error) if is_intercepted(&error) => {
                refusal.get_or_insert(error);
            }
            Err(error) => return Err(error),
        }
    }
    if steered {
        return drawn_child_point(connection, session, token, &viewport, deadline)?
            .ok_or_else(|| inner_controls_only(token.backend_node_id));
    }
    Err(refusal
        .unwrap_or_else(|| not_rendered_error(token.backend_node_id, "the node has an empty box")))
}

/// A point of the first element a `display: contents` node draws that is
/// not a separate control, for a node whose own boxes land on one (a
/// shadow host whose first box is its link).
fn drawn_child_point(
    connection: &mut CdpConnection,
    session: &PageSession,
    token: &ElementToken,
    viewport: &ViewportMetrics,
    deadline: Deadline,
) -> Result<Option<Point>, AppError> {
    let quads = drawn_child_quads(connection, session, token, deadline)?.unwrap_or_default();
    for point in quads
        .iter()
        .filter_map(|quad| viewport.visible_center(quad))
        .take(MAX_FRAGMENTS)
    {
        let hit = viewport.hit_point(point.clone());
        match confirm_hit(connection, session, token, &hit, deadline) {
            Ok(_) => return Ok(Some(point)),
            Err(error) if is_intercepted(&error) => {}
            Err(error) => return Err(error),
        }
    }
    Ok(None)
}

fn inner_controls_only(backend_node_id: i64) -> AppError {
    AppError::new(
        ErrorCode::ActionFailed,
        format!(
            "Every point of DOM node {backend_node_id} lands on a separate control it draws, which a press would act on instead"
        ),
    )
    .with_suggestion("Click the control inside it you mean, by its own name or ref")
    .with_details(json!({ "kind": "cdp_press_on_inner_control", "backend_node_id": backend_node_id }))
    .with_disposition(DeliverySemantics::not_delivered())
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
