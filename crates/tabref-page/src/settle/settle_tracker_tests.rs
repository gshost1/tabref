use std::time::{Duration, Instant};

use serde_json::{Value, json};

use super::{BODY_GRACE, SettleTracker};
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
    assert_eq!(
        tracker.next_wait(now, QUIET),
        SettleWait::QuietUntil(now + QUIET)
    );
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

#[test]
fn requests_to_other_sites_or_from_blank_frames_do_not_hold_the_settle() {
    let now = Instant::now();
    let mut tracker = SettleTracker::new("F");
    let page = "https://dictionary.example.org/word";
    for (id, frame, document, url) in [
        ("1", "F", page, "https://ads.adnet.example.com/bid"),
        ("2", "AD", "about:blank", "https://www.example.org/api"),
    ] {
        tracker.absorb(
            &event(
                "Network.requestWillBeSent",
                json!({ "requestId": id, "type": "XHR", "frameId": frame,
                        "documentURL": document, "request": { "url": url } }),
            ),
            now,
        );
    }
    assert_eq!(tracker.next_wait(now, QUIET), SettleWait::Done);
    tracker.absorb(
        &event(
            "Network.requestWillBeSent",
            json!({ "requestId": "4", "type": "Fetch", "frameId": "EDITOR",
                    "documentURL": "https://www.example.org/editor", "request": { "url": "https://www.example.org/save" } }),
        ),
        now,
    );
    assert_eq!(tracker.next_wait(now, QUIET), SettleWait::Network);
    tracker.absorb(
        &event(
            "Network.requestWillBeSent",
            json!({ "requestId": "3", "type": "Fetch", "frameId": "F",
                    "documentURL": page, "request": { "url": "https://api.example.org/save" } }),
        ),
        now,
    );
    assert_eq!(tracker.next_wait(now, QUIET), SettleWait::Network);
}

#[test]
fn a_document_answered_as_an_attachment_stops_holding_the_settle() {
    let now = Instant::now();
    let mut tracker = SettleTracker::new("F");
    tracker.absorb(
        &event("Page.frameStartedLoading", json!({ "frameId": "F" })),
        now,
    );
    tracker.absorb(
        &event(
            "Network.requestWillBeSent",
            json!({ "requestId": "d", "type": "Document" }),
        ),
        now,
    );
    tracker.absorb(
        &event(
            "Network.responseReceived",
            json!({ "requestId": "d", "type": "Document", "frameId": "F", "response": {
                "headers": { "Content-Disposition": "attachment; filename=\"slow.bin\"" }
            } }),
        ),
        now,
    );
    assert_eq!(tracker.next_wait(now + QUIET, QUIET), SettleWait::Done);
}

fn fetch_answered(tracker: &mut SettleTracker, now: Instant, status: u64, headers: Value) {
    tracker.absorb(
        &event(
            "Network.requestWillBeSent",
            json!({ "requestId": "r", "type": "Fetch" }),
        ),
        now,
    );
    tracker.absorb(
        &event(
            "Network.responseReceived",
            json!({ "requestId": "r", "type": "Fetch", "frameId": "F",
                    "response": { "status": status, "headers": headers } }),
        ),
        now,
    );
}

fn data(tracker: &mut SettleTracker, now: Instant, length: u64) {
    tracker.absorb(
        &event(
            "Network.dataReceived",
            json!({ "requestId": "r", "dataLength": length, "encodedDataLength": length }),
        ),
        now,
    );
}

#[test]
fn a_response_whose_every_byte_arrived_stops_holding_the_settle_without_loading_finished() {
    let now = Instant::now();
    let mut tracker = SettleTracker::new("F");
    fetch_answered(
        &mut tracker,
        now,
        200,
        json!({ "Content-Length": "12", "Cache-Control": "no-store" }),
    );
    assert_eq!(tracker.next_wait(now, QUIET), SettleWait::Network);
    data(&mut tracker, now, 5);
    assert_eq!(tracker.next_wait(now, QUIET), SettleWait::Network);
    data(&mut tracker, now, 7);
    assert_eq!(
        tracker.next_wait(now, QUIET),
        SettleWait::QuietUntil(now + BODY_GRACE)
    );
    assert_eq!(
        tracker.next_wait(now + QUIET, QUIET),
        SettleWait::QuietUntil(now + BODY_GRACE)
    );
    assert_eq!(tracker.next_wait(now + BODY_GRACE, QUIET), SettleWait::Done);
}

#[test]
fn loading_finished_inside_the_grace_ends_the_request_at_once() {
    let now = Instant::now();
    let mut tracker = SettleTracker::new("F");
    fetch_answered(&mut tracker, now, 200, json!({ "Content-Length": "12" }));
    data(&mut tracker, now, 12);
    let read = now + Duration::from_millis(150);
    tracker.absorb(
        &event("Network.loadingFinished", json!({ "requestId": "r" })),
        read,
    );
    assert_eq!(
        tracker.next_wait(read, QUIET),
        SettleWait::QuietUntil(read + QUIET)
    );
    assert_eq!(tracker.next_wait(read + QUIET, QUIET), SettleWait::Done);
}

#[test]
fn a_request_still_in_flight_holds_the_settle_past_another_ones_grace() {
    let now = Instant::now();
    let mut tracker = SettleTracker::new("F");
    fetch_answered(&mut tracker, now, 200, json!({ "Content-Length": "12" }));
    data(&mut tracker, now, 12);
    tracker.absorb(
        &event(
            "Network.requestWillBeSent",
            json!({ "requestId": "s", "type": "Fetch" }),
        ),
        now,
    );
    assert_eq!(
        tracker.next_wait(now + BODY_GRACE, QUIET),
        SettleWait::Network
    );
}

#[test]
fn a_response_with_no_body_stops_holding_the_settle_when_it_arrives() {
    let now = Instant::now();
    for (status, headers) in [
        (204, json!({})),
        (200, json!({ "content-length": "0" })),
        (304, json!({})),
    ] {
        let mut tracker = SettleTracker::new("F");
        fetch_answered(&mut tracker, now, status, headers);
        assert_eq!(
            tracker.next_wait(now + QUIET, QUIET),
            SettleWait::Done,
            "{status}"
        );
    }
}

#[test]
fn a_response_of_unknown_or_encoded_length_waits_for_loading_finished() {
    let now = Instant::now();
    let mut tracker = SettleTracker::new("F");
    fetch_answered(
        &mut tracker,
        now,
        200,
        json!({ "Transfer-Encoding": "chunked" }),
    );
    data(&mut tracker, now, 4_096);
    assert_eq!(tracker.next_wait(now, QUIET), SettleWait::Network);
    let mut tracker = SettleTracker::new("F");
    fetch_answered(
        &mut tracker,
        now,
        200,
        json!({ "Content-Length": "100", "Content-Encoding": "gzip" }),
    );
    tracker.absorb(
        &event(
            "Network.dataReceived",
            json!({ "requestId": "r", "dataLength": 400, "encodedDataLength": 0 }),
        ),
        now,
    );
    assert_eq!(tracker.next_wait(now, QUIET), SettleWait::Network);
    tracker.absorb(
        &event(
            "Network.dataReceived",
            json!({ "requestId": "r", "dataLength": 0, "encodedDataLength": 100 }),
        ),
        now,
    );
    assert_eq!(
        tracker.next_wait(now, QUIET),
        SettleWait::QuietUntil(now + BODY_GRACE)
    );
}
