use serde_json::{Value, json};
use tabref_core::{AppError, Deadline};

use super::element_errors::script_failed;
use super::remote_element::with_element;
use crate::{CdpConnection, ElementToken, PageSession};

/// The selector for an element that takes a press of its own: a press that
/// lands on one inside a label or a `display: contents` wrapper acts on it,
/// not on the control the label or wrapper stands for.
pub(crate) const CONTROLS: &str = "a[href], area[href], button, input, select, textarea, summary, \
[role=button], [role=link], [role=checkbox], [role=radio], [role=switch], [role=menuitem], \
[role=tab], [contenteditable=''], [contenteditable=true], [tabindex]";

/// The first of a control's labels that has a box on screen and holds no
/// other control, which a press on the label would reach instead.
pub(crate) const DRAWN_LABEL: &str = r"function(controls) {
  const drawn = label => Array.from(label.getClientRects()).some(box => box.width > 0 && box.height > 0);
  const alone = label => Array.from(label.querySelectorAll(controls)).every(inner => inner === this);
  return Array.from(this.labels || []).find(label => drawn(label) && alone(label)) || null;
}";

/// The first element a `display: contents` node draws through: a child,
/// shadow child or slotted node with a box on screen, looking through
/// children that are `display: contents` themselves. A child that is or
/// holds a separate control is passed over for one that holds none; only
/// when the node holds one control and draws nothing else is that control's
/// child taken, as for a host that wraps its one inner `<button>`.
pub(crate) const DRAWN_CHILD: &str = r"function(controls) {
  const view = this.ownerDocument.defaultView;
  const drawn = el => Array.from(el.getClientRects()).some(box => box.width > 0 && box.height > 0);
  const children = node => {
    const own = node.localName === 'slot' ? Array.from(node.assignedNodes({ flatten: true })) : [];
    const shadow = node.shadowRoot ? Array.from(node.shadowRoot.childNodes) : [];
    return own.concat(shadow, Array.from(node.childNodes));
  };
  const held = new Set();
  const collect = node => {
    for (const child of children(node)) {
      if (child.nodeType !== 1) continue;
      if (child.matches(controls)) held.add(child);
      collect(child);
    }
  };
  collect(this);
  const inside = (node, root) => {
    for (let at = node; at; at = at.parentNode || at.host) if (at === root) return true;
    return false;
  };
  const holds = el => Array.from(held).some(control => inside(control, el));
  const find = (node, accept) => {
    for (const child of children(node)) {
      if (child.nodeType !== 1) continue;
      if (view.getComputedStyle(child).display === 'contents') {
        const deep = find(child, accept);
        if (deep) return deep;
      } else if (drawn(child) && accept(child)) {
        return child;
      }
    }
    return null;
  };
  return find(this, child => !holds(child)) || (held.size === 1 ? find(this, () => true) : null);
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
            "arguments": [{ "value": CONTROLS }],
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
