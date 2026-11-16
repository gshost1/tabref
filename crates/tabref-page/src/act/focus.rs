use std::time::Duration;

use serde_json::{Value, json};
use tabref_core::{AppError, Deadline, DeliverySemantics, ErrorCode};

use super::drawn_element::drawn_element;
use super::element_call::ElementCall;
use super::element_errors::{node_error, script_failed};
use super::focus_watch::FocusWatch;
use super::remote_element::{call_function, call_on_element, prove_document, with_element};
use crate::{CdpConnection, ElementToken, PageSession};
use tabref_cdp::is_protocol_error;

const FOCUS_CONTROL: &str = r"function() {
  if (this.localName === 'label' && this.control) return this.control;
  if (this.shadowRoot && this.shadowRoot.delegatesFocus) {
    return this.shadowRoot.querySelector('input:not([disabled]), select:not([disabled]), textarea:not([disabled]), button:not([disabled]), a[href], area[href], [tabindex], [contenteditable]');
  }
  return null;
}";

const FOCUSABLE: &str = r"function() {
  const index = this.getAttribute('tabindex');
  if (index !== null && !Number.isNaN(parseInt(index, 10))) return true;
  if (this.isContentEditable) return true;
  if (this.shadowRoot && this.shadowRoot.delegatesFocus) return true;
  const html = this.namespaceURI === 'http://www.w3.org/1999/xhtml';
  if (html && (this.localName === 'a' || this.localName === 'area') && !this.hasAttribute('href')) return false;
  return this.tabIndex >= 0;
}";

const HOLDS_FOCUS: &str = r"function() {
  const root = this.getRootNode ? this.getRootNode() : this.ownerDocument;
  const active = (root && root.activeElement) || this.ownerDocument.activeElement;
  return active === this || (!!active && this.contains(active));
}";

const WATCH_FOCUS: &str = r"function(ms, listeners) {
  const el = this;
  const doc = el.ownerDocument;
  const holds = () => {
    const root = el.getRootNode ? el.getRootNode() : doc;
    const active = (root && root.activeElement) || doc.activeElement;
    return active === el || (!!active && el.contains(active));
  };
  const kinds = ['focus', 'focusin', 'blur', 'focusout'];
  const path = [];
  for (let node = el; node; node = node.parentNode || node.host || null) path.push(node);
  path.push(doc.defaultView);
  const heard = node => {
    const found = listeners(node) || {};
    return kinds.some(kind => (found[kind] || []).length > 0);
  };
  let listened = true;
  try { listened = typeof listeners !== 'function' || path.some(heard); } catch (error) { listened = true; }
  if (!listened) return null;
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

const WATCH_GROUP: &str = "tabref-focus-watch";

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
/// the control a `<label>` names, or, for a shadow host that delegates
/// focus, the first control in its shadow tree, which `DOM.focus` reaches
/// where it refuses the host. A node `DOM.focus` would refuse (no
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
                (object, FOCUS_CONTROL),
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
/// `window`, when anything on its path (the node, its ancestors across
/// shadow hosts, the document and the window) listens for `focus`,
/// `focusin`, `blur` or `focusout`: a `focusout` from it or a `focusin`
/// elsewhere ends the watch the moment the focus is gone, and the window's
/// end checks once more. With no such listener, no watch starts, and
/// [`focus_kept`] checks the focus once. The watch runs while the action
/// settles. A watch an earlier, cancelled command left behind is released
/// first.
pub fn watch_focus(
    connection: &mut CdpConnection,
    session: &PageSession,
    token: &ElementToken,
    window: Duration,
    deadline: Deadline,
) -> Result<FocusWatch, AppError> {
    release_focus_watch(connection, session, deadline);
    let listeners = listener_reader(connection, session, deadline);
    let window_ms = u64::try_from(window.as_millis()).unwrap_or(u64::MAX);
    let reader = listeners
        .as_ref()
        .map_or_else(|| json!({ "value": null }), |id| json!({ "objectId": id }));
    let watched = with_element(
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
                    "arguments": [{ "value": window_ms }, reader],
                    "returnByValue": false,
                    "awaitPromise": false,
                    "objectGroup": WATCH_GROUP,
                }),
                Some(&session.session_id),
                deadline,
            )?;
            if let Some(exception) = reply.get("exceptionDetails") {
                return Err(script_failed(exception));
            }
            Ok(FocusWatch {
                promise: reply
                    .pointer("/result/objectId")
                    .and_then(Value::as_str)
                    .map(str::to_owned),
            })
        },
    );
    if let Some(id) = listeners {
        let _ = connection.call(
            "Runtime.releaseObject",
            json!({ "objectId": id }),
            Some(&session.session_id),
            deadline,
        );
    }
    watched
}

/// Whether the node still held the focus when `watch` ended, or now when
/// no watch started. A page that navigated meanwhile took the focus with
/// it, which is `false`.
pub fn focus_kept(
    connection: &mut CdpConnection,
    session: &PageSession,
    token: &ElementToken,
    watch: FocusWatch,
    deadline: Deadline,
) -> Result<bool, AppError> {
    let reply = match &watch.promise {
        Some(promise) => connection
            .call(
                "Runtime.awaitPromise",
                json!({ "promiseObjectId": promise, "returnByValue": true }),
                Some(&session.session_id),
                deadline,
            )
            .map(|reply| reply.pointer("/result/value").cloned().unwrap_or_default()),
        None => call_on_element(
            connection,
            session,
            token,
            ElementCall {
                function: HOLDS_FOCUS,
                arguments: Vec::new(),
            },
            deadline,
        ),
    };
    release_focus_watch(connection, session, deadline);
    match reply {
        Ok(held) => Ok(held == Value::Bool(true)),
        Err(error) if is_protocol_error(&error) || error.code == ErrorCode::StaleRef => Ok(false),
        Err(error) => Err(error),
    }
}

/// Releases what a focus watch holds in the page, for a command that ends
/// before it reads its watch. Best effort: a cancelled command cannot send
/// it, and the next watch releases it then.
pub fn release_focus_watch(
    connection: &mut CdpConnection,
    session: &PageSession,
    deadline: Deadline,
) {
    let _ = connection.call(
        "Runtime.releaseObjectGroup",
        json!({ "objectGroup": WATCH_GROUP }),
        Some(&session.session_id),
        deadline,
    );
}

/// The page's `getEventListeners` (a DevTools command-line function), held
/// in the watch's object group, or `None` when the page has none.
fn listener_reader(
    connection: &mut CdpConnection,
    session: &PageSession,
    deadline: Deadline,
) -> Option<String> {
    let reply = connection
        .call(
            "Runtime.evaluate",
            json!({
                "expression": "getEventListeners",
                "includeCommandLineAPI": true,
                "objectGroup": WATCH_GROUP,
            }),
            Some(&session.session_id),
            deadline,
        )
        .ok()?;
    if reply.pointer("/result/type").and_then(Value::as_str) != Some("function") {
        return None;
    }
    reply
        .pointer("/result/objectId")
        .and_then(Value::as_str)
        .map(str::to_owned)
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
