use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use serde_json::json;
use tabref_core::{AppError, Deadline};

use crate::{CdpConnection, ConnectionGuard, ScriptedTransport};

fn guard_on(tripped: &Arc<AtomicBool>) -> ConnectionGuard {
    let tripped = Arc::clone(tripped);
    Arc::new(move || {
        if tripped.load(Ordering::SeqCst) {
            Err(AppError::internal("the permission was withdrawn"))
        } else {
            Ok(())
        }
    })
}

#[test]
fn a_tripped_guard_stops_a_request_before_it_is_sent() {
    let mut connection = CdpConnection::new(ScriptedTransport::new().boxed());
    let tripped = Arc::new(AtomicBool::new(true));
    connection.set_guard(Some(guard_on(&tripped)));

    let error = connection
        .call(
            "Page.navigate",
            json!({}),
            Some("S"),
            Deadline::after(500).unwrap(),
        )
        .unwrap_err();

    assert_eq!(error.message, "the permission was withdrawn");
}

#[test]
fn a_guard_that_trips_during_a_wait_ends_it() {
    let transport = ScriptedTransport::new().expect_silence("Runtime.evaluate");
    let mut connection = CdpConnection::new(transport.boxed());
    let tripped = Arc::new(AtomicBool::new(false));
    connection.set_guard(Some(guard_on(&tripped)));
    let flip = Arc::clone(&tripped);
    let flipper = std::thread::spawn(move || {
        std::thread::sleep(Duration::from_millis(80));
        flip.store(true, Ordering::SeqCst);
    });

    let started = Instant::now();
    let error = connection
        .call(
            "Runtime.evaluate",
            json!({}),
            Some("S"),
            Deadline::after(5_000).unwrap(),
        )
        .unwrap_err();

    flipper.join().unwrap();
    assert_eq!(error.message, "the permission was withdrawn");
    assert!(started.elapsed() < Duration::from_secs(2));
}
