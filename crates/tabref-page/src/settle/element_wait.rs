use std::time::Duration;

use serde_json::{Value, json};
use tabref_cdp::is_timeout;
use tabref_core::{AppError, Deadline, ErrorCode};

use super::element_state::ElementState;
use super::state_wait::StateWait;
use crate::act::remote_element::with_element;
use crate::{CdpConnection, ElementToken, PageSession};

const RESERVE: Duration = Duration::from_millis(300);

const WAIT_STATE: &str = r"function(want, budget) {
  const el = this;
  const view = el.ownerDocument.defaultView;
  const up = node => node.parentElement || (node.parentNode && node.parentNode.host) || null;
  const shown = () => {
    if (!el.isConnected) return false;
    if (el.checkVisibility && !el.checkVisibility()) return false;
    const style = view.getComputedStyle(el);
    if (style.visibility === 'hidden' || style.visibility === 'collapse') return false;
    if (style.display === 'contents') return true;
    return Array.from(el.getClientRects()).some(box => box.width > 0 && box.height > 0);
  };
  const disabled = () => {
    const owner = el.closest('button, input, select, textarea, option, optgroup') || el;
    if (owner.matches(':disabled')) return true;
    for (let node = el; node; node = up(node)) {
      const aria = node.getAttribute && node.getAttribute('aria-disabled');
      if (aria === 'true') return true;
      if (aria === 'false') break;
    }
    return false;
  };
  const value = () => ('value' in el && el.value !== undefined && el.value !== null)
    ? String(el.value)
    : String(el.innerText !== undefined ? el.innerText : (el.textContent || '')).trim();
  const now = () => {
    if (!el.isConnected) return 'gone';
    if (want.state === 'value') return 'value ' + JSON.stringify(value());
    if (want.state === 'enabled') return disabled() ? 'disabled' : 'enabled';
    return shown() ? 'visible' : 'hidden';
  };
  const holds = () => {
    switch (want.state) {
      case 'gone': return !el.isConnected;
      case 'hidden': return !shown();
      case 'visible': return shown();
      case 'enabled': return el.isConnected && !disabled();
      case 'value': return el.isConnected && value() === want.value;
    }
    return false;
  };
  if (holds()) return { held: true, now: now() };
  return new Promise(resolve => {
    let done = false;
    let observer = null;
    let timer = 0;
    let limit = 0;
    const finish = held => {
      if (done) return;
      done = true;
      if (observer) observer.disconnect();
      el.removeEventListener('input', check);
      el.removeEventListener('change', check);
      clearInterval(timer);
      clearTimeout(limit);
      resolve({ held, now: now() });
    };
    const check = () => { if (holds()) finish(true); };
    observer = new MutationObserver(check);
    observer.observe(el.ownerDocument, { subtree: true, childList: true, attributes: true, characterData: true });
    el.addEventListener('input', check);
    el.addEventListener('change', check);
    timer = setInterval(check, 100);
    limit = setTimeout(() => finish(holds()), budget);
  });
}";

/// Waits within `deadline` for the token's element to reach `state`. The
/// page decides it: a mutation, an input or change event, or a check every
/// 100 ms for what neither reports (a value set from script, a style that
/// came from a class elsewhere) ends the wait the moment the state holds. A
/// state already held returns at once. An element whose node or document
/// is gone has reached `gone`, and any other state of it is `STALE_REF`.
pub fn wait_for_state(
    connection: &mut CdpConnection,
    session: &PageSession,
    token: &ElementToken,
    state: &ElementState,
    deadline: Deadline,
) -> Result<StateWait, AppError> {
    let budget = deadline.remaining().saturating_sub(RESERVE);
    let budget_ms = u64::try_from(budget.as_millis()).unwrap_or(u64::MAX);
    let waited = with_element(
        connection,
        session,
        token,
        deadline,
        |connection, object| {
            let reply = connection.call(
                "Runtime.callFunctionOn",
                json!({
                    "objectId": object,
                    "functionDeclaration": WAIT_STATE,
                    "arguments": [{ "value": state.wire() }, { "value": budget_ms }],
                    "returnByValue": true,
                    "awaitPromise": true,
                }),
                Some(&session.session_id),
                deadline,
            );
            match reply {
                Ok(reply) => Ok(read_outcome(&reply)),
                Err(error) if is_timeout(&error) => Ok(StateWait {
                    held: false,
                    now: "unknown".to_owned(),
                }),
                Err(error) => Err(error),
            }
        },
    );
    match waited {
        Err(error) if error.code == ErrorCode::StaleRef && *state == ElementState::Gone => {
            Ok(StateWait {
                held: true,
                now: "gone".to_owned(),
            })
        }
        other => other,
    }
}

fn read_outcome(reply: &Value) -> StateWait {
    let value = reply.pointer("/result/value");
    StateWait {
        held: value
            .and_then(|value| value.get("held"))
            .and_then(Value::as_bool)
            == Some(true),
        now: value
            .and_then(|value| value.get("now"))
            .and_then(Value::as_str)
            .unwrap_or("unknown")
            .to_owned(),
    }
}
