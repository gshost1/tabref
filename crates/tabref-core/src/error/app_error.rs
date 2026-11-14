use std::fmt;

use serde_json::{Value, json};

use super::delivery_semantics::DeliverySemantics;
use super::error_code::ErrorCode;

/// The one error type every tabref layer returns. `message` says what went
/// wrong, `suggestion` what to do next, `platform_detail` carries the
/// browser's own words, `details.kind` a stable machine tag such as
/// `cdp_click_intercepted`, and `disposition` whether the action reached the
/// page, which decides whether a retry is safe.
#[derive(Debug, Clone, PartialEq)]
pub struct AppError {
    pub code: ErrorCode,
    pub message: String,
    pub suggestion: Option<String>,
    pub platform_detail: Option<String>,
    pub details: Option<Value>,
    pub disposition: DeliverySemantics,
}

impl AppError {
    pub fn new(code: ErrorCode, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
            suggestion: None,
            platform_detail: None,
            details: None,
            disposition: DeliverySemantics::unknown(),
        }
    }

    pub fn with_suggestion(mut self, suggestion: impl Into<String>) -> Self {
        self.suggestion = Some(suggestion.into());
        self
    }

    pub fn with_platform_detail(mut self, detail: impl Into<String>) -> Self {
        self.platform_detail = Some(detail.into());
        self
    }

    pub fn with_details(mut self, details: Value) -> Self {
        self.details = Some(details);
        self
    }

    pub fn with_disposition(mut self, disposition: DeliverySemantics) -> Self {
        self.disposition = disposition;
        self
    }

    /// The `details.kind` tag, when the error carries one.
    pub fn kind(&self) -> Option<&str> {
        self.details
            .as_ref()
            .and_then(|details| details.get("kind"))
            .and_then(Value::as_str)
    }

    pub fn invalid_args(message: impl Into<String>) -> Self {
        Self::new(ErrorCode::InvalidArgs, message)
            .with_disposition(DeliverySemantics::not_delivered())
    }

    pub fn internal(message: impl Into<String>) -> Self {
        Self::new(ErrorCode::Internal, message)
    }

    pub fn timeout(message: impl Into<String>) -> Self {
        Self::new(ErrorCode::Timeout, message)
            .with_suggestion("The page may be busy; retry with a longer --timeout")
    }

    pub fn stale_ref(ref_id: &str) -> Self {
        Self::new(
            ErrorCode::StaleRef,
            format!("{ref_id} is not in its snapshot"),
        )
        .with_suggestion("Run 'tabref snap' for fresh refs, then retry with the new ref")
        .with_disposition(DeliverySemantics::not_delivered())
    }

    /// `STALE_REF` for a ref whose element is gone or changed, with the
    /// reason the caller already knows.
    pub fn stale_ref_because(message: impl Into<String>) -> Self {
        Self::new(ErrorCode::StaleRef, message)
            .with_suggestion("Run 'tabref snap' for fresh refs, then retry with the new ref")
            .with_disposition(DeliverySemantics::not_delivered())
    }

    pub fn snapshot_not_found(snapshot_id: &str) -> Self {
        Self::new(
            ErrorCode::SnapshotNotFound,
            format!("Snapshot '{snapshot_id}' not found"),
        )
        .with_suggestion("Run 'tabref snap' and use a ref it prints")
        .with_details(json!({ "kind": "snapshot_not_found", "snapshot_id": snapshot_id }))
        .with_disposition(DeliverySemantics::not_delivered())
    }
}

impl fmt::Display for AppError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl std::error::Error for AppError {}

impl From<std::io::Error> for AppError {
    fn from(error: std::io::Error) -> Self {
        Self::internal(format!("I/O error: {error}"))
            .with_details(json!({ "kind": "io", "io_kind": format!("{:?}", error.kind()) }))
    }
}

impl From<serde_json::Error> for AppError {
    fn from(error: serde_json::Error) -> Self {
        Self::internal(format!("JSON error: {error}")).with_details(json!({ "kind": "json" }))
    }
}

#[cfg(test)]
#[path = "app_error_tests.rs"]
mod tests;
