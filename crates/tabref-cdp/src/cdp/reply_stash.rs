use std::collections::HashMap;

use serde_json::Value;
use tabref_core::AppError;

use super::waiter::Waiter;

const ADD_NEW_DOCUMENT_SCRIPT: &str = "Page.addScriptToEvaluateOnNewDocument";

/// The requests a connection has in flight, and the replies that arrived
/// for them before their caller asked. A reply to an id nobody waits for
/// any more is dropped, except when it identifies a script to remove.
#[derive(Debug, Default)]
pub(crate) struct ReplyStash {
    waiting: HashMap<u64, Waiter>,
    ready: HashMap<u64, Result<Value, AppError>>,
    late_scripts: HashMap<u64, String>,
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
        self.late_scripts.retain(|_, session| session != session_id);
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

    /// Stops waiting for `id`. Keep the session of an in-flight script
    /// registration so a late success can be undone.
    pub(crate) fn abandon(&mut self, id: u64) {
        if let Some(waiter) = self.waiting.remove(&id)
            && waiter.method == ADD_NEW_DOCUMENT_SCRIPT
            && let Some(session) = waiter.session_id
        {
            self.late_scripts.insert(id, session);
        }
        self.ready.remove(&id);
    }

    pub(crate) fn take_late_script_session(&mut self, id: u64) -> Option<String> {
        self.late_scripts.remove(&id)
    }
}
