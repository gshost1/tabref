use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use super::ServerState;

#[test]
fn a_stop_cancels_every_running_request_and_any_that_arrives_after() {
    let state = ServerState::new("build".to_owned(), None);
    let running = Arc::new(AtomicBool::new(false));
    let finished = Arc::new(AtomicBool::new(false));
    state.track(&running);
    state.track(&finished);
    drop(finished);

    state.shut_down();
    let late = Arc::new(AtomicBool::new(false));
    state.track(&late);

    assert!(running.load(Ordering::SeqCst));
    assert!(late.load(Ordering::SeqCst));
}
