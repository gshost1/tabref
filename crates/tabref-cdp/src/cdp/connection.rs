use serde_json::{Value, json};
use tabref_core::{AppError, Deadline, ErrorCode};

use super::cdp_event::CdpEvent;
use super::event_buffer::EventBuffer;
use super::incoming::Incoming;
use super::protocol_error::{protocol_error, target_gone};
use super::reply_stash::ReplyStash;
use super::session_ledger::SessionLedger;
use super::waiter::Waiter;
use crate::transport::cdp_transport::CdpTransport;
use crate::transport::transport_error::{is_timeout, transport_failed};

const DETACHED: &str = "Target.detachedFromTarget";
const DIALOG_OPENING: &str = "Page.javascriptDialogOpening";
const POLL_LIMIT: usize = 4096;

/// A CDP client over one transport. Requests carry monotonic ids and may be
/// pipelined: [`send`](Self::send) several, then collect each reply with
/// [`reply`](Self::reply) or whichever comes first with
/// [`next_reply`](Self::next_reply). Events that arrive meanwhile are
/// buffered, oldest dropped first once the buffer holds 1024. The connection
/// remembers every flattened session the browser announced as detached: a
/// request on one fails at once as `cdp_target_gone`, and so does a request
/// whose session detaches while its reply is awaited, since a closing target
/// may never answer. A repeated `Page.enable` (and the other idempotent
/// switches the session ledger knows) is answered locally.
pub struct CdpConnection {
    transport: Box<dyn CdpTransport>,
    next_id: u64,
    events: EventBuffer,
    ledger: SessionLedger,
    replies: ReplyStash,
    answer_dialogs: bool,
    closed: bool,
}

impl CdpConnection {
    pub fn new(transport: Box<dyn CdpTransport>) -> Self {
        Self {
            transport,
            next_id: 1,
            events: EventBuffer::default(),
            ledger: SessionLedger::default(),
            replies: ReplyStash::default(),
            answer_dialogs: true,
            closed: false,
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
        let id = self.send(method, params, session_id, deadline)?;
        self.reply(id, deadline)
    }

    /// Sends `method` without waiting and returns the id its reply will
    /// carry. Every id sent must be collected or abandoned.
    pub fn send(
        &mut self,
        method: &str,
        params: Value,
        session_id: Option<&str>,
        deadline: Deadline,
    ) -> Result<u64, AppError> {
        if session_id.is_some_and(|session| self.ledger.is_detached(session)) {
            return Err(session_detached(method));
        }
        let id = self.next_id;
        self.next_id += 1;
        let local = self.ledger.already_on(method, &params, session_id);
        let tracked = SessionLedger::tracks(method).then(|| params.clone());
        self.replies.wait_for(
            id,
            Waiter {
                method: method.to_owned(),
                session_id: session_id.map(str::to_owned),
                params: tracked,
            },
        );
        if local {
            self.replies.settle(id, Ok(json!({})));
            return Ok(id);
        }
        let mut request = json!({ "id": id, "method": method, "params": params });
        if let Some(session_id) = session_id {
            request["sessionId"] = Value::String(session_id.to_owned());
        }
        if let Err(error) = self.send_text(&request.to_string(), deadline) {
            self.replies.abandon(id);
            return Err(error);
        }
        Ok(id)
    }

    /// Waits for the reply to `id`. A request that misses `deadline` is
    /// abandoned, so its late reply is skipped.
    pub fn reply(&mut self, id: u64, deadline: Deadline) -> Result<Value, AppError> {
        loop {
            if let Some(outcome) = self.replies.take(id) {
                return outcome;
            }
            if self.replies.waiter(id).is_none() {
                return Err(AppError::internal(format!(
                    "CDP request {id} is not in flight"
                )));
            }
            if let Err(error) = self.read_one(deadline) {
                self.replies.abandon(id);
                return Err(error);
            }
        }
    }

    /// Waits for whichever of `ids` is answered first and returns its id
    /// with its outcome. A missed deadline leaves every id in flight; the
    /// caller abandons the ones it gives up on.
    pub fn next_reply(
        &mut self,
        ids: &[u64],
        deadline: Deadline,
    ) -> Result<(u64, Result<Value, AppError>), AppError> {
        loop {
            if let Some(id) = ids.iter().copied().find(|id| self.replies.is_ready(*id)) {
                if let Some(outcome) = self.replies.take(id) {
                    return Ok((id, outcome));
                }
            }
            if let Some(id) = ids
                .iter()
                .copied()
                .find(|id| self.replies.waiter(*id).is_none())
            {
                return Err(AppError::internal(format!(
                    "CDP request {id} is not in flight"
                )));
            }
            self.read_one(deadline)?;
        }
    }

    /// Stops waiting for `id`; its reply is skipped whenever it comes.
    pub fn abandon(&mut self, id: u64) {
        self.replies.abandon(id);
    }

    /// Returns the first event, buffered or newly read, that `matches`,
    /// leaving every other buffered event in place.
    pub fn wait_for_event(
        &mut self,
        mut matches: impl FnMut(&CdpEvent) -> bool,
        deadline: Deadline,
    ) -> Result<CdpEvent, AppError> {
        if let Some(event) = self.events.take_first(&mut matches) {
            return Ok(event);
        }
        loop {
            let text = self.receive_text(deadline)?;
            if let Some(event) = self.route(&text, deadline)? {
                if matches(&event) {
                    return Ok(event);
                }
                self.events.push(event);
            }
        }
    }

    /// Reads, without blocking, every frame the browser already sent, so
    /// detaches, dialogs and late replies are accounted for before the next
    /// command. A closed connection is an error, and [`is_closed`](Self::is_closed)
    /// says so from then on.
    pub fn poll_pending(&mut self, deadline: Deadline) -> Result<(), AppError> {
        for _ in 0..POLL_LIMIT {
            let polled = self.transport.poll_text();
            let text = match polled {
                Ok(Some(text)) => text,
                Ok(None) => return Ok(()),
                Err(error) => return Err(self.note_failure(error)),
            };
            if let Some(event) = self.route(&text, deadline)? {
                self.events.push(event);
            }
        }
        Ok(())
    }

    /// Whether the transport failed or closed, after which no request can
    /// succeed and the connection must be replaced.
    pub fn is_closed(&self) -> bool {
        self.closed
    }

    /// Whether the browser announced `session_id` as detached.
    pub fn is_session_detached(&self, session_id: &str) -> bool {
        self.ledger.is_detached(session_id)
    }

    /// Drains every buffered event.
    pub fn take_events(&mut self) -> Vec<CdpEvent> {
        self.events.take_all()
    }

    /// Removes and returns, in arrival order, every buffered event that
    /// `matches`, leaving the rest buffered.
    pub fn drain_events(&mut self, matches: impl FnMut(&CdpEvent) -> bool) -> Vec<CdpEvent> {
        self.events.drain(matches)
    }

    /// Discards buffered events that `matches`.
    pub fn discard_events(&mut self, matches: impl FnMut(&CdpEvent) -> bool) {
        self.events.discard(matches);
    }

    /// How many events were discarded because the buffer was full.
    pub fn dropped_event_count(&self) -> u64 {
        self.events.dropped()
    }

    fn read_one(&mut self, deadline: Deadline) -> Result<(), AppError> {
        let text = self.receive_text(deadline)?;
        if let Some(event) = self.route(&text, deadline)? {
            self.events.push(event);
        }
        Ok(())
    }

    /// Settles a reply, or does an event's bookkeeping (accepting a dialog,
    /// failing the requests of a detached session) and hands the event back.
    fn route(&mut self, text: &str, deadline: Deadline) -> Result<Option<CdpEvent>, AppError> {
        match Incoming::parse(text) {
            Some(Incoming::Reply { id, body }) => {
                if let Some(waiter) = self.replies.waiter(id).cloned() {
                    let outcome = reply_result(&waiter.method, body);
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
        self.send_text(&request.to_string(), deadline)
    }

    fn send_text(&mut self, text: &str, deadline: Deadline) -> Result<(), AppError> {
        self.transport
            .send_text(text, deadline)
            .map_err(|error| self.note_failure(error))
    }

    fn receive_text(&mut self, deadline: Deadline) -> Result<String, AppError> {
        self.transport
            .receive_text(deadline)
            .map_err(|error| self.note_failure(error))
    }

    fn note_failure(&mut self, error: AppError) -> AppError {
        if !is_timeout(&error) {
            self.closed = true;
        }
        error
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
