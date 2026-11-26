//! One command at a time in the user's own browser. That browser asks its
//! user to approve every new DevTools connection, so the daemon keeps a
//! single connection to it: a command that comes while another one uses it
//! waits for its turn (bounded by its own deadline) and then reuses the kept
//! connection, instead of opening a second one and a second prompt.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Condvar, Mutex};
use std::time::{Duration, Instant};

use tabref_core::{AppError, DeliverySemantics, ErrorCode};

const SLICE: Duration = Duration::from_millis(50);

#[derive(Default)]
pub(crate) struct UserBrowserTurns {
    taken: Mutex<bool>,
    freed: Condvar,
}

/// The turn a command holds; dropping it lets the next command in.
pub(crate) struct UserBrowserTurn<'a> {
    turns: &'a UserBrowserTurns,
}

impl Drop for UserBrowserTurn<'_> {
    fn drop(&mut self) {
        if let Ok(mut taken) = self.turns.taken.lock() {
            *taken = false;
        }
        self.turns.freed.notify_one();
    }
}

impl UserBrowserTurns {
    /// Waits until no other command uses the user's browser, at most
    /// `limit`, and returns the turn with how long it waited. A set
    /// `cancel` (the caller went away) ends the wait.
    pub(crate) fn take(
        &self,
        limit: Duration,
        cancel: &AtomicBool,
    ) -> Result<(UserBrowserTurn<'_>, Duration), AppError> {
        let started = Instant::now();
        let mut taken = self.taken.lock().map_err(|_| poisoned())?;
        while *taken {
            if cancel.load(Ordering::SeqCst) {
                return Err(cancelled());
            }
            let waited = started.elapsed();
            if waited >= limit {
                return Err(busy(waited));
            }
            let slice = SLICE.min(limit - waited);
            taken = self
                .freed
                .wait_timeout(taken, slice)
                .map_err(|_| poisoned())?
                .0;
        }
        *taken = true;
        Ok((UserBrowserTurn { turns: self }, started.elapsed()))
    }
}

fn busy(waited: Duration) -> AppError {
    AppError::new(
        ErrorCode::Timeout,
        format!(
            "Another tabref command used your browser for the whole {} ms this one could wait",
            waited.as_millis()
        ),
    )
    .with_suggestion(
        "tabref keeps one connection to your own browser (each new one makes it ask you to \
         allow it), so commands there run one at a time: wait for the other command, or pass a \
         longer --timeout",
    )
    .with_details(serde_json::json!({ "kind": "user_chrome_busy" }))
    .with_disposition(DeliverySemantics::not_delivered())
}

fn cancelled() -> AppError {
    AppError::new(
        ErrorCode::ActionFailed,
        "Stopped waiting for your browser: the command's caller went away",
    )
    .with_details(serde_json::json!({ "kind": "cancelled" }))
    .with_disposition(DeliverySemantics::not_delivered())
}

fn poisoned() -> AppError {
    AppError::internal("The daemon's turn for your browser was poisoned by a crashed command")
}

#[cfg(test)]
#[path = "user_browser_turn_tests.rs"]
mod tests;
