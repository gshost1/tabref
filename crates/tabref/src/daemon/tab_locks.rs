use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Condvar, Mutex};
use std::time::Duration;

use tabref_core::{AppError, Deadline, ErrorCode};

const WAIT_SLICE: Duration = Duration::from_millis(50);

/// Which command holds each tab. The daemon runs commands concurrently, but
/// never two on one tab: a command claims every tab it works in and waits,
/// bounded by its deadline and ended by its `cancel` flag, while another
/// command holds one. Claims are re-entrant for their owner and released
/// together.
#[derive(Debug, Default)]
pub(crate) struct TabLocks {
    held: Mutex<HashMap<String, u64>>,
    freed: Condvar,
}

impl TabLocks {
    pub(crate) fn claim(
        &self,
        tab: &str,
        owner: u64,
        deadline: Deadline,
        cancel: &AtomicBool,
    ) -> Result<(), AppError> {
        let mut held = self.held.lock().map_err(|_| poisoned())?;
        loop {
            match held.get(tab) {
                None => {
                    held.insert(tab.to_owned(), owner);
                    return Ok(());
                }
                Some(holder) if *holder == owner => return Ok(()),
                Some(_) if cancel.load(Ordering::SeqCst) => return Err(cancelled(tab)),
                Some(_) if deadline.is_expired() => return Err(busy(tab)),
                Some(_) => {
                    let slice = deadline.remaining().min(WAIT_SLICE);
                    held = self
                        .freed
                        .wait_timeout(held, slice)
                        .map_err(|_| poisoned())?
                        .0;
                }
            }
        }
    }

    /// Holds `tab` for `owner` if no other command holds it, without
    /// waiting: `Some(true)` when it was taken now, `Some(false)` when
    /// `owner` held it already, `None` when another command holds it.
    pub(crate) fn try_hold(&self, tab: &str, owner: u64) -> Option<bool> {
        let mut held = self.held.lock().ok()?;
        match held.get(tab) {
            None => {
                held.insert(tab.to_owned(), owner);
                Some(true)
            }
            Some(holder) if *holder == owner => Some(false),
            Some(_) => None,
        }
    }

    /// Releases `owner`'s hold on `tab` alone, so a command that held a tab
    /// only for one call does not keep it for the rest of its run.
    pub(crate) fn release_tab(&self, tab: &str, owner: u64) {
        if let Ok(mut held) = self.held.lock()
            && held.get(tab) == Some(&owner)
        {
            held.remove(tab);
        }
        self.freed.notify_all();
    }

    pub(crate) fn release(&self, owner: u64) {
        if let Ok(mut held) = self.held.lock() {
            held.retain(|_, holder| *holder != owner);
        }
        self.freed.notify_all();
    }

    pub(crate) fn held_count(&self) -> usize {
        self.held.lock().map_or(0, |held| held.len())
    }
}

fn busy(tab: &str) -> AppError {
    AppError::new(
        ErrorCode::Timeout,
        format!("Tab {tab} stayed busy with another command until the deadline"),
    )
    .with_suggestion("Wait for the other command to finish, or pass a longer --timeout")
    .with_details(serde_json::json!({ "kind": "tab_busy", "tab": tab }))
    .with_disposition(tabref_core::DeliverySemantics::not_delivered())
}

fn cancelled(tab: &str) -> AppError {
    AppError::new(
        ErrorCode::ActionFailed,
        format!("Stopped waiting for tab {tab}: the command's caller went away"),
    )
    .with_details(serde_json::json!({ "kind": "cancelled", "tab": tab }))
    .with_disposition(tabref_core::DeliverySemantics::not_delivered())
}

fn poisoned() -> AppError {
    AppError::internal("The daemon's tab locks are unusable after a failed command")
}

#[cfg(test)]
#[path = "tab_locks_tests.rs"]
mod tests;
