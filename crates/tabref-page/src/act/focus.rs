use serde_json::{Value, json};
use tabref_core::{AppError, Deadline, DeliverySemantics, ErrorCode};

use super::drawn_element::drawn_element;
use super::element_call::ElementCall;
use super::element_errors::node_error;
use super::remote_element::{call_function, call_on_element, prove_document, with_element};
use crate::{CdpConnection, ElementToken, PageSession};
use tabref_cdp::is_protocol_error;

const HOLDS_FOCUS: &str = r"function() {
  const root = this.getRootNode ? this.getRootNode() : null;
  const active = (root && root.activeElement) || this.ownerDocument.activeElement;
  return active === this || (!!active && this.contains(active));
}";

const LABELLED_CONTROL: &str = r"function() {
  return this.localName === 'label' && this.control ? this.control : null;
}";

const FOCUSABLE: &str = r"function() {
  return this.tabIndex >= 0 || this.hasAttribute('tabindex') || this.isContentEditable;
}";

const PAGE_FOCUSED: &str = "document.visibilityState === 'visible' && document.hasFocus()";

/// Moves the page's keyboard focus to the node. Only the page's own focus
/// changes; the browser window is not raised. A node that cannot take
/// focus is `ACTION_NOT_SUPPORTED` `cdp_not_focusable`.
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
        .map_err(|error| {
            let refused = is_protocol_error(&error)
                && error
                    .platform_detail
                    .as_deref()
                    .is_some_and(|detail| detail.to_ascii_lowercase().contains("not focusable"));
            if refused {
                not_focusable(token.backend_node_id, error.platform_detail.as_deref())
            } else {
                node_error(error, token)
            }
        })
}

/// The element the `focus` command focuses for `token`: the node itself,
/// or the control a `<label>` names. A node that cannot take focus is
/// refused here, before anything in the page changes.
pub fn focus_target(
    connection: &mut CdpConnection,
    session: &PageSession,
    token: &ElementToken,
    deadline: Deadline,
) -> Result<ElementToken, AppError> {
    let control = with_element(
        connection,
        session,
        token,
        deadline,
        |connection, object| {
            drawn_element(
                connection,
                session,
                (object, LABELLED_CONTROL),
                token,
                deadline,
            )
        },
    )?;
    let target = control.unwrap_or_else(|| token.clone());
    let focusable = with_element(
        connection,
        session,
        &target,
        deadline,
        |connection, object| {
            call_function(
                connection,
                session,
                object,
                ElementCall {
                    function: FOCUSABLE,
                    arguments: Vec::new(),
                },
                deadline,
            )
        },
    )?;
    if focusable != Value::Bool(true) {
        return Err(not_focusable(target.backend_node_id, None));
    }
    Ok(target)
}

/// Whether the tab is visible and holds the window focus, so its `focus`
/// handlers fire without help.
pub fn page_focused(
    connection: &mut CdpConnection,
    session: &PageSession,
    deadline: Deadline,
) -> Result<bool, AppError> {
    let reply = connection.call(
        "Runtime.evaluate",
        json!({ "expression": PAGE_FOCUSED, "returnByValue": true }),
        Some(&session.session_id),
        deadline,
    )?;
    Ok(reply.pointer("/result/value") == Some(&Value::Bool(true)))
}

/// Tells the tab it holds the window focus (`on`), or stops telling it.
/// While on, a background tab reads as visible and focused.
pub fn emulate_focus(
    connection: &mut CdpConnection,
    session: &PageSession,
    on: bool,
    deadline: Deadline,
) -> Result<(), AppError> {
    connection
        .call(
            "Emulation.setFocusEmulationEnabled",
            json!({ "enabled": on }),
            Some(&session.session_id),
            deadline,
        )
        .map(|_| ())
}

/// Whether the node, or one inside it, holds the focus.
pub fn holds_focus(
    connection: &mut CdpConnection,
    session: &PageSession,
    token: &ElementToken,
    deadline: Deadline,
) -> Result<bool, AppError> {
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
