use serde::Serialize;
use serde_json::Value;

use super::recovery_hint::RecoveryHint;
use crate::{AppError, DeliverySemantics, ErrorCode, RetryDisposition};

/// The `error` member of a failed envelope. `code`, `message` and
/// `disposition` are always present; the rest are omitted when empty, and
/// `recovery` appears only when the disposition makes a retry safe.
#[derive(Debug, Serialize)]
pub struct ErrorPayload {
    pub code: String,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub suggestion: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub recovery: Option<RecoveryHint>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub platform_detail: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub details: Option<Value>,
    pub disposition: DeliverySemantics,
}

impl ErrorPayload {
    pub fn from_error(error: &AppError) -> Self {
        Self {
            code: error.code.as_str().to_owned(),
            message: error.message.clone(),
            suggestion: error.suggestion.clone(),
            recovery: recovery_for(error),
            platform_detail: error.platform_detail.clone(),
            details: error.details.clone(),
            disposition: error.disposition,
        }
    }
}

fn recovery_for(error: &AppError) -> Option<RecoveryHint> {
    if error.disposition.retry() != RetryDisposition::Safe {
        return None;
    }
    let explicit_no_retry = error
        .details
        .as_ref()
        .and_then(|details| details.get("retryable"))
        .and_then(Value::as_bool)
        == Some(false);
    if explicit_no_retry {
        return None;
    }
    let hint = |strategy: &str, fresh: bool, after: Option<u64>| RecoveryHint {
        strategy: strategy.into(),
        retryable: true,
        requires_fresh_snapshot: fresh,
        retry_after_ms: after,
    };
    match error.code {
        ErrorCode::StaleRef | ErrorCode::SnapshotNotFound => {
            Some(hint("refresh_snapshot_then_retry_original", true, None))
        }
        ErrorCode::Timeout => Some(hint("inspect_state_then_retry_original", true, Some(250))),
        _ => None,
    }
}
