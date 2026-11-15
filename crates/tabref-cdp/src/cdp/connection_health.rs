use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use serde_json::json;
use tabref_core::{AppError, DeliverySemantics, ErrorCode};

/// Whether a connection can still be used: its transport may have failed
/// for good, and its current caller may have been cancelled (a daemon
/// command whose client went away), which ends every wait at once.
#[derive(Debug, Default)]
pub(crate) struct ConnectionHealth {
    pub(crate) closed: bool,
    pub(crate) cancel: Option<Arc<AtomicBool>>,
}

impl ConnectionHealth {
    pub(crate) fn is_cancelled(&self) -> bool {
        self.cancel
            .as_ref()
            .is_some_and(|cancel| cancel.load(Ordering::SeqCst))
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
