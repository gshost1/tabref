use serde_json::{Value, json};
use tabref_core::{AppError, Deadline};

use super::element_errors::script_failed;
use super::remote_element::with_element;
use crate::{CdpConnection, ElementToken, PageSession};

/// The first of a control's labels that has a box on screen.
pub(crate) const DRAWN_LABEL: &str = r"function() {
  const drawn = label => Array.from(label.getClientRects()).some(box => box.width > 0 && box.height > 0);
  return Array.from(this.labels || []).find(drawn) || null;
}";

/// The first element a `display: contents` node draws through: a child,
/// shadow child or slotted node with a box on screen, looking through
/// children that are `display: contents` themselves.
pub(crate) const DRAWN_CHILD: &str = r"function() {
  const view = this.ownerDocument.defaultView;
  const drawn = el => Array.from(el.getClientRects()).some(box => box.width > 0 && box.height > 0);
  const children = node => {
    const own = node.localName === 'slot' ? Array.from(node.assignedNodes({ flatten: true })) : [];
    const shadow = node.shadowRoot ? Array.from(node.shadowRoot.childNodes) : [];
    return own.concat(shadow, Array.from(node.childNodes));
  };
  const find = node => {
    for (const child of children(node)) {
      if (child.nodeType !== 1) continue;
      if (view.getComputedStyle(child).display === 'contents') {
        const deep = find(child);
        if (deep) return deep;
      } else if (drawn(child)) {
        return child;
      }
    }
    return null;
  };
  return find(this);
}";

/// Runs `function` (one of this module's lookups) on the resolved node
/// `object_id` and returns the token of the element it found, in the same
/// document as `token`, or `None` when it found none.
pub(crate) fn drawn_element(
    connection: &mut CdpConnection,
    session: &PageSession,
    (object_id, function): (&str, &'static str),
    token: &ElementToken,
    deadline: Deadline,
) -> Result<Option<ElementToken>, AppError> {
    let reply = connection.call(
        "Runtime.callFunctionOn",
        json!({
            "objectId": object_id,
            "functionDeclaration": function,
            "returnByValue": false,
            "awaitPromise": false,
        }),
        Some(&session.session_id),
        deadline,
    )?;
    if let Some(exception) = reply.get("exceptionDetails") {
        return Err(script_failed(exception));
    }
    let Some(found) = reply.pointer("/result/objectId").and_then(Value::as_str) else {
        return Ok(None);
    };
    let described = connection.call(
        "DOM.describeNode",
        json!({ "objectId": found }),
        Some(&session.session_id),
        deadline,
    );
    let _ = connection.call(
        "Runtime.releaseObject",
        json!({ "objectId": found }),
        Some(&session.session_id),
        deadline,
    );
    Ok(described?
        .pointer("/node/backendNodeId")
        .and_then(Value::as_i64)
        .map(|backend_node_id| ElementToken {
            backend_node_id,
            ..token.clone()
        }))
}

/// The token of the first element the token's node draws through when it
/// has no box of its own ([`DRAWN_CHILD`]), or `None`.
pub(crate) fn drawn_child(
    connection: &mut CdpConnection,
    session: &PageSession,
    token: &ElementToken,
    deadline: Deadline,
) -> Result<Option<ElementToken>, AppError> {
    with_element(
        connection,
        session,
        token,
        deadline,
        |connection, object| {
            drawn_element(connection, session, (object, DRAWN_CHILD), token, deadline)
        },
    )
}

#[cfg(test)]
#[path = "drawn_element_tests.rs"]
mod tests;
