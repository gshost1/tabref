use std::sync::LazyLock;
use std::time::Duration;

use serde_json::{Value, json};
use tabref_cdp::{is_protocol_error, is_timeout};
use tabref_core::{AppError, Deadline, ErrorCode};

use super::element_state::ElementState;
use super::state_wait::StateWait;
use crate::act::action_need::ActionNeed;
use crate::act::drawn_element::CONTROLS;
use crate::act::element_errors::{script_failed, stale};
use crate::act::gate_script::GATE;
use crate::act::remote_element::{prove_document, with_element};
use crate::{CdpConnection, ElementToken, PageSession};

const RESERVE: Duration = Duration::from_millis(50);

const WAIT_STATE: &str = r"function(want, budget, shown, press, controls) {
  const gate = __GATE__;
  const node = this;
  const el = node.nodeType === 1 ? node : node.parentElement;
  const fields = ['input', 'textarea', 'select'];
  const value = () => fields.includes(el.localName)
    ? String(el.value)
    : String(el.innerText !== undefined ? el.innerText : (el.textContent || '')).trim();
  const now = () => {
    const seen = gate(node, shown, controls);
    if (seen.blocker === 'detached') return 'gone';
    if (want.state === 'value') return 'value ' + JSON.stringify(value());
    if (seen.blocker) return 'hidden';
    if (want.state !== 'enabled') return 'visible';
    return gate(node, press, controls).blocker ? 'disabled' : 'enabled';
  };
  const absent = state => state === 'gone' || state === 'hidden';
  const holds = state => {
    switch (want.state) {
      case 'gone': case 'hidden': return absent(state);
      case 'value': return state === 'value ' + JSON.stringify(want.value);
    }
    return state === want.state;
  };
  const settled = state => holds(state) || state === 'gone';
  const first = now();
  if (settled(first)) return { held: holds(first), now: first };
  return new Promise(resolve => {
    let done = false;
    let observer = null;
    let timer = 0;
    let limit = 0;
    const finish = state => {
      if (done) return;
      done = true;
      if (observer) observer.disconnect();
      el.removeEventListener('input', check);
      el.removeEventListener('change', check);
      clearInterval(timer);
      clearTimeout(limit);
      resolve({ held: holds(state), now: state });
    };
    const check = () => { const state = now(); if (settled(state)) finish(state); };
    observer = new MutationObserver(check);
    observer.observe(node.ownerDocument, { subtree: true, childList: true, attributes: true, characterData: true });
    el.addEventListener('input', check);
    el.addEventListener('change', check);
    timer = setInterval(check, 100);
    limit = setTimeout(() => finish(now()), budget);
  });
}";

static WAIT_SCRIPT: LazyLock<String> = LazyLock::new(|| WAIT_STATE.replace("__GATE__", GATE));

/// Waits within `deadline` for the token's element to reach `state`, by the
/// action gate's own rules: `visible` is what the gate does not find hidden
/// (an `aria-hidden` or `inert` section hides, a `display: contents`
/// element shows what it draws), `enabled` is visible and not disabled, so
/// the next action accepts it. `gone` and `hidden` both hold once the
/// element is no longer shown, removed or not; `now` says which. A value is
/// the field value of an input, text area or select, and the text of any
/// other element. The page decides it: a mutation, an input or change
/// event, or a check every 100 ms for what neither reports ends the wait the
/// moment the state holds. A state already held returns at once. An element
/// whose node or document is gone (a navigation during the wait replaces
/// the document) has reached `gone`, and any other state of it is
/// `STALE_REF`.
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
                    "functionDeclaration": WAIT_SCRIPT.as_str(),
                    "arguments": [
                        { "value": state.wire() },
                        { "value": budget_ms },
                        { "value": ActionNeed::Shown.spec() },
                        { "value": ActionNeed::Press.spec() },
                        { "value": CONTROLS },
                    ],
                    "returnByValue": true,
                    "awaitPromise": true,
                }),
                Some(&session.session_id),
                deadline,
            );
            match reply {
                Ok(reply) => match reply.get("exceptionDetails") {
                    Some(exception) => {
                        prove_document(connection, session, token, deadline)?;
                        Err(script_failed(exception))
                    }
                    None => read_outcome(&reply, token),
                },
                Err(error) if is_timeout(&error) => Ok(StateWait {
                    held: false,
                    now: "unknown".to_owned(),
                }),
                Err(error) if is_protocol_error(&error) => {
                    prove_document(connection, session, token, deadline)?;
                    Err(error)
                }
                Err(error) => Err(error),
            }
        },
    );
    match waited {
        Err(error) if error.code == ErrorCode::StaleRef && state.absent() => Ok(StateWait {
            held: true,
            now: "gone".to_owned(),
        }),
        other => other,
    }
}

fn read_outcome(reply: &Value, token: &ElementToken) -> Result<StateWait, AppError> {
    let value = reply.pointer("/result/value");
    let outcome = StateWait {
        held: value
            .and_then(|value| value.get("held"))
            .and_then(Value::as_bool)
            == Some(true),
        now: value
            .and_then(|value| value.get("now"))
            .and_then(Value::as_str)
            .unwrap_or("unknown")
            .to_owned(),
    };
    if !outcome.held && outcome.now == "gone" {
        return Err(stale(token, "the node is no longer in its document"));
    }
    Ok(outcome)
}
