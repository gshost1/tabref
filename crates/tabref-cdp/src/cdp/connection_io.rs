//! How a connection reads and writes frames: routing each inbound frame to
//! the request it answers or to the event buffer, settling raw replies,
//! accepting dialogs, and noticing a cancelled caller or a failed transport.

use std::time::Duration;

use serde_json::{Value, json};
use tabref_core::{AppError, Deadline, ErrorCode};

use super::cdp_event::CdpEvent;
use super::connection::CdpConnection;
use super::dialog_choice::DialogChoice;
use super::frame_envelope::FrameEnvelope;
use super::incoming::Incoming;
use super::protocol_error::{protocol_error, target_gone};
use crate::transport::transport_error::{is_timeout, transport_failed};

const DETACHED: &str = "Target.detachedFromTarget";
const ATTACHED: &str = "Target.attachedToTarget";
const DIALOG_OPENING: &str = "Page.javascriptDialogOpening";
const RAW_SCAN_MIN: usize = 64 * 1024;
const CANCEL_SLICE: Duration = Duration::from_millis(50);
const CLEANUP_SLICE: Duration = Duration::from_millis(20);

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
                if let Some(session_id) = self.replies.take_late_script_session(id) {
                    if let Some(identifier) =
                        body.pointer("/result/identifier").and_then(Value::as_str)
                    {
                        self.remove_late_script(&session_id, identifier, deadline)?;
                    }
                    return Ok(None);
                }
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
                if self.replies.take_late_script_session(id).is_some() {
                    self.health.closed = true;
                }
                if let Some(method) = self.replies.waiter(id).map(|waiter| waiter.method.clone()) {
                    self.replies.settle(id, Err(reply_unreadable(&method)));
                }
                Ok(None)
            }
            Some(Incoming::Event(mut event)) => {
                self.note_child_tab(&event);
                self.answer_dialog(&mut event, deadline)?;
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

    /// Send cleanup without the cancelled caller's guard and without
    /// awaiting a reply while routing an unrelated command's frames.
    fn remove_late_script(
        &mut self,
        session_id: &str,
        identifier: &str,
        deadline: Deadline,
    ) -> Result<(), AppError> {
        let id = self.next_id;
        self.next_id += 1;
        let request = json!({
            "id": id,
            "method": "Page.removeScriptToEvaluateOnNewDocument",
            "params": { "identifier": identifier },
            "sessionId": session_id,
        });
        match self.send_text(&request.to_string(), deadline.capped(CLEANUP_SLICE)) {
            Err(error) if is_timeout(&error) => {
                self.health.closed = true;
                Ok(())
            }
            sent => sent,
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

    /// Answers a dialog that opened: as the command chose, when the choice
    /// applies to this kind of dialog (never an alert; see
    /// `DialogChoice::applies_to`), else accepted (a prompt with its own
    /// default text). The event keeps the answer in `params.answer` for the
    /// report.
    pub(super) fn answer_dialog(
        &mut self,
        event: &mut CdpEvent,
        deadline: Deadline,
    ) -> Result<(), AppError> {
        if !self.dialogs.auto || event.method != DIALOG_OPENING {
            return Ok(());
        }
        let Some(session_id) = event.session_id.clone() else {
            return Ok(());
        };
        let id = self.next_id;
        self.next_id += 1;
        let default_prompt = event
            .params
            .get("defaultPrompt")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_owned();
        let kind = event
            .params
            .get("type")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_owned();
        let chosen = match self.dialogs.next.take() {
            Some(choice) if choice.applies_to(&kind) => choice,
            unused => {
                self.dialogs.next = unused;
                DialogChoice::Accept { text: None }
            }
        };
        let (accept, text) = chosen.answer(&default_prompt);
        let request = json!({
            "id": id,
            "method": "Page.handleJavaScriptDialog",
            "params": { "accept": accept, "promptText": text },
            "sessionId": session_id,
        });
        let prompt = kind == "prompt";
        let mut answer = json!({ "accept": accept });
        if accept && prompt {
            answer["text"] = Value::String(text);
        }
        event.params["answer"] = answer;
        if let Some(tab) = self.ledger.tab_of(&session_id) {
            event.params["tab"] = Value::String(tab.to_owned());
        }
        self.send_text(&request.to_string(), deadline)
    }

    /// An iframe session auto-attached under a session belongs to that
    /// session's tab.
    fn note_child_tab(&mut self, event: &CdpEvent) {
        if event.method != ATTACHED {
            return;
        }
        let (Some(parent), Some(child)) = (
            event.session_id.as_deref(),
            event.params.get("sessionId").and_then(Value::as_str),
        ) else {
            return;
        };
        if let Some(tab) = self.ledger.tab_of(parent).map(str::to_owned) {
            self.ledger.note_tab(child, &tab);
        }
    }

    pub(super) fn send_text(&mut self, text: &str, deadline: Deadline) -> Result<(), AppError> {
        self.transport
            .send_text(text, deadline)
            .map_err(|error| self.note_failure(error))
    }

    pub(super) fn receive_text(&mut self, deadline: Deadline) -> Result<String, AppError> {
        if !self.health.watched() {
            return self
                .transport
                .receive_text(deadline)
                .map_err(|error| self.note_failure(error));
        }
        loop {
            self.health.check()?;
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
