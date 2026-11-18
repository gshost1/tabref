use serde_json::{Value, json};
use tabref_core::{AppError, Deadline, DeliverySemantics, ErrorCode};

use super::drawn_element::drawn_element;
use super::element_call::ElementCall;
use super::element_errors::{node_error, stale};
use super::focus_target::FocusTarget;
use super::remote_element::{call_function, call_on_element, prove_document, with_element};
use crate::{CdpConnection, ElementToken, PageSession};
use tabref_cdp::is_protocol_error;

const FOCUS_CONTROL: &str = r"function() {
  return this.localName === 'label' && this.control ? this.control : null;
}";

const FOCUS_IN_PAGE: &str = r"function() {
  if (!this.isConnected) return 'gone';
  const doc = this.ownerDocument;
  const root = this.getRootNode ? this.getRootNode() : doc;
  const deep = () => {
    let active = doc.activeElement;
    while (active && active.shadowRoot && active.shadowRoot.activeElement) active = active.shadowRoot.activeElement;
    return active;
  };
  const before = deep();
  const near = root && root.activeElement;
  let heard = false;
  const note = () => { heard = true; };
  doc.addEventListener('focusin', note, true);
  doc.addEventListener('focusout', note, true);
  try { this.focus(); } finally {
    doc.removeEventListener('focusin', note, true);
    doc.removeEventListener('focusout', note, true);
  }
  const active = (root && root.activeElement) || doc.activeElement;
  if (active === this || (!!active && this.contains(active))) return 'focused';
  const moved = heard || deep() !== before || (root && root.activeElement) !== near;
  return moved ? 'moved' : 'unchanged';
}";

const FOCUSABLE: &str = r"function() {
  const index = this.getAttribute('tabindex');
  if (index !== null && !Number.isNaN(parseInt(index, 10))) return true;
  if (this.isContentEditable) return true;
  const html = this.namespaceURI === 'http://www.w3.org/1999/xhtml';
  if (html && (this.localName === 'a' || this.localName === 'area') && !this.hasAttribute('href')) return false;
  return this.tabIndex >= 0;
}";

const PAGE_FOCUSED: &str = "document.visibilityState === 'visible' && document.hasFocus()";

/// Moves the page's keyboard focus to `target`. Only the page's own focus
/// changes; the browser window is not raised. A node the browser refuses
/// as not focusable is `ACTION_NOT_SUPPORTED` `cdp_not_focusable`. A
/// shadow host is focused in the page, so the browser itself picks the
/// control it delegates to. A host whose focus lands nowhere in it, and
/// that moved nothing and fired no focus event, is refused the same way,
/// with nothing in the page changed; one whose page moved the focus on
/// (an inner control that blurs itself, a focus trap) counts as focused,
/// and the watch reports the move.
pub fn focus(
    connection: &mut CdpConnection,
    session: &PageSession,
    target: &FocusTarget,
    deadline: Deadline,
) -> Result<(), AppError> {
    let token = &target.token;
    if target.in_page {
        let held = call_on_element(
            connection,
            session,
            token,
            ElementCall {
                function: FOCUS_IN_PAGE,
                arguments: Vec::new(),
            },
            deadline,
        )?;
        return match held.as_str() {
            Some("focused" | "moved") => Ok(()),
            Some("gone") => Err(stale(token, "the node is no longer in its document")),
            _ => Err(not_focusable(token.backend_node_id, None)),
        };
    }
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

/// What the `focus` command focuses for `token`: the node itself, or the
/// control a `<label>` names. A node `DOM.focus` would refuse (no valid tab
/// index and not editable, or an HTML link or image-map area with no
/// address and no `tabindex`) is refused here, before anything in the page
/// changes. A shadow host, open or closed, is focused in the page before
/// any of that is checked: `DOM.focus` refuses a host that delegates its
/// focus, even one with a `tabindex`.
pub fn focus_target(
    connection: &mut CdpConnection,
    session: &PageSession,
    token: &ElementToken,
    deadline: Deadline,
) -> Result<FocusTarget, AppError> {
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
    if shadow_host(connection, session, &target, deadline)? {
        return Ok(FocusTarget {
            token: target,
            in_page: true,
        });
    }
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
    if focusable == Value::Bool(true) {
        return Ok(FocusTarget {
            token: target,
            in_page: false,
        });
    }
    Err(not_focusable(target.backend_node_id, None))
}

/// Whether the node hosts an author shadow tree, open or closed, as the
/// browser describes it; the browser's own shadow trees (inside an
/// `<input>` or a `<video>`) do not count.
fn shadow_host(
    connection: &mut CdpConnection,
    session: &PageSession,
    token: &ElementToken,
    deadline: Deadline,
) -> Result<bool, AppError> {
    prove_document(connection, session, token, deadline)?;
    let reply = connection
        .call(
            "DOM.describeNode",
            json!({ "backendNodeId": token.backend_node_id, "depth": 0, "pierce": true }),
            Some(&session.session_id),
            deadline,
        )
        .map_err(|error| node_error(error, token))?;
    Ok(reply
        .pointer("/node/shadowRoots")
        .and_then(Value::as_array)
        .is_some_and(|roots| {
            roots.iter().any(|root| {
                root.get("shadowRootType").and_then(Value::as_str) != Some("user-agent")
            })
        }))
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
