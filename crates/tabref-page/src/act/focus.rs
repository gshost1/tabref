use std::time::Duration;

use serde_json::{Value, json};
use tabref_core::{AppError, Deadline, DeliverySemantics, ErrorCode};

use super::drawn_element::drawn_element;
use super::element_call::ElementCall;
use super::element_errors::{node_error, script_failed};
use super::focus_watch::FocusWatch;
use super::remote_element::{call_function, prove_document, with_element};
use crate::{CdpConnection, ElementToken, PageSession};
use tabref_cdp::is_protocol_error;

const LABELLED_CONTROL: &str = r"function() {
  return this.localName === 'label' && this.control ? this.control : null;
}";

const FOCUSABLE: &str = r"function() {
  if (this.isContentEditable || this.hasAttribute('tabindex')) return true;
  if ((this.localName === 'a' || this.localName === 'area') && !this.hasAttribute('href')) return false;
  return this.tabIndex >= 0;
}";

const WATCH_FOCUS: &str = r"function(ms) {
  const el = this;
  const doc = el.ownerDocument;
  const holds = () => {
    const root = el.getRootNode ? el.getRootNode() : doc;
    const active = (root && root.activeElement) || doc.activeElement;
    return active === el || (!!active && el.contains(active));
  };
  return new Promise(resolve => {
    let done = false;
    let timer = 0;
    const moved = () => setTimeout(() => { if (!holds()) finish(false); }, 0);
    const finish = held => {
      if (done) return;
      done = true;
      el.removeEventListener('focusout', moved);
      doc.removeEventListener('focusin', moved, true);
      clearTimeout(timer);
      resolve(held);
    };
    if (!holds()) return finish(false);
    el.addEventListener('focusout', moved);
    doc.addEventListener('focusin', moved, true);
    timer = setTimeout(() => finish(holds()), ms);
  });
}";

const PAGE_FOCUSED: &str = "document.visibilityState === 'visible' && document.hasFocus()";

/// Moves the page's keyboard focus to the node. Only the page's own focus
/// changes; the browser window is not raised. A node the browser refuses
/// as not focusable is `ACTION_NOT_SUPPORTED` `cdp_not_focusable`.
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
/// or the control a `<label>` names. A node `DOM.focus` would refuse (no
/// tab index and not editable, or a link or image-map area with no address
/// and no `tabindex`) is refused here, before anything in the page changes.
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
/// handlers run.
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

/// Starts watching, in the page, whether the focus leaves the node within
/// `window`: a `focusout` from it or a `focusin` elsewhere ends the watch
/// the moment the focus is gone, and the window's end checks once more.
/// The watch runs while the action settles; [`focus_kept`] reads it.
pub fn watch_focus(
    connection: &mut CdpConnection,
    session: &PageSession,
    token: &ElementToken,
    window: Duration,
    deadline: Deadline,
) -> Result<FocusWatch, AppError> {
    let window_ms = u64::try_from(window.as_millis()).unwrap_or(u64::MAX);
    with_element(
        connection,
        session,
        token,
        deadline,
        |connection, object| {
            let reply = connection.call(
                "Runtime.callFunctionOn",
                json!({
                    "objectId": object,
                    "functionDeclaration": WATCH_FOCUS,
                    "arguments": [{ "value": window_ms }],
                    "returnByValue": false,
                    "awaitPromise": false,
                }),
                Some(&session.session_id),
                deadline,
            )?;
            if let Some(exception) = reply.get("exceptionDetails") {
                return Err(script_failed(exception));
            }
            let promise = reply
                .pointer("/result/objectId")
                .and_then(Value::as_str)
                .ok_or_else(|| AppError::internal("the focus watch returned no promise"))?;
            Ok(FocusWatch {
                promise: promise.to_owned(),
            })
        },
    )
}

/// Whether the node still held the focus when `watch` ended. A page that
/// navigated meanwhile took the focus with it, which is `false`.
pub fn focus_kept(
    connection: &mut CdpConnection,
    session: &PageSession,
    watch: FocusWatch,
    deadline: Deadline,
) -> Result<bool, AppError> {
    let reply = connection.call(
        "Runtime.awaitPromise",
        json!({ "promiseObjectId": watch.promise, "returnByValue": true }),
        Some(&session.session_id),
        deadline,
    );
    let _ = connection.call(
        "Runtime.releaseObject",
        json!({ "objectId": watch.promise }),
        Some(&session.session_id),
        deadline,
    );
    match reply {
        Ok(reply) => Ok(reply.pointer("/result/value") == Some(&Value::Bool(true))),
        Err(error) if is_protocol_error(&error) => Ok(false),
        Err(error) => Err(error),
    }
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
