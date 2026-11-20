use std::sync::Arc;
use std::sync::atomic::AtomicBool;

use serde_json::{Value, json};
use tabref_core::{AppError, Deadline};

use super::cdp_event::CdpEvent;
use super::connection_health::{ConnectionGuard, ConnectionHealth};
use super::connection_io::session_detached;
use super::dialog_answers::DialogAnswers;
use super::dialog_choice::DialogChoice;
use super::event_buffer::EventBuffer;
use super::reply_stash::ReplyStash;
use super::session_ledger::SessionLedger;
use super::waiter::Waiter;
use crate::transport::cdp_transport::CdpTransport;

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
    pub(super) transport: Box<dyn CdpTransport>,
    pub(super) next_id: u64,
    pub(super) events: EventBuffer,
    pub(super) ledger: SessionLedger,
    pub(super) replies: ReplyStash,
    pub(super) dialogs: DialogAnswers,
    pub(super) health: ConnectionHealth,
}

impl CdpConnection {
    pub fn new(transport: Box<dyn CdpTransport>) -> Self {
        Self {
            transport,
            next_id: 1,
            events: EventBuffer::default(),
            ledger: SessionLedger::default(),
            replies: ReplyStash::default(),
            dialogs: DialogAnswers::default(),
            health: ConnectionHealth::default(),
        }
    }

    /// Whether a JavaScript dialog that opens while this connection waits is
    /// accepted at once (the default). A page blocked on `alert()` answers no
    /// other request, so an unanswered dialog stalls every later call; the
    /// `Page.javascriptDialogOpening` event stays buffered so the caller can
    /// report what the dialog said.
    pub fn set_answer_dialogs(&mut self, answer: bool) {
        self.dialogs.auto = answer;
    }

    /// Answers the next dialog that opens as `choice` says (or clears an
    /// earlier choice with `None`); the dialogs after it are accepted again.
    /// The buffered `Page.javascriptDialogOpening` event of every dialog the
    /// connection answers carries its answer in `params.answer`
    /// (`{accept, text}`, `text` for a prompt).
    pub fn answer_next_dialog(&mut self, choice: Option<DialogChoice>) {
        self.dialogs.next = choice;
    }

    /// The choice no dialog used yet, which is cleared.
    pub fn take_dialog_choice(&mut self) -> Option<DialogChoice> {
        self.dialogs.next.take()
    }

    /// Records that `session_id` is the page session of the tab `target_id`,
    /// so a dialog it (or an iframe session under it) opens names that tab
    /// in `params.tab`.
    pub fn note_session_tab(&mut self, session_id: &str, target_id: &str) {
        self.ledger.note_tab(session_id, target_id);
    }

    /// Marks every dialog event buffered so far with `params.late`: they
    /// opened while no command was running.
    pub fn mark_buffered_dialogs_late(&mut self) {
        let dialogs = self
            .events
            .drain(|event| event.method == "Page.javascriptDialogOpening");
        for mut event in dialogs {
            event.params["late"] = serde_json::Value::Bool(true);
            self.events.push(event);
        }
    }

    /// Ties the connection's waits to `cancel` (or unties them with `None`):
    /// once it is set, every request and every wait fails at once with kind
    /// `cancelled`, and a blocking read notices within 50 ms. The connection
    /// stays usable for the next caller.
    pub fn set_cancel(&mut self, cancel: Option<Arc<AtomicBool>>) {
        self.health.cancel = cancel;
    }

    /// Checks `guard` before every request and, while a reply or event is
    /// awaited, every 50 ms: its error ends the request or wait at once. The
    /// guard stays until it is replaced.
    pub fn set_guard(&mut self, guard: Option<ConnectionGuard>) {
        self.health.guard = guard;
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

    /// Sends every `(method, params)` on `session_id` before waiting for any
    /// reply, then returns the replies in order, so the calls cost one round
    /// trip instead of one each. The browser handles them in the order sent.
    /// The first error is returned and the other replies are abandoned.
    pub fn call_all(
        &mut self,
        calls: &[(&str, Value)],
        session_id: Option<&str>,
        deadline: Deadline,
    ) -> Result<Vec<Value>, AppError> {
        let mut ids = Vec::with_capacity(calls.len());
        for (method, params) in calls {
            match self.send(method, params.clone(), session_id, deadline) {
                Ok(id) => ids.push(id),
                Err(error) => {
                    ids.into_iter().for_each(|id| self.abandon(id));
                    return Err(error);
                }
            }
        }
        let mut replies = Vec::with_capacity(ids.len());
        let mut pending = ids.into_iter();
        while let Some(id) = pending.next() {
            match self.reply(id, deadline) {
                Ok(reply) => replies.push(reply),
                Err(error) => {
                    pending.for_each(|id| self.abandon(id));
                    return Err(error);
                }
            }
        }
        Ok(replies)
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
        self.health.check()?;
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
                raw: false,
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

    /// [`send`](Self::send) for a reply too large to parse twice, such as a
    /// full accessibility tree: its outcome is the whole reply frame's text
    /// as a `Value::String`, for the caller to parse into its own types. An
    /// error reply is still an error.
    pub fn send_raw(
        &mut self,
        method: &str,
        params: Value,
        session_id: Option<&str>,
        deadline: Deadline,
    ) -> Result<u64, AppError> {
        let id = self.send(method, params, session_id, deadline)?;
        self.replies.mark_raw(id);
        Ok(id)
    }

    /// Waits for the reply to `id`. A request that misses `deadline` is
    /// abandoned. Late replies are skipped unless they identify a script
    /// registration that must be undone.
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

    /// Stops waiting for `id`, with the same late-reply handling as a
    /// timeout in [`reply`](Self::reply).
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

    /// Whether the transport failed or the connection must be retired
    /// after the current command because cleanup could not be sent.
    pub fn is_closed(&self) -> bool {
        self.health.closed
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
}
#[cfg(test)]
#[path = "connection_tests.rs"]
mod tests;
