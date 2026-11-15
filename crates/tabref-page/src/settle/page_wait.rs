//! Waits that end the moment they can decide. Text is watched in the page:
//! a `MutationObserver` re-checks the rendered text after each batch of DOM
//! changes, so the wait returns within a task of the text showing or going.
//! A navigation replaces the watched document; the wait then starts again
//! on the new one as soon as it commits. An address is checked again on
//! each navigation event.

use std::time::Duration;

use serde_json::{Value, json};
use tabref_core::{AppError, Deadline};

use crate::{CdpConnection, CdpEvent, PageInfo, PageSession};
use tabref_cdp::{is_protocol_error, is_timeout};

const RECOMMIT: Duration = Duration::from_millis(500);

const TEXT_WAIT: &str = r"(want, present, ms) => new Promise(resolve => {
  const needle = String(want).toLowerCase();
  const holds = () => {
    const body = document.body;
    const shown = body ? body.innerText.toLowerCase().includes(needle) : false;
    return shown === present;
  };
  if (holds()) return resolve(true);
  let done = false;
  let queued = false;
  const finish = (value) => {
    if (done) return;
    done = true;
    observer.disconnect();
    clearTimeout(timer);
    resolve(value);
  };
  const observer = new MutationObserver(() => {
    if (queued) return;
    queued = true;
    Promise.resolve().then(() => { queued = false; if (holds()) finish(true); });
  });
  observer.observe(document, { subtree: true, childList: true, characterData: true, attributes: true });
  const timer = setTimeout(() => finish(false), ms);
})";

/// Waits until the page's rendered text contains `text` (ignoring case),
/// or no longer does when `present` is false. `false` means the deadline
/// passed first.
pub fn wait_for_text(
    connection: &mut CdpConnection,
    session: &PageSession,
    text: &str,
    present: bool,
    deadline: Deadline,
) -> Result<bool, AppError> {
    while !deadline.is_expired() {
        let expression = format!(
            "({TEXT_WAIT})({}, {present}, {})",
            json!(text),
            deadline.remaining_ms()
        );
        let reply = connection.call(
            "Runtime.evaluate",
            json!({ "expression": expression, "awaitPromise": true, "returnByValue": true }),
            Some(&session.session_id),
            deadline,
        );
        match reply {
            Ok(reply) if reply.get("exceptionDetails").is_none() => {
                return Ok(reply.pointer("/result/value").and_then(Value::as_bool) == Some(true));
            }
            Ok(_) => recommitted(connection, session, deadline)?,
            Err(error) if is_timeout(&error) => return Ok(false),
            Err(error) if is_protocol_error(&error) => {
                recommitted(connection, session, deadline)?;
            }
            Err(error) => return Err(error),
        }
    }
    Ok(false)
}

/// Waits until the tab's address contains `part`, checking at once and
/// again after each navigation of the tab. Returns the address, or `None`
/// when the deadline passed first.
pub fn wait_for_url(
    connection: &mut CdpConnection,
    session: &PageSession,
    part: &str,
    deadline: Deadline,
) -> Result<Option<String>, AppError> {
    connection.call(
        "Page.enable",
        json!({}),
        Some(&session.session_id),
        deadline,
    )?;
    loop {
        let info = PageInfo::read(connection, &session.target_id, deadline)?;
        if info.url.contains(part) {
            return Ok(Some(info.url));
        }
        let id = session.session_id.clone();
        match connection.wait_for_event(|event| is_navigation(event, &id), deadline) {
            Ok(_) => {}
            Err(error) if is_timeout(&error) => return Ok(None),
            Err(error) => return Err(error),
        }
    }
}

fn recommitted(
    connection: &mut CdpConnection,
    session: &PageSession,
    deadline: Deadline,
) -> Result<(), AppError> {
    let id = session.session_id.clone();
    match connection.wait_for_event(
        |event| {
            event.session_id.as_deref() == Some(id.as_str())
                && matches!(
                    event.method.as_str(),
                    "Page.domContentEventFired" | "Page.loadEventFired"
                )
        },
        deadline.capped(RECOMMIT),
    ) {
        Ok(_) => Ok(()),
        Err(error) if is_timeout(&error) => Ok(()),
        Err(error) => Err(error),
    }
}

fn is_navigation(event: &CdpEvent, session_id: &str) -> bool {
    event.session_id.as_deref() == Some(session_id)
        && (event.method == "Page.navigatedWithinDocument"
            || (event.method == "Page.frameNavigated"
                && event.params.pointer("/frame/parentId").is_none()))
}
