use std::time::{Duration, Instant};

use serde_json::json;

use crate::{AppError, ErrorCode};

const MAX_TIMEOUT_MS: u64 = 3_600_000;

/// The point in time an operation must finish by. Every blocking read and
/// write takes one, so no call can outlive the command's `--timeout`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Deadline {
    started_at: Instant,
    expires_at: Instant,
    timeout_ms: u64,
}

impl Deadline {
    /// A deadline `timeout_ms` from now. More than an hour is refused.
    pub fn after(timeout_ms: u64) -> Result<Self, AppError> {
        let started_at = Instant::now();
        let expires_at = Some(timeout_ms)
            .filter(|timeout| *timeout <= MAX_TIMEOUT_MS)
            .and_then(|timeout| started_at.checked_add(Duration::from_millis(timeout)))
            .ok_or_else(|| {
                AppError::new(
                    ErrorCode::InvalidArgs,
                    "Timeout cannot be represented safely",
                )
                .with_details(json!({ "timeout_ms": timeout_ms }))
            })?;
        Ok(Self {
            started_at,
            expires_at,
            timeout_ms,
        })
    }

    pub fn remaining(self) -> Duration {
        self.expires_at.saturating_duration_since(Instant::now())
    }

    pub fn remaining_ms(self) -> u64 {
        u64::try_from(self.remaining().as_millis()).unwrap_or(u64::MAX)
    }

    /// The earlier of this deadline and `maximum` from now.
    pub fn capped(self, maximum: Duration) -> Self {
        let expires_at = Instant::now()
            .checked_add(maximum)
            .map_or(self.expires_at, |slice| self.expires_at.min(slice));
        Self { expires_at, ..self }
    }

    pub fn is_expired(self) -> bool {
        self.remaining().is_zero()
    }

    pub fn elapsed(self) -> Duration {
        Instant::now().saturating_duration_since(self.started_at)
    }

    pub fn timeout_ms(self) -> u64 {
        self.timeout_ms
    }

    pub fn timeout_error(self) -> AppError {
        AppError::timeout("Operation exceeded its deadline").with_details(json!({
            "kind": "deadline",
            "timeout_ms": self.timeout_ms,
            "elapsed_ms": self.elapsed().as_millis(),
        }))
    }
}

#[cfg(test)]
#[path = "deadline_tests.rs"]
mod tests;
