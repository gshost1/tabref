//! How a connection reads and writes frames: routing each inbound frame to
//! the request it answers or to the event buffer, settling raw replies,
//! accepting dialogs, and noticing a cancelled caller or a failed transport.

use std::time::Duration;

use serde_json::{Value, json};
use tabref_core::{AppError, Deadline, ErrorCode};

use super::cdp_event::CdpEvent;
use super::connection::CdpConnection;
use super::connection_health::cancelled;
use super::frame_envelope::FrameEnvelope;
use super::incoming::Incoming;
use super::protocol_error::{protocol_error, target_gone};
use crate::transport::transport_error::{is_timeout, transport_failed};

const DETACHED: &str = "Target.detachedFromTarget";
const DIALOG_OPENING: &str = "Page.javascriptDialogOpening";
const RAW_SCAN_MIN: usize = 64 * 1024;
const CANCEL_SLICE: Duration = Duration::from_millis(50);

impl CdpConnection {
    pub(super) fn read_one(&mut self, deadline: Deadline) -> Result<(), AppError> {
        let text = self.receive_text(deadline)?;
        if let Some(event) = self.route(&text, deadline)? {
            self.events.push(event);
        }
        Ok(())
    }

    /// Settles a reply, or does an event's bookkeeping (accepting a dialog,
    /// failing the requests of a detached session) and hands the event back.
    pub(super) fn route(
        &mut self,
        text: &str,
        deadline: Deadline,
    ) -> Result<Option<CdpEvent>, AppError> {
        if text.len() >= RAW_SCAN_MIN && self.settle_raw(text) {
            return Ok(None);
        }
        match Incoming::parse(text) {
            Some(Incoming::Reply { id, body }) => {
                if let Some(waiter) = self.replies.waiter(id).cloned() {
                    let mut outcome = reply_result(&waiter.method, body);
                    if waiter.raw && outcome.is_ok() {
                        outcome = Ok(Value::String(text.to_owned()));
                    }
                    if outcome.is_ok() {
                        let params = waiter.params.unwrap_or(Value::Null);
                        self.ledger
                            .record(&waiter.method, &params, waiter.session_id.as_deref());
                    }
                    self.replies.settle(id, outcome);
                }
                Ok(None)
            }
            Some(Incoming::UnreadableReply { id }) => {
                if let Some(method) = self.replies.waiter(id).map(|waiter| waiter.method.clone()) {
                    self.replies.settle(id, Err(reply_unreadable(&method)));
                }
                Ok(None)
            }
            Some(Incoming::Event(event)) => {
                self.answer_dialog(&event, deadline)?;
                if event.method == DETACHED {
                    if let Some(session) = event.params.get("sessionId").and_then(Value::as_str) {
                        self.ledger.mark_detached(session);
                        self.replies.fail_session(session, session_detached);
                    }
                }
                Ok(Some(event))
            }
            Some(Incoming::Unrecognized) => Ok(None),
            None => Err(transport_failed("receive", "unparseable DevTools frame")),
        }
    }

    /// Settles a large reply someone wants raw from its id and error alone,
    /// without building the rest of it. `false` when the frame is not one.
    pub(super) fn settle_raw(&mut self, text: &str) -> bool {
        let Ok(envelope) = serde_json::from_str::<FrameEnvelope>(text) else {
            return false;
        };
        let Some(id) = envelope.id else {
            return false;
        };
        let Some(waiter) = self.replies.waiter(id).filter(|waiter| waiter.raw) else {
            return false;
        };
        let outcome = match &envelope.error {
            Some(error) => Err(protocol_error(&waiter.method, error)),
            None => Ok(Value::String(text.to_owned())),
        };
        self.replies.settle(id, outcome);
        true
    }

    pub(super) fn answer_dialog(
        &mut self,
        event: &CdpEvent,
        deadline: Deadline,
    ) -> Result<(), AppError> {
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
        self.send_text(&request.to_string(), deadline)
    }

    pub(super) fn send_text(&mut self, text: &str, deadline: Deadline) -> Result<(), AppError> {
        self.transport
            .send_text(text, deadline)
            .map_err(|error| self.note_failure(error))
    }

    pub(super) fn receive_text(&mut self, deadline: Deadline) -> Result<String, AppError> {
        if self.health.cancel.is_none() {
            return self
                .transport
                .receive_text(deadline)
                .map_err(|error| self.note_failure(error));
        }
        loop {
            if self.health.is_cancelled() {
                return Err(cancelled());
            }
            match self.transport.receive_text(deadline.capped(CANCEL_SLICE)) {
                Err(error) if is_timeout(&error) && !deadline.is_expired() => {}
                received => return received.map_err(|error| self.note_failure(error)),
            }
        }
    }

    pub(super) fn note_failure(&mut self, error: AppError) -> AppError {
        if !is_timeout(&error) {
            self.health.closed = true;
        }
        error
    }
}

pub(super) fn session_detached(method: &str) -> AppError {
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
