use serde_json::{Value, json};
use tabref_core::{AppError, Deadline, DeliverySemantics, ErrorCode};

use super::element_call::ElementCall;
use super::element_errors::node_error;
use super::remote_element::{call_on_element, prove_document};
use crate::{CdpConnection, ElementToken, PageSession};
use tabref_cdp::is_protocol_error;

const HOLDS_FOCUS: &str = r"function() {
  const root = this.getRootNode ? this.getRootNode() : null;
  const active = (root && root.activeElement) || this.ownerDocument.activeElement;
  return active === this || (!!active && this.contains(active));
}";

/// Moves the page's keyboard focus to the node. Only the page's own focus
/// changes; the browser window is not raised.
pub fn focus(
    connection: &mut CdpConnection,
    session: &PageSession,
    token: &ElementToken,
    deadline: Deadline,
) -> Result<(), AppError> {
    prove_document(connection, session, token, deadline)?;
    connection
        .call(
            "DOM.focus",
            json!({ "backendNodeId": token.backend_node_id }),
            Some(&session.session_id),
            deadline,
        )
        .map(|_| ())
        .map_err(|error| node_error(error, token))
}

/// [`focus`] for the `focus` command. The page is first told it has the
/// window focus (`Emulation.setFocusEmulationEnabled`, left on), as a
/// headless or background tab would otherwise move the focus without
/// firing the page's `focus` handlers. A node that cannot take focus (a
/// clickable `div` with no tab index) is `ACTION_NOT_SUPPORTED`
/// `cdp_not_focusable`. Returns whether the node, or one inside it, holds
/// the focus afterwards, as a page may move it away at once.
pub fn focus_element(
    connection: &mut CdpConnection,
    session: &PageSession,
    token: &ElementToken,
    deadline: Deadline,
) -> Result<bool, AppError> {
    connection.call(
        "Emulation.setFocusEmulationEnabled",
        json!({ "enabled": true }),
        Some(&session.session_id),
        deadline,
    )?;
    focus(connection, session, token, deadline).map_err(|error| {
        if is_protocol_error(&error) {
            not_focusable(token.backend_node_id, error.platform_detail.as_deref())
        } else {
            error
        }
    })?;
    let held = call_on_element(
        connection,
        session,
        token,
        ElementCall {
            function: HOLDS_FOCUS,
            arguments: Vec::new(),
        },
        deadline,
    )?;
    Ok(held == Value::Bool(true))
}

fn not_focusable(backend_node_id: i64, detail: Option<&str>) -> AppError {
    AppError::new(
        ErrorCode::ActionNotSupported,
        format!("DOM node {backend_node_id} cannot take the keyboard focus"),
    )
    .with_suggestion("Focus a field, button or link inside it, or click it instead")
    .with_platform_detail(detail.unwrap_or("not focusable"))
    .with_details(json!({ "kind": "cdp_not_focusable", "backend_node_id": backend_node_id }))
    .with_disposition(DeliverySemantics::not_delivered())
}
