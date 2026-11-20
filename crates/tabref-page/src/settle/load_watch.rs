//! Watches a page load for what the page does after `load`. Before the
//! navigation, network events are turned on and a script is registered for
//! the new document that counts DOM mutations of the top document from
//! `DOMContentLoaded` on, so mutations of the parse itself never count. It
//! hangs off a registered symbol of `window`, which page scripts that list
//! the window's keys never see, and is removed once read.

use std::time::Duration;

use serde_json::{Value, json};
use tabref_core::{AppError, Deadline};

use crate::{CdpConnection, PageSession};
use tabref_cdp::is_timeout;

const WATCH_KEY: &str = "tabref.load-watch";
const NEAR_EMPTY_ELEMENTS: usize = 300;
const NEAR_EMPTY_CHARS: usize = 200;
const CLEANUP_WAIT: Duration = Duration::from_millis(20);
const START_WAIT: Duration = Duration::from_millis(200);

const LOAD_OBSERVER: &str = r"(() => {
  if (window !== window.top) return;
  const key = Symbol.for('KEY');
  if (window[key]) return;
  const state = { count: 0, last: 0 };
  window[key] = state;
  const start = () => {
    if (state.observer) return;
    state.observer = new MutationObserver(records => { state.count += records.length; state.last = performance.now(); });
    state.observer.observe(document, { subtree: true, childList: true, attributes: true, characterData: true });
  };
  if (document.readyState === 'loading') document.addEventListener('DOMContentLoaded', start, { once: true });
  else start();
})()";

const LOAD_STATUS: &str = r"(() => {
  const state = window[Symbol.for('KEY')];
  const now = performance.now();
  let since = null;
  if (state && state.observer) {
    const pending = state.observer.takeRecords().length;
    if (pending) { state.count += pending; state.last = now; }
    if (state.count) since = now - state.last;
  }
  const body = document.body;
  let empty = !body;
  if (body && body.getElementsByTagName('*').length < ELEMENTS * 4) {
    const frame = 'header, nav, footer, [role=banner], [role=navigation], [role=contentinfo]';
    const controls = 'input:not([type=hidden]), select, textarea, button, [contenteditable], [role=button], [role=textbox]';
    const outer = [...body.querySelectorAll(frame)].filter(el => !el.parentElement.closest(frame));
    const inside = outer.reduce((sum, el) => sum + el.getElementsByTagName('*').length + 1, 0);
    if (body.getElementsByTagName('*').length - inside < ELEMENTS) {
      const chars = outer.reduce((left, el) => left - el.innerText.trim().length, body.innerText.trim().length);
      const control = [...body.querySelectorAll(controls)].some(el => !el.closest(frame));
      empty = chars < CHARS && !control;
    }
  }
  return { wall: performance.timeOrigin + now, since, empty, watched: !!state };
})()";

const LOAD_STOP: &str = r"(() => {
  const key = Symbol.for('KEY');
  const state = window[key];
  if (state && state.observer) state.observer.disconnect();
  delete window[key];
  return true;
})()";

/// A page load being watched: the page session and the id of the script
/// registered for its new document.
#[derive(Debug, Clone)]
pub struct LoadWatch {
    pub(crate) session: PageSession,
    pub(crate) script: Option<String>,
}

/// The page as one quick read saw it: the page's clock in milliseconds
/// since the epoch, how long ago the DOM last changed (`None` when it has
/// not changed since `DOMContentLoaded`, or was not watched), and whether
/// the page is near-empty: outside its header, navigation and footer
/// (`header`, `nav`, `footer` and their ARIA roles), fewer than 300
/// elements, fewer than 200 characters of rendered text and no field or
/// button. An app shell whose only search box sits in its header is
/// near-empty until its content renders.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct LoadStatus {
    pub(crate) wall_ms: f64,
    pub(crate) since_mutation: Option<Duration>,
    pub(crate) near_empty: bool,
}

impl LoadWatch {
    /// Starts watching `session` before a navigation. A browser that
    /// refuses the script still gets a watch, of the network alone. Each
    /// call waits at most 200 ms, as the page being left may be too busy
    /// to answer; a call that times out gives the watch up, so the
    /// navigation keeps its time.
    pub fn start(
        connection: &mut CdpConnection,
        session: &PageSession,
        deadline: Deadline,
    ) -> Result<Self, AppError> {
        let id = Some(session.session_id.as_str());
        let quick = || deadline.capped(START_WAIT);
        connection.call("Page.enable", json!({}), id, quick())?;
        let source = LOAD_OBSERVER.replace("KEY", WATCH_KEY);
        let script = match connection.call(
            "Page.addScriptToEvaluateOnNewDocument",
            json!({ "source": source }),
            id,
            quick(),
        ) {
            Ok(reply) => reply
                .get("identifier")
                .and_then(Value::as_str)
                .map(str::to_owned),
            Err(error) if is_timeout(&error) => return Err(error),
            Err(_) => None,
        };
        let mut watch = Self {
            session: session.clone(),
            script,
        };
        if let Err(error) = connection.call("Network.enable", json!({}), id, quick()) {
            watch.forget_script(connection, deadline);
            return Err(error);
        }
        let session_id = session.session_id.clone();
        connection.discard_events(|event| is_network_event(event, &session_id));
        Ok(watch)
    }

    /// Takes the script off new documents, so later navigations of the
    /// tab run without it.
    pub(crate) fn forget_script(&mut self, connection: &mut CdpConnection, deadline: Deadline) {
        if let Some(identifier) = self.script.take() {
            let _ = connection.call(
                "Page.removeScriptToEvaluateOnNewDocument",
                json!({ "identifier": identifier }),
                Some(&self.session.session_id),
                deadline.capped(CLEANUP_WAIT),
            );
        }
    }

    pub(crate) fn status(
        &self,
        connection: &mut CdpConnection,
        deadline: Deadline,
    ) -> Result<LoadStatus, AppError> {
        let expression = LOAD_STATUS
            .replace("KEY", WATCH_KEY)
            .replace("ELEMENTS", &NEAR_EMPTY_ELEMENTS.to_string())
            .replace("CHARS", &NEAR_EMPTY_CHARS.to_string());
        let reply = connection.call(
            "Runtime.evaluate",
            json!({ "expression": expression, "returnByValue": true }),
            Some(&self.session.session_id),
            deadline,
        )?;
        parse_status(&reply).ok_or_else(|| AppError::internal("the page's context changed"))
    }

    /// Ends the watch: the script is taken off, the page's observer is
    /// stopped, network events are turned off and their backlog dropped.
    /// The page stops its observer whenever it is free, unawaited, and each
    /// call to the browser is waited for at most 20 ms, so a busy page adds
    /// next to nothing. Failures are ignored, as the page may be gone.
    pub fn stop(mut self, connection: &mut CdpConnection, deadline: Deadline) {
        self.forget_script(connection, deadline);
        let id = Some(self.session.session_id.as_str());
        let expression = LOAD_STOP.replace("KEY", WATCH_KEY);
        if let Ok(sent) = connection.send(
            "Runtime.evaluate",
            json!({ "expression": expression, "returnByValue": true }),
            id,
            deadline,
        ) {
            connection.abandon(sent);
        }
        let _ = connection.call(
            "Network.disable",
            json!({}),
            id,
            deadline.capped(CLEANUP_WAIT),
        );
        let session_id = self.session.session_id.clone();
        connection.discard_events(|event| is_network_event(event, &session_id));
    }
}

pub(crate) fn parse_status(reply: &Value) -> Option<LoadStatus> {
    if reply.get("exceptionDetails").is_some() {
        return None;
    }
    let value = reply.pointer("/result/value")?;
    let since_mutation = value
        .get("since")
        .and_then(Value::as_f64)
        .map(|ms| Duration::from_secs_f64(ms.max(0.0) / 1_000.0));
    Some(LoadStatus {
        wall_ms: value.get("wall").and_then(Value::as_f64)?,
        since_mutation,
        near_empty: value.get("empty").and_then(Value::as_bool) == Some(true),
    })
}

pub(crate) fn is_network_event(event: &crate::CdpEvent, session_id: &str) -> bool {
    event.session_id.as_deref() == Some(session_id) && event.method.starts_with("Network.")
}
