use std::collections::{HashSet, VecDeque};

use serde_json::{Value, json};
use tabref_core::{AppError, Deadline, ErrorCode};

use super::cdp_event::CdpEvent;
use super::incoming::Incoming;
use super::protocol_error::{protocol_error, target_gone};
use crate::transport::cdp_transport::CdpTransport;
use crate::transport::transport_error::transport_failed;

const MAX_BUFFERED_EVENTS: usize = 1024;
const MAX_DETACHED_SESSIONS: usize = 1024;
const DETACHED: &str = "Target.detachedFromTarget";
const DIALOG_OPENING: &str = "Page.javascriptDialogOpening";

/// A CDP client over one transport. Requests carry monotonic ids; events
/// that arrive while a reply is awaited are buffered, oldest dropped first
/// once the buffer holds 1024. The connection remembers every flattened
/// session the browser announced as detached: a request on one fails at once
/// as `cdp_target_gone`, and so does a request whose session detaches while
/// its reply is awaited, since a closing target may never answer.
pub struct CdpConnection {
    transport: Box<dyn CdpTransport>,
    next_id: u64,
    events: VecDeque<CdpEvent>,
    dropped_events: u64,
    detached_sessions: HashSet<String>,
    answer_dialogs: bool,
}

impl CdpConnection {
    pub fn new(transport: Box<dyn CdpTransport>) -> Self {
        Self {
            transport,
            next_id: 1,
            events: VecDeque::new(),
            dropped_events: 0,
            detached_sessions: HashSet::new(),
            answer_dialogs: true,
        }
    }

    /// Whether a JavaScript dialog that opens while this connection waits is
    /// accepted at once (the default). A page blocked on `alert()` answers no
    /// other request, so an unanswered dialog stalls every later call; the
    /// `Page.javascriptDialogOpening` event stays buffered so the caller can
    /// report what the dialog said.
    pub fn set_answer_dialogs(&mut self, answer: bool) {
        self.answer_dialogs = answer;
    }

    /// Sends `method` and waits for its reply. A flattened target session is
    /// addressed with `session_id`. A CDP error reply becomes an adapter
    /// error carrying the CDP message in `platform_detail`.
    pub fn call(
        &mut self,
        method: &str,
        params: Value,
        session_id: Option<&str>,
        deadline: Deadline,
    ) -> Result<Value, AppError> {
        if session_id.is_some_and(|session| self.detached_sessions.contains(session)) {
            return Err(session_detached(method));
        }
        let id = self.next_id;
        self.next_id += 1;
        let mut request = json!({ "id": id, "method": method, "params": params });
        if let Some(session_id) = session_id {
            request["sessionId"] = Value::String(session_id.to_owned());
        }
        self.transport.send_text(&request.to_string(), deadline)?;
        loop {
            let text = self.transport.receive_text(deadline)?;
            match Incoming::parse(&text) {
                Some(Incoming::Reply { id: reply_id, body }) if reply_id == id => {
                    return reply_result(method, body);
                }
                Some(Incoming::UnreadableReply { id: reply_id }) if reply_id == id => {
                    return Err(reply_unreadable(method));
                }
                Some(Incoming::Event(event)) => {
                    self.answer_dialog(&event, deadline)?;
                    self.buffer(event);
                    if session_id.is_some_and(|session| self.detached_sessions.contains(session)) {
                        return Err(session_detached(method));
                    }
                }
                Some(
                    Incoming::Reply { .. }
                    | Incoming::UnreadableReply { .. }
                    | Incoming::Unrecognized,
                ) => {}
                None => return Err(transport_failed(method, "unparseable DevTools frame")),
            }
        }
    }

    /// Returns the first event, buffered or newly read, that `matches`,
    /// leaving every other buffered event in place.
    pub fn wait_for_event(
        &mut self,
        mut matches: impl FnMut(&CdpEvent) -> bool,
        deadline: Deadline,
    ) -> Result<CdpEvent, AppError> {
        if let Some(index) = self.events.iter().position(&mut matches) {
            if let Some(event) = self.events.remove(index) {
                return Ok(event);
            }
        }
        loop {
            let text = self.transport.receive_text(deadline)?;
            match Incoming::parse(&text) {
                Some(Incoming::Event(event)) => {
                    self.answer_dialog(&event, deadline)?;
                    if matches(&event) {
                        return Ok(event);
                    }
                    self.buffer(event);
                }
                Some(_) => {}
                None => return Err(transport_failed("event", "unparseable DevTools frame")),
            }
        }
    }

    /// Drains every buffered event.
    pub fn take_events(&mut self) -> Vec<CdpEvent> {
        self.events.drain(..).collect()
    }

    /// Removes and returns, in arrival order, every buffered event that
    /// `matches`, leaving the rest buffered.
    pub fn drain_events(&mut self, mut matches: impl FnMut(&CdpEvent) -> bool) -> Vec<CdpEvent> {
        let mut drained = Vec::new();
        self.events.retain(|event| {
            if matches(event) {
                drained.push(event.clone());
                return false;
            }
            true
        });
        drained
    }

    /// Discards buffered events that `matches`.
    pub fn discard_events(&mut self, mut matches: impl FnMut(&CdpEvent) -> bool) {
        self.events.retain(|event| !matches(event));
    }

    /// How many events were discarded because the buffer was full.
    pub fn dropped_event_count(&self) -> u64 {
        self.dropped_events
    }

    fn answer_dialog(&mut self, event: &CdpEvent, deadline: Deadline) -> Result<(), AppError> {
        if !self.answer_dialogs || event.method != DIALOG_OPENING {
            return Ok(());
        }
        let Some(session_id) = event.session_id.as_deref() else {
            return Ok(());
        };
        let id = self.next_id;
        self.next_id += 1;
        let prompt = event
            .params
            .get("defaultPrompt")
            .and_then(Value::as_str)
            .unwrap_or_default();
        let request = json!({
            "id": id,
            "method": "Page.handleJavaScriptDialog",
            "params": { "accept": true, "promptText": prompt },
            "sessionId": session_id,
        });
        self.transport.send_text(&request.to_string(), deadline)
    }

    fn buffer(&mut self, event: CdpEvent) {
        if event.method == DETACHED {
            if let Some(session) = event.params.get("sessionId").and_then(Value::as_str) {
                if self.detached_sessions.len() >= MAX_DETACHED_SESSIONS {
                    self.detached_sessions.clear();
                }
                self.detached_sessions.insert(session.to_owned());
            }
        }
        if self.events.len() >= MAX_BUFFERED_EVENTS {
            self.events.pop_front();
            self.dropped_events += 1;
        }
        self.events.push_back(event);
    }
}

fn session_detached(method: &str) -> AppError {
    target_gone(method, "the browser detached the target's session")
}

fn reply_unreadable(method: &str) -> AppError {
    AppError::new(
        ErrorCode::ActionFailed,
        format!("The reply to '{method}' nests too deeply to read"),
    )
    .with_details(json!({ "kind": "cdp_reply_unreadable", "method": method }))
}

fn reply_result(method: &str, mut body: Value) -> Result<Value, AppError> {
    if let Some(error) = body.get("error") {
        return Err(protocol_error(method, error));
    }
    Ok(body
        .get_mut("result")
        .map_or_else(|| json!({}), Value::take))
}

#[cfg(test)]
#[path = "connection_tests.rs"]
mod tests;
