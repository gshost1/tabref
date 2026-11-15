use std::time::{Duration, Instant};

use serde_json::{Value, json};
use tabref_core::{AppError, Deadline};

use super::action_need::ActionNeed;
use super::blocker::Blocker;
use super::drawn_element::CONTROLS;
use super::element_call::ElementCall;
use super::element_errors::stale;
use super::remote_element::{call_function, with_element};
use super::unactionable::Unactionable;
use crate::{CdpConnection, ElementToken, PageSession};

const LONGEST_WAIT: Duration = Duration::from_millis(1500);
const POLL: Duration = Duration::from_millis(50);

const CHECK: &str = r"function(need, controls) {
  const el = this.nodeType === 1 ? this : this.parentElement;
  if (!el || !el.isConnected) return { blocker: 'detached' };
  const up = node => node.parentElement || (node.parentNode && node.parentNode.host) || null;
  const view = el.ownerDocument.defaultView;
  for (let node = el; node; node = up(node)) {
    if (node.hasAttribute('inert')) return { blocker: 'hidden', detail: 'inside an inert section' };
    if (node.getAttribute('aria-hidden') === 'true') return { blocker: 'hidden', detail: 'aria-hidden' };
  }
  if (need.box && el.localName === 'area') return { blocker: 'unpressable', detail: 'image-map area' };
  const area = boxes => Array.from(boxes).some(box => box.width > 0 && box.height > 0);
  const textArea = text => { const range = text.ownerDocument.createRange(); range.selectNodeContents(text); return area(range.getClientRects()); };
  const children = node => {
    const own = node.localName === 'slot' ? Array.from(node.assignedNodes({ flatten: true })) : [];
    const shadow = node.shadowRoot ? Array.from(node.shadowRoot.childNodes) : [];
    return own.concat(shadow, Array.from(node.childNodes));
  };
  const drawn = node => {
    const style = view.getComputedStyle(node);
    if (style.display === 'contents') return children(node).some(child => child.nodeType === 1 ? drawn(child) : child.nodeType === 3 && textArea(child));
    if (node.checkVisibility && !node.checkVisibility()) return false;
    if (style.visibility === 'hidden' || style.visibility === 'collapse') return false;
    return !need.box || area(node.getClientRects());
  };
  const style = view.getComputedStyle(el);
  if (style.display === 'contents') {
    if (!drawn(el)) return { blocker: 'hidden', detail: 'display:contents with nothing shown' };
  } else {
    if (el.checkVisibility && !el.checkVisibility()) {
      let detail = 'not rendered';
      if (el.closest('details:not([open])')) detail = 'inside a closed section';
      for (let node = el; node; node = up(node)) {
        if (view.getComputedStyle(node).display === 'none') { detail = 'display:none'; break; }
      }
      return { blocker: 'hidden', detail };
    }
    if (style.display === 'none') return { blocker: 'hidden', detail: 'display:none' };
    if (style.visibility === 'hidden' || style.visibility === 'collapse') return { blocker: 'hidden', detail: 'visibility:hidden' };
    if (need.box && !area(el.getClientRects())) {
      const alone = label => Array.from(label.querySelectorAll(controls)).every(inner => inner === el);
      const labelled = need.label && Array.from(el.labels || []).some(label => area(label.getClientRects()) && alone(label));
      if (!labelled) return { blocker: 'zero', detail: 'zero size' };
    }
  }
  if (need.enabled) {
    const owner = el.closest('button, input, select, textarea, option, optgroup') || el;
    if (owner.matches(':disabled')) {
      const detail = owner.hasAttribute('disabled') ? 'disabled'
        : owner.closest('fieldset[disabled]') ? 'inside a disabled fieldset' : 'inside a disabled group';
      return { blocker: 'disabled', detail };
    }
    for (let node = el; node; node = up(node)) {
      const aria = node.getAttribute('aria-disabled');
      if (aria === 'true') return { blocker: 'disabled', detail: 'aria-disabled' };
      if (aria === 'false') break;
    }
  }
  if (need.editable) {
    const field = el.localName === 'input' || el.localName === 'textarea';
    if (field && el.readOnly) return { blocker: 'readonly', detail: 'read-only' };
    if (el.getAttribute('aria-readonly') === 'true') return { blocker: 'readonly', detail: 'aria-readonly' };
  }
  return { ready: true };
}";

/// Checks that the node can take an action that needs `need`: attached,
/// shown (rendered, not `visibility: hidden`, not inside an inert or
/// `aria-hidden` section; a `display: contents` node by what its children
/// draw), with a box when the action presses it, enabled unless it only
/// hovers or shows, and not read-only for a fill. A node that is not ready
/// is checked again every 50 ms for up to 1.5 s, at most half of what is
/// left of `deadline`, on one resolved object. A node with no size does not
/// grow one by waiting, so it is reported at once, as a detached node is
/// `STALE_REF` at once. Returns `None` when the node is ready, or why it was
/// not.
pub fn wait_actionable(
    connection: &mut CdpConnection,
    session: &PageSession,
    token: &ElementToken,
    need: ActionNeed,
    deadline: Deadline,
) -> Result<Option<Unactionable>, AppError> {
    let started = Instant::now();
    let budget = LONGEST_WAIT.min(deadline.remaining() / 2);
    with_element(
        connection,
        session,
        token,
        deadline,
        |connection, object| {
            let mut slept = false;
            loop {
                let state = call_function(connection, session, object, check(need), deadline)?;
                let Some(wire) = state.get("blocker").and_then(Value::as_str) else {
                    return Ok(None);
                };
                if wire == "detached" {
                    return Err(stale(token, "the node is no longer in its document"));
                }
                let Some(blocker) = Blocker::from_wire(wire) else {
                    return Ok(None);
                };
                let waited = started.elapsed();
                if blocker.is_settled() || waited + POLL >= budget {
                    let detail = state.get("detail").and_then(Value::as_str);
                    return Ok(Some(Unactionable {
                        blocker,
                        detail: detail.unwrap_or(wire).to_owned(),
                        waited_ms: if slept {
                            u64::try_from(waited.as_millis()).unwrap_or(u64::MAX)
                        } else {
                            0
                        },
                    }));
                }
                std::thread::sleep(POLL);
                slept = true;
            }
        },
    )
}

fn check(need: ActionNeed) -> ElementCall {
    ElementCall {
        function: CHECK,
        arguments: vec![need.spec(), json!(CONTROLS)],
    }
}

#[cfg(test)]
#[path = "actionable_tests.rs"]
mod tests;
