use std::time::{Duration, Instant};

use serde_json::{Value, json};
use tabref_core::{AppError, Deadline};

use super::blocker::Blocker;
use super::element_call::ElementCall;
use super::element_errors::stale;
use super::remote_element::{call_function, with_element};
use super::unactionable::Unactionable;
use crate::{CdpConnection, ElementToken, LocateIntent, PageSession};

const LONGEST_WAIT: Duration = Duration::from_millis(1500);
const POLL: Duration = Duration::from_millis(50);

const CHECK: &str = r"function(need) {
  const el = this.nodeType === 1 ? this : this.parentElement;
  if (!el || !el.isConnected) return { blocker: 'detached' };
  const up = node => node.parentElement || (node.parentNode && node.parentNode.host) || null;
  const view = el.ownerDocument.defaultView;
  for (let node = el; node; node = up(node)) {
    if (node.hasAttribute('inert')) return { blocker: 'hidden', detail: 'inside an inert section' };
    if (node.getAttribute('aria-hidden') === 'true') return { blocker: 'hidden', detail: 'aria-hidden' };
  }
  if (el.checkVisibility && !el.checkVisibility()) {
    let detail = 'not rendered';
    if (el.closest('details:not([open])')) detail = 'inside a closed section';
    for (let node = el; node; node = up(node)) {
      if (view.getComputedStyle(node).display === 'none') { detail = 'display:none'; break; }
    }
    return { blocker: 'hidden', detail };
  }
  const style = view.getComputedStyle(el);
  if (style.display === 'none') return { blocker: 'hidden', detail: 'display:none' };
  if (style.visibility === 'hidden' || style.visibility === 'collapse') return { blocker: 'hidden', detail: 'visibility:hidden' };
  const boxes = Array.from(el.getClientRects());
  if (style.display !== 'contents' && !boxes.some(box => box.width > 0 && box.height > 0)) return { blocker: 'hidden', detail: 'zero size' };
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

/// Checks that the node can take the action `intent` names: attached, shown
/// (rendered, with a box, not `visibility: hidden`, not inside an inert or
/// `aria-hidden` section), and, for anything but a hover, enabled (not
/// disabled, `aria-disabled` or inside a disabled fieldset); a fill also
/// needs a field that is not read-only. A node that is not ready is checked
/// again every 50 ms for up to 1.5 s, at most half of what is left of
/// `deadline`, on one resolved object. Returns `None` when the node is
/// ready, or why it still was not when the wait ended. A detached node is
/// `STALE_REF` at once: it does not come back.
pub fn wait_actionable(
    connection: &mut CdpConnection,
    session: &PageSession,
    token: &ElementToken,
    intent: LocateIntent,
    deadline: Deadline,
) -> Result<Option<Unactionable>, AppError> {
    let started = Instant::now();
    let budget = LONGEST_WAIT.min(deadline.remaining() / 2);
    with_element(
        connection,
        session,
        token,
        deadline,
        |connection, object| loop {
            let state = call_function(connection, session, object, check(intent), deadline)?;
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
            if waited + POLL >= budget {
                return Ok(Some(Unactionable {
                    blocker,
                    detail: state
                        .get("detail")
                        .and_then(Value::as_str)
                        .unwrap_or(wire)
                        .to_owned(),
                    waited_ms: u64::try_from(waited.as_millis()).unwrap_or(u64::MAX),
                }));
            }
            std::thread::sleep(POLL);
        },
    )
}

fn check(intent: LocateIntent) -> ElementCall {
    let enabled = !matches!(intent, LocateIntent::Hover | LocateIntent::Read);
    let editable = intent == LocateIntent::Fill;
    ElementCall {
        function: CHECK,
        arguments: vec![json!({ "enabled": enabled, "editable": editable })],
    }
}

#[cfg(test)]
#[path = "actionable_tests.rs"]
mod tests;
