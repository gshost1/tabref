//! One retry for a connection that stalls before it is set up. A browser
//! that has just started, or one starved of CPU, can take a TCP connection
//! and not answer on it; waiting out the whole deadline on that one socket
//! turns a hiccup into a failed command. The first attempt gets half of what
//! remains and a stalled one is dropped; the second, on a fresh connection,
//! gets the rest. A refused or failed attempt is never retried, and a
//! budget under two seconds (a quick probe) is not split.

use std::time::Duration;

use tabref_core::{AppError, Deadline};

use super::transport_error::is_timeout;

const MIN_SPLIT: Duration = Duration::from_secs(2);

/// Runs `attempt` with half of `deadline`'s remaining time, and once more
/// with the rest when that attempt timed out.
pub(crate) fn retry_once_if_stalled<T>(
    deadline: Deadline,
    mut attempt: impl FnMut(Deadline) -> Result<T, AppError>,
) -> Result<T, AppError> {
    if deadline.remaining() < MIN_SPLIT {
        return attempt(deadline);
    }
    let first = deadline.capped(deadline.remaining() / 2);
    match attempt(first) {
        Err(error) if is_timeout(&error) && !deadline.is_expired() => attempt(deadline),
        outcome => outcome,
    }
}

#[cfg(test)]
#[path = "stalled_retry_tests.rs"]
mod tests;
