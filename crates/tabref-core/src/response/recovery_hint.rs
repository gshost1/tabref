use serde::Serialize;

/// The structured next step for an error that is safe to retry.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct RecoveryHint {
    pub strategy: String,
    pub retryable: bool,
    pub requires_fresh_snapshot: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub retry_after_ms: Option<u64>,
}
