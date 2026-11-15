use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread;
use std::time::{Duration, Instant};

use tabref_core::{Deadline, ErrorCode};

use super::TabLocks;

fn live() -> AtomicBool {
    AtomicBool::new(false)
}

fn deadline(ms: u64) -> Deadline {
    Deadline::after(ms).unwrap()
}

#[test]
fn a_claim_is_reentrant_for_its_owner_and_exclusive_for_others() {
    let locks = TabLocks::default();
    locks.claim("T1", 1, deadline(100), &live()).unwrap();
    locks.claim("T1", 1, deadline(100), &live()).unwrap();
    locks.claim("T2", 2, deadline(100), &live()).unwrap();

    let error = locks.claim("T1", 2, deadline(30), &live()).unwrap_err();
    assert_eq!(error.code, ErrorCode::Timeout);
    assert_eq!(error.details.unwrap()["kind"], "tab_busy");
    assert_eq!(locks.held_count(), 2);
}

#[test]
fn a_waiting_claim_proceeds_once_the_holder_releases() {
    let locks = Arc::new(TabLocks::default());
    locks.claim("T", 1, deadline(100), &live()).unwrap();
    let waiter = {
        let locks = Arc::clone(&locks);
        thread::spawn(move || {
            let started = Instant::now();
            locks.claim("T", 2, deadline(2_000), &live()).unwrap();
            started.elapsed()
        })
    };
    thread::sleep(Duration::from_millis(40));
    locks.release(1);

    let waited = waiter.join().unwrap();
    assert!(waited >= Duration::from_millis(30));
    assert!(waited < Duration::from_millis(1_000));
    locks.release(2);
    assert_eq!(locks.held_count(), 0);
}

#[test]
fn a_cancelled_command_stops_waiting_for_a_busy_tab() {
    let locks = Arc::new(TabLocks::default());
    locks.claim("T", 1, deadline(100), &live()).unwrap();
    let cancel = Arc::new(AtomicBool::new(false));
    let flag = Arc::clone(&cancel);
    let waiter = {
        let locks = Arc::clone(&locks);
        thread::spawn(move || locks.claim("T", 2, deadline(5_000), &flag))
    };
    thread::sleep(Duration::from_millis(30));
    cancel.store(true, Ordering::SeqCst);
    let started = Instant::now();

    let error = waiter.join().unwrap().unwrap_err();

    assert_eq!(error.details.unwrap()["kind"], "cancelled");
    assert!(started.elapsed() < Duration::from_millis(500));
}
