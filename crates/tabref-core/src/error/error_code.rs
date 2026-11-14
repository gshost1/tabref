use serde::Serialize;

/// The machine-readable error codes, a subset of agent-desktop's with the
/// same spelling, so an agent that knows one tool reads the other.
#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ErrorCode {
    PermDenied,
    ElementNotFound,
    ActionFailed,
    ActionNotSupported,
    StaleRef,
    AmbiguousTarget,
    WindowNotFound,
    Timeout,
    InvalidArgs,
    SnapshotNotFound,
    Internal,
}

impl ErrorCode {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::PermDenied => "PERM_DENIED",
            Self::ElementNotFound => "ELEMENT_NOT_FOUND",
            Self::ActionFailed => "ACTION_FAILED",
            Self::ActionNotSupported => "ACTION_NOT_SUPPORTED",
            Self::StaleRef => "STALE_REF",
            Self::AmbiguousTarget => "AMBIGUOUS_TARGET",
            Self::WindowNotFound => "WINDOW_NOT_FOUND",
            Self::Timeout => "TIMEOUT",
            Self::InvalidArgs => "INVALID_ARGS",
            Self::SnapshotNotFound => "SNAPSHOT_NOT_FOUND",
            Self::Internal => "INTERNAL",
        }
    }
}
