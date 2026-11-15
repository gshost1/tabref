use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use serde_json::json;
use tabref_core::{AppError, DeliverySemantics, ErrorCode};

/// A condition a connection's user sets for every request and every wait:
/// its error ends them at once, as a cancel does (for example, the user
/// withdrew the permission the connection was made under).
pub type ConnectionGuard = Arc<dyn Fn() -> Result<(), AppError> + Send + Sync>;

/// Whether a connection can still be used: its transport may have failed
/// for good, its current caller may have been cancelled (a daemon command
/// whose client went away), and its guard may have tripped; the last two
/// end every wait at once.
#[derive(Default)]
pub(crate) struct ConnectionHealth {
    pub(crate) closed: bool,
    pub(crate) cancel: Option<Arc<AtomicBool>>,
    pub(crate) guard: Option<ConnectionGuard>,
}

impl ConnectionHealth {
    /// `Ok` while the caller is not cancelled and the guard holds.
    pub(crate) fn check(&self) -> Result<(), AppError> {
        let cancelled_now = self
            .cancel
            .as_ref()
            .is_some_and(|cancel| cancel.load(Ordering::SeqCst));
        if cancelled_now {
            return Err(cancelled());
        }
        self.guard.as_ref().map_or(Ok(()), |guard| guard())
    }

    /// Whether waits must be sliced so the cancel flag and the guard are
    /// looked at while the browser is silent.
    pub(crate) fn watched(&self) -> bool {
        self.cancel.is_some() || self.guard.is_some()
    }
}

impl std::fmt::Debug for ConnectionHealth {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ConnectionHealth")
            .field("closed", &self.closed)
            .field("cancel", &self.cancel)
            .field("guard", &self.guard.is_some())
            .finish()
    }
}

/// The error a cancelled wait ends with.
pub(crate) fn cancelled() -> AppError {
    AppError::new(
        ErrorCode::ActionFailed,
        "The command was cancelled because its caller went away",
    )
    .with_details(json!({ "kind": "cancelled" }))
    .with_disposition(DeliverySemantics::uncertain())
}

#[cfg(test)]
#[path = "connection_health_tests.rs"]
mod tests;
