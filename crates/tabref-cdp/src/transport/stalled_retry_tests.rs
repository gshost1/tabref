use std::time::Duration;

use tabref_core::{AppError, Deadline, ErrorCode};

use super::retry_once_if_stalled;
use crate::transport::transport_error::{transport_failed, transport_timeout};

#[test]
fn a_stalled_first_attempt_gets_half_and_a_second_gets_the_rest() {
    let deadline = Deadline::after(3_000).unwrap();
    let mut budgets = Vec::new();
    let outcome = retry_once_if_stalled(deadline, |attempt| {
        budgets.push(attempt.remaining());
        if budgets.len() == 1 {
            return Err(transport_timeout("handshake"));
        }
        Ok(7)
    });

    assert_eq!(outcome.unwrap(), 7);
    assert_eq!(budgets.len(), 2);
    assert!(budgets[0] <= Duration::from_millis(1_500));
    assert!(budgets[1] > Duration::from_millis(2_900));
}

#[test]
fn a_failure_that_is_not_a_stall_is_not_retried() {
    let mut attempts = 0;
    let outcome: Result<(), AppError> =
        retry_once_if_stalled(Deadline::after(3_000).unwrap(), |_| {
            attempts += 1;
            Err(transport_failed("handshake", "refused"))
        });

    assert!(outcome.is_err());
    assert_eq!(attempts, 1);
}

#[test]
fn a_second_stall_is_the_error() {
    let mut attempts = 0;
    let outcome: Result<(), AppError> =
        retry_once_if_stalled(Deadline::after(3_000).unwrap(), |_| {
            attempts += 1;
            Err(transport_timeout("handshake"))
        });

    assert_eq!(outcome.unwrap_err().code, ErrorCode::Timeout);
    assert_eq!(attempts, 2);
}

#[test]
fn a_short_probe_is_one_attempt_with_all_of_its_budget() {
    let mut budgets = Vec::new();
    let outcome: Result<(), AppError> =
        retry_once_if_stalled(Deadline::after(300).unwrap(), |attempt| {
            budgets.push(attempt.remaining());
            Err(transport_timeout("http"))
        });

    assert!(outcome.is_err());
    assert_eq!(budgets.len(), 1);
    assert!(budgets[0] > Duration::from_millis(250));
}
