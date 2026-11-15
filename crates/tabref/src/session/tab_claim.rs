#![cfg_attr(not(unix), allow(dead_code))]

use std::sync::Arc;
use std::sync::atomic::AtomicBool;

use tabref_core::{AppError, Deadline};

use crate::daemon::tab_locks::TabLocks;

/// A daemon command's hold on the tabs it works in, released when the claim
/// is dropped at the end of the command, and the flag that cancels the
/// command when its client goes away.
#[derive(Debug)]
pub(crate) struct TabClaim {
    locks: Arc<TabLocks>,
    owner: u64,
    cancel: Arc<AtomicBool>,
}

impl TabClaim {
    pub(crate) fn new(locks: Arc<TabLocks>, owner: u64, cancel: Arc<AtomicBool>) -> Self {
        Self {
            locks,
            owner,
            cancel,
        }
    }

    pub(crate) fn claim(&self, tab: &str, deadline: Deadline) -> Result<(), AppError> {
        self.locks.claim(tab, self.owner, deadline, &self.cancel)
    }

    pub(crate) fn cancel_flag(&self) -> Arc<AtomicBool> {
        Arc::clone(&self.cancel)
    }
}

impl Drop for TabClaim {
    fn drop(&mut self) {
        self.locks.release(self.owner);
    }
}
