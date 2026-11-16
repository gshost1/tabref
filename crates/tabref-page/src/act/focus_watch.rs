use std::time::Duration;

use serde_json::{Value, json};
use tabref_core::{AppError, Deadline, ErrorCode};

use super::element_call::ElementCall;
use super::element_errors::script_failed;
use super::remote_element::{call_on_element, with_element};
use crate::{CdpConnection, ElementToken, PageSession};
use tabref_cdp::is_protocol_error;

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

/// A watch, started in the page right after a focus, on whether the page
/// moves the focus away within a short window: the promise the page holds
/// it as, or `None` when nothing on the element's path listens for focus
/// events, so nothing is watched. [`focus_kept`](crate::focus_kept) reads
/// it.
#[derive(Debug)]
pub struct FocusWatch {
    pub(crate) promise: Option<String>,
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
