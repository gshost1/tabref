use std::time::Duration;

use super::Deadline;
use crate::ErrorCode;

#[test]
fn a_fresh_deadline_has_its_time_left() {
    let deadline = Deadline::after(5_000).unwrap();
    assert!(!deadline.is_expired());
    assert!(deadline.remaining() <= Duration::from_millis(5_000));
    assert!(deadline.remaining_ms() > 4_000);
    assert_eq!(deadline.timeout_ms(), 5_000);
}

#[test]
fn a_zero_deadline_is_expired_at_once() {
    let deadline = Deadline::after(0).unwrap();
    assert!(deadline.is_expired());
    assert_eq!(deadline.timeout_error().code, ErrorCode::Timeout);
}

#[test]
fn capping_only_ever_shortens() {
    let deadline = Deadline::after(10_000).unwrap();
    assert!(deadline.capped(Duration::from_millis(50)).remaining() <= Duration::from_millis(50));
    let short = Deadline::after(20).unwrap();
    assert!(short.capped(Duration::from_secs(60)).remaining() <= Duration::from_millis(20));
}

#[test]
fn an_unrepresentable_timeout_is_invalid_args() {
    let error = Deadline::after(u64::MAX).unwrap_err();
    assert_eq!(error.code, ErrorCode::InvalidArgs);
}
