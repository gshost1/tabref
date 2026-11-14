#![cfg_attr(not(unix), allow(dead_code))]

use std::sync::Arc;

use tabref_core::{AppError, Deadline};

use crate::daemon::tab_locks::TabLocks;

/// A daemon command's hold on the tabs it works in, released when the claim
/// is dropped at the end of the command.
#[derive(Debug)]
pub(crate) struct TabClaim {
    locks: Arc<TabLocks>,
    owner: u64,
}

impl TabClaim {
    pub(crate) fn new(locks: Arc<TabLocks>, owner: u64) -> Self {
        Self { locks, owner }
    }

    pub(crate) fn claim(&self, tab: &str, deadline: Deadline) -> Result<(), AppError> {
        self.locks.claim(tab, self.owner, deadline)
    }
}

impl Drop for TabClaim {
    fn drop(&mut self) {
        self.locks.release(self.owner);
    }
}
