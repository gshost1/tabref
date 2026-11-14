use serde::{Deserialize, Serialize};
use serde_json::Value;
use tabref_core::{AppError, DeliverySemantics, ErrorCode};

const CODES: [ErrorCode; 11] = [
    ErrorCode::PermDenied,
    ErrorCode::ElementNotFound,
    ErrorCode::ActionFailed,
    ErrorCode::ActionNotSupported,
    ErrorCode::StaleRef,
    ErrorCode::AmbiguousTarget,
    ErrorCode::WindowNotFound,
    ErrorCode::Timeout,
    ErrorCode::InvalidArgs,
    ErrorCode::SnapshotNotFound,
    ErrorCode::Internal,
];

/// An [`AppError`] as the daemon sends it back, every member kept, so the
/// client prints exactly what an in-process run would.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub(crate) struct WireError {
    pub(crate) code: String,
    pub(crate) message: String,
    pub(crate) suggestion: Option<String>,
    pub(crate) platform_detail: Option<String>,
    pub(crate) details: Option<Value>,
    pub(crate) disposition: DeliverySemantics,
}

impl From<&AppError> for WireError {
    fn from(error: &AppError) -> Self {
        Self {
            code: error.code.as_str().to_owned(),
            message: error.message.clone(),
            suggestion: error.suggestion.clone(),
            platform_detail: error.platform_detail.clone(),
            details: error.details.clone(),
            disposition: error.disposition,
        }
    }
}

impl From<WireError> for AppError {
    fn from(wire: WireError) -> Self {
        let code = CODES
            .into_iter()
            .find(|code| code.as_str() == wire.code)
            .unwrap_or(ErrorCode::Internal);
        Self {
            code,
            message: wire.message,
            suggestion: wire.suggestion,
            platform_detail: wire.platform_detail,
            details: wire.details,
            disposition: wire.disposition,
        }
    }
}

#[cfg(test)]
#[path = "wire_error_tests.rs"]
mod tests;
