use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use serde_json::json;
use tabref_core::AppError;

use super::activity::Activity;
use super::tab_locks::TabLocks;
use crate::commands::downloads;
use crate::session::Session;
use crate::session::session_options::SessionOptions;
use crate::session::tab_claim::TabClaim;

const MAX_IDLE_SESSIONS: usize = 4;

/// What one daemon process keeps between requests: its build and endpoint,
/// the browser connections not in use right now (each with its attached
/// page sessions), the tab locks, and its activity clock. A request takes a
/// connection out of the pool for as long as it runs, so requests on
/// different tabs run side by side on their own connections.
pub(crate) struct ServerState {
    pub(crate) build: String,
    pub(crate) cdp: Option<String>,
    pub(crate) activity: Activity,
    pool: Mutex<Vec<Session>>,
    locks: Arc<TabLocks>,
    next_owner: AtomicU64,
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
        }
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
                return Ok(session);
            }
        }
        let claim = TabClaim::new(Arc::clone(&self.locks), owner, Arc::clone(cancel));
        Session::open_claimed(options, Some(claim))
    }

    /// Returns a session after its command, unless its connection broke.
    pub(crate) fn checkin(&self, mut session: Session) {
        session.end();
        if session.is_broken() {
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
                downloads::tend(session);
            }
        }
    }

    /// Cancels the downloads still running, before the daemon exits.
    pub(crate) fn retire_downloads(&self) {
        if let Ok(mut pool) = self.pool.lock() {
            for session in pool.iter_mut() {
                downloads::retire(session);
            }
        }
    }

    fn take_idle(&self) -> Option<Session> {
        self.pool.lock().ok()?.pop()
    }
}
