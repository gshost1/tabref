use std::collections::HashMap;

use serde_json::Value;
use tabref_core::AppError;

use super::waiter::Waiter;

/// The requests a connection has in flight, and the replies that arrived
/// for them before their caller asked. A reply to an id nobody waits for
/// any more, such as an abandoned request's, is dropped on arrival.
#[derive(Debug, Default)]
pub(crate) struct ReplyStash {
    waiting: HashMap<u64, Waiter>,
    ready: HashMap<u64, Result<Value, AppError>>,
}

impl ReplyStash {
    pub(crate) fn wait_for(&mut self, id: u64, waiter: Waiter) {
        self.waiting.insert(id, waiter);
    }

    pub(crate) fn mark_raw(&mut self, id: u64) {
        if let Some(waiter) = self.waiting.get_mut(&id) {
            waiter.raw = true;
        }
    }

    pub(crate) fn waiter(&self, id: u64) -> Option<&Waiter> {
        self.waiting.get(&id)
    }

    /// Keeps the outcome of `id` when someone waits for it.
    pub(crate) fn settle(&mut self, id: u64, outcome: Result<Value, AppError>) {
        if self.waiting.remove(&id).is_some() {
            self.ready.insert(id, outcome);
        }
    }

    /// Fails every request in flight on `session_id` with `error`.
    pub(crate) fn fail_session(&mut self, session_id: &str, error: impl Fn(&str) -> AppError) {
        let lost: Vec<(u64, String)> = self
            .waiting
            .iter()
            .filter(|(_, waiter)| waiter.session_id.as_deref() == Some(session_id))
            .map(|(id, waiter)| (*id, waiter.method.clone()))
            .collect();
        for (id, method) in lost {
            self.settle(id, Err(error(&method)));
        }
    }

    pub(crate) fn take(&mut self, id: u64) -> Option<Result<Value, AppError>> {
        self.ready.remove(&id)
    }

    pub(crate) fn is_ready(&self, id: u64) -> bool {
        self.ready.contains_key(&id)
    }

    /// Stops waiting for `id`; its reply, early or late, is dropped.
    pub(crate) fn abandon(&mut self, id: u64) {
        self.waiting.remove(&id);
        self.ready.remove(&id);
    }
}
