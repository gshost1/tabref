use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, Weak};

use serde_json::json;
use tabref_core::AppError;

use super::activity::Activity;
use super::tab_locks::TabLocks;
use crate::commands::{download_tend, downloads};
use crate::session::Session;
use crate::session::session_options::SessionOptions;
use crate::session::tab_claim::TabClaim;

const MAX_IDLE_SESSIONS: usize = 4;

/// What one daemon process keeps between requests: its build and endpoint,
/// the browser connections not in use right now (each with its attached
/// page sessions), the tab locks, its activity clock, and the cancel flags
/// of the requests running now (`None` once it is stopping). A request
/// takes a connection out of the pool for as long as it runs, so requests
/// on different tabs run side by side on their own connections.
pub(crate) struct ServerState {
    pub(crate) build: String,
    pub(crate) cdp: Option<String>,
    pub(crate) activity: Activity,
    pool: Mutex<Vec<Session>>,
    locks: Arc<TabLocks>,
    next_owner: AtomicU64,
    running: Mutex<Option<Vec<Weak<AtomicBool>>>>,
}

impl ServerState {
    pub(crate) fn new(build: String, cdp: Option<String>) -> Self {
        Self {
            build,
            cdp,
            activity: Activity::new(),
            pool: Mutex::new(Vec::new()),
            locks: Arc::new(TabLocks::default()),
            next_owner: AtomicU64::new(1),
            running: Mutex::new(Some(Vec::new())),
        }
    }

    /// Notes a request's cancel flag, so a stop can end it. A request that
    /// arrives while the daemon stops is cancelled at once.
    pub(crate) fn track(&self, cancel: &Arc<AtomicBool>) {
        let Ok(mut running) = self.running.lock() else {
            cancel.store(true, Ordering::SeqCst);
            return;
        };
        match running.as_mut() {
            Some(flags) => {
                flags.retain(|flag| flag.strong_count() > 0);
                flags.push(Arc::downgrade(cancel));
            }
            None => cancel.store(true, Ordering::SeqCst),
        }
    }

    /// Stops everything the daemon does with a browser: every running
    /// request is cancelled (its next request or wait slice fails), and the
    /// kept connections are closed. Nothing is kept from then on.
    pub(crate) fn shut_down(&self) {
        let flags = self
            .running
            .lock()
            .map_or_else(|_| None, |mut running| running.take());
        for flag in flags.into_iter().flatten() {
            if let Some(flag) = flag.upgrade() {
                flag.store(true, Ordering::SeqCst);
            }
        }
        if let Ok(mut pool) = self.pool.lock() {
            pool.clear();
        }
    }

    fn stopping(&self) -> bool {
        self.running
            .lock()
            .map_or(true, |running| running.is_none())
    }

    /// A session ready for the next command: a kept one whose connection
    /// still answers, or a new connection. A kept session whose browser went
    /// away is dropped here, so the command reconnects instead of failing.
    pub(crate) fn checkout(
        &self,
        options: &SessionOptions,
        cancel: &Arc<AtomicBool>,
    ) -> Result<Session, AppError> {
        let owner = self.next_owner.fetch_add(1, Ordering::SeqCst);
        while let Some(mut session) = self.take_idle() {
            let claim = TabClaim::new(Arc::clone(&self.locks), owner, Arc::clone(cancel));
            if session.begin(options, Some(claim)).is_ok() {
                downloads::begin_command(&session);
                return Ok(session);
            }
        }
        let claim = TabClaim::new(Arc::clone(&self.locks), owner, Arc::clone(cancel));
        let session = Session::open_claimed(options, Some(claim))?;
        downloads::begin_command(&session);
        Ok(session)
    }

    /// Returns a session after its command, unless its connection broke.
    pub(crate) fn checkin(&self, mut session: Session) {
        downloads::end_command(&mut session);
        session.end();
        if session.is_broken() || self.stopping() {
            return;
        }
        if let Ok(mut pool) = self.pool.lock() {
            if pool.len() < MAX_IDLE_SESSIONS {
                pool.push(session);
            }
        }
    }

    pub(crate) fn status(&self) -> serde_json::Value {
        let (idle_sessions, attached, endpoint) = self.pool.lock().map_or((0, 0, None), |pool| {
            (
                pool.len(),
                pool.iter().map(Session::attached_count).sum::<usize>(),
                pool.first()
                    .map(|session| session.endpoint().websocket_url.as_url().to_owned()),
            )
        });
        json!({
            "running": true,
            "pid": std::process::id(),
            "build": self.build,
            "cdp": self.cdp,
            "browser": endpoint,
            "uptime_ms": self.activity.uptime().as_millis(),
            "idle_ms": self.activity.idle().as_millis(),
            "connections": idle_sessions,
            "attached_tabs": attached,
            "busy_tabs": self.locks.held_count(),
        })
    }

    /// Folds the download events the idle connections got since their last
    /// command, so a download that lands between commands is named.
    pub(crate) fn tend_downloads(&self) {
        if let Ok(mut pool) = self.pool.lock() {
            for session in pool.iter_mut() {
                download_tend::tend(session);
            }
        }
    }

    /// Cancels the downloads still running, before the daemon exits.
    pub(crate) fn retire_downloads(&self) {
        if let Ok(mut pool) = self.pool.lock() {
            for session in pool.iter_mut() {
                download_tend::retire(session);
            }
        }
    }

    fn take_idle(&self) -> Option<Session> {
        self.pool.lock().ok()?.pop()
    }
}

#[cfg(test)]
#[path = "server_state_tests.rs"]
mod tests;
