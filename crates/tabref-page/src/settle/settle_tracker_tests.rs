use std::time::{Duration, Instant};

use serde_json::{Value, json};

use super::SettleTracker;
use crate::CdpEvent;
use crate::settle::settle_wait::SettleWait;

const QUIET: Duration = Duration::from_millis(50);

fn event(method: &str, params: Value) -> CdpEvent {
    CdpEvent {
        method: method.into(),
        params,
        session_id: Some("S".into()),
    }
}

#[test]
fn requests_hold_the_settle_until_they_finish_and_the_page_is_quiet() {
    let now = Instant::now();
    let mut tracker = SettleTracker::new("F");
    tracker.absorb(
        &event(
            "Network.requestWillBeSent",
            json!({ "requestId": "1", "type": "Fetch" }),
        ),
        now,
    );
    assert_eq!(tracker.next_wait(now, QUIET), SettleWait::Network);
    tracker.absorb(
        &event("Network.loadingFinished", json!({ "requestId": "1" })),
        now,
    );
    assert_eq!(tracker.next_wait(now, QUIET), SettleWait::QuietUntil(now + QUIET));
    assert_eq!(tracker.next_wait(now + QUIET, QUIET), SettleWait::Done);
    assert!(tracker.saw_network());
}

#[test]
fn images_and_beacons_do_not_hold_the_settle() {
    let now = Instant::now();
    let mut tracker = SettleTracker::new("F");
    tracker.absorb(
        &event(
            "Network.requestWillBeSent",
            json!({ "requestId": "1", "type": "Image" }),
        ),
        now,
    );
    assert_eq!(tracker.next_wait(now, QUIET), SettleWait::Done);
}

#[test]
fn an_iframe_navigation_is_not_the_tab_navigating() {
    let now = Instant::now();
    let mut tracker = SettleTracker::new("F");
    tracker.absorb(
        &event("Page.frameStartedLoading", json!({ "frameId": "AD" })),
        now,
    );
    tracker.absorb(
        &event("Page.frameNavigated", json!({ "frame": { "id": "AD" } })),
        now,
    );
    assert_eq!(tracker.next_wait(now, QUIET), SettleWait::Done);
    assert!(!tracker.committed());
}

#[test]
fn a_navigation_that_never_commits_ends_when_loading_stops() {
    let now = Instant::now();
    let mut tracker = SettleTracker::new("F");
    tracker.absorb(
        &event("Page.frameStartedLoading", json!({ "frameId": "F" })),
        now,
    );
    assert_eq!(tracker.next_wait(now, QUIET), SettleWait::Navigation);
    tracker.absorb(
        &event("Page.frameStoppedLoading", json!({ "frameId": "F" })),
        now,
    );
    assert_eq!(tracker.next_wait(now, QUIET), SettleWait::Done);
}

#[test]
fn a_route_change_within_the_document_is_noted() {
    let now = Instant::now();
    let mut tracker = SettleTracker::new("F");
    tracker.absorb(
        &event(
            "Page.navigatedWithinDocument",
            json!({ "frameId": "F", "url": "http://x/b" }),
        ),
        now,
    );
    assert!(tracker.same_document());
    assert_eq!(tracker.next_wait(now, QUIET), SettleWait::Done);
}
