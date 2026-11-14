use std::collections::hash_map::RandomState;
use std::hash::BuildHasher;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use crate::AppError;

const DIGITS: &[u8; 36] = b"0123456789abcdefghijklmnopqrstuvwxyz";
const ID_DIGITS: usize = 7;

static COUNTER: AtomicU64 = AtomicU64::new(0);

/// A fresh snapshot id: `s` and seven base-36 digits mixed from the time,
/// the process id, a per-process counter and a random hasher seed, so two
/// invocations in the same instant still differ.
pub fn new_snapshot_id() -> String {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|elapsed| elapsed.as_nanos())
        .unwrap_or(0);
    let counter = COUNTER.fetch_add(1, Ordering::Relaxed);
    let mut value = RandomState::new().hash_one((nanos, std::process::id(), counter));
    let mut id = String::with_capacity(ID_DIGITS + 1);
    id.push('s');
    for _ in 0..ID_DIGITS {
        let digit = usize::try_from(value % 36).unwrap_or(0);
        id.push(char::from(DIGITS[digit]));
        value /= 36;
    }
    id
}

/// Accepts the ids `new_snapshot_id` makes and any other short token of
/// letters, digits, `-` and `_`, which is all a snapshot directory name may
/// hold.
pub fn validate_snapshot_id(snapshot_id: &str) -> Result<(), AppError> {
    let valid = (3..=64).contains(&snapshot_id.len())
        && snapshot_id
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_');
    if valid {
        return Ok(());
    }
    Err(AppError::invalid_args(format!(
        "Invalid snapshot id '{snapshot_id}'"
    )))
}

#[cfg(test)]
#[path = "snapshot_id_tests.rs"]
mod tests;
