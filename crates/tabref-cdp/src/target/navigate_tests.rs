use serde_json::json;
use tabref_core::{Deadline, ErrorCode};

use std::time::Duration;

use super::{load_bound, navigate, navigate_by, wait_for_load};
use crate::ScriptedTransport;
use crate::{CdpConnection, PageSession};

fn session() -> PageSession {
    PageSession {
        target_id: "T".into(),
        session_id: "S".into(),
    }
}

fn enabled() -> ScriptedTransport {
    ScriptedTransport::new()
        .expect("Page.enable", json!({}))
        .expect("Page.setLifecycleEventsEnabled", json!({}))
}

#[test]
fn navigation_waits_for_the_matching_lifecycle_load() {
    let transport = enabled().expect_with_events(
        "Page.navigate",
        vec![
            json!({ "method": "Page.lifecycleEvent", "sessionId": "S",
                "params": { "name": "load", "frameId": "F", "loaderId": "L1" } }),
            json!({ "method": "Page.lifecycleEvent", "sessionId": "OTHER",
                "params": { "name": "load", "frameId": "F", "loaderId": "L2" } }),
            json!({ "method": "Page.lifecycleEvent", "sessionId": "S",
                "params": { "name": "load", "frameId": "F", "loaderId": "L2" } }),
        ],
        json!({ "result": { "frameId": "F", "loaderId": "L2" } }),
    );
    let mut connection = CdpConnection::new(transport.boxed());

    navigate(
        &mut connection,
        &session(),
        "https://a.test/",
        Deadline::after(500).unwrap(),
    )
    .unwrap();

    assert_eq!(connection.take_events().len(), 2);
}

#[test]
fn a_navigation_error_text_fails_without_waiting() {
    let transport = enabled().expect(
        "Page.navigate",
        json!({ "frameId": "F", "loaderId": "L", "errorText": "net::ERR_NAME_NOT_RESOLVED" }),
    );
    let mut connection = CdpConnection::new(transport.boxed());

    let error = navigate(
        &mut connection,
        &session(),
        "https://nope.test/",
        Deadline::after(500).unwrap(),
    )
    .unwrap_err();

    assert_eq!(error.code, ErrorCode::ActionFailed);
    assert_eq!(
        error.platform_detail.as_deref(),
        Some("net::ERR_NAME_NOT_RESOLVED")
    );
}

#[test]
fn a_missing_load_is_a_bounded_timeout() {
    let transport = enabled().expect("Page.navigate", json!({ "frameId": "F", "loaderId": "L" }));
    let mut connection = CdpConnection::new(transport.boxed());

    let error = navigate(
        &mut connection,
        &session(),
        "https://slow.test/",
        Deadline::after(50).unwrap(),
    )
    .unwrap_err();

    assert_eq!(error.code, ErrorCode::Timeout);
    assert_eq!(error.details.as_ref().unwrap()["kind"], "cdp_load_timeout");
}

#[test]
fn a_loaded_document_needs_no_wait() {
    let transport = enabled().expect(
        "Runtime.evaluate",
        json!({ "result": { "type": "string", "value": "complete" } }),
    );
    let mut connection = CdpConnection::new(transport.boxed());

    wait_for_load(&mut connection, &session(), Deadline::after(500).unwrap()).unwrap();
}

fn lifecycle_load(loader_id: &str) -> serde_json::Value {
    json!({ "method": "Page.lifecycleEvent", "sessionId": "S",
        "params": { "name": "load", "frameId": "F", "loaderId": loader_id } })
}

fn stale_loads() -> Vec<serde_json::Value> {
    vec![
        json!({ "method": "Page.loadEventFired", "sessionId": "S", "params": {} }),
        lifecycle_load("L1"),
    ]
}

fn loading(
    transport: ScriptedTransport,
    frame_events: Vec<serde_json::Value>,
) -> ScriptedTransport {
    transport
        .expect(
            "Runtime.evaluate",
            json!({ "result": { "type": "string", "value": "loading" } }),
        )
        .expect_with_events(
            "Page.getFrameTree",
            frame_events,
            json!({ "result": { "frameTree": { "frame": { "id": "F", "loaderId": "L2" } } } }),
        )
}

#[test]
fn a_loading_document_waits_for_its_own_load_event() {
    let mut events = stale_loads();
    events.push(lifecycle_load("L2"));
    let transport = loading(enabled(), events);
    let mut connection = CdpConnection::new(transport.boxed());

    wait_for_load(&mut connection, &session(), Deadline::after(500).unwrap()).unwrap();
}

#[test]
fn buffered_loads_of_an_earlier_document_do_not_end_the_wait() {
    let mut transport = ScriptedTransport::new();
    for event in stale_loads() {
        transport = transport.push_inbound(event);
    }
    let transport = loading(
        transport
            .expect("Page.enable", json!({}))
            .expect("Page.setLifecycleEventsEnabled", json!({})),
        Vec::new(),
    );
    let mut connection = CdpConnection::new(transport.boxed());

    let error =
        wait_for_load(&mut connection, &session(), Deadline::after(50).unwrap()).unwrap_err();

    assert_eq!(error.code, ErrorCode::Timeout);
}

#[test]
fn a_navigation_the_browser_never_starts_is_a_pending_navigation() {
    let transport = enabled().expect_silence("Page.navigate");
    let mut connection = CdpConnection::new(transport.boxed());

    let error = navigate_by(
        &mut connection,
        &session(),
        "http://127.0.0.1:1/",
        Deadline::after(50).unwrap(),
        Deadline::after(10).unwrap(),
    )
    .unwrap_err();

    assert_eq!(error.code, ErrorCode::Timeout);
    assert!(
        error.message.contains("did not respond"),
        "{}",
        error.message
    );
    assert!(!error.suggestion.clone().unwrap().contains("repeat"));
    let details = error.details.unwrap();
    assert_eq!(details["kind"], "cdp_navigation_pending");
    assert!(details.get("retryable").is_none());
}

#[test]
fn a_commit_just_before_the_load_bound_still_gets_half_a_second_to_load() {
    let commit = Deadline::after(5_000).unwrap();

    let nearly_spent = load_bound(commit, Deadline::after(50).unwrap());
    let roomy = load_bound(commit, Deadline::after(3_000).unwrap());
    let tight = load_bound(Deadline::after(200).unwrap(), Deadline::after(50).unwrap());

    assert!(nearly_spent.remaining() > Duration::from_millis(450));
    assert!(nearly_spent.remaining() <= Duration::from_millis(500));
    assert!(roomy.remaining() > Duration::from_millis(2_900));
    assert!(tight.remaining() <= Duration::from_millis(200));
    assert!(tight.remaining() > Duration::from_millis(100));
}

#[test]
fn a_commit_after_the_load_bound_still_waits_for_its_document() {
    let transport = enabled().expect_with_events(
        "Page.navigate",
        vec![json!({ "method": "Page.lifecycleEvent", "sessionId": "S",
            "params": { "name": "DOMContentLoaded", "frameId": "F", "loaderId": "L2" } })],
        json!({ "result": { "frameId": "F", "loaderId": "L2" } }),
    );
    let mut connection = CdpConnection::new(transport.boxed());

    navigate_by(
        &mut connection,
        &session(),
        "https://a.test/",
        Deadline::after(500).unwrap(),
        Deadline::after(0).unwrap(),
    )
    .unwrap();
}

#[test]
fn a_late_load_of_the_previous_document_does_not_end_a_navigation() {
    let transport = enabled().expect_with_events(
        "Page.navigate",
        stale_loads(),
        json!({ "result": { "frameId": "F", "loaderId": "L2" } }),
    );
    let mut connection = CdpConnection::new(transport.boxed());

    let error = navigate(
        &mut connection,
        &session(),
        "https://a.test/",
        Deadline::after(50).unwrap(),
    )
    .unwrap_err();

    assert_eq!(error.code, ErrorCode::Timeout);
}
