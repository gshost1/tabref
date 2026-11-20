use std::time::{Duration, Instant};

use serde_json::{Value, json};

use super::{EventClock, LoadSettle, SettleScope, settle_load};
use crate::act::act_fixture::{deadline, session};
use crate::settle::load_watch::{LoadWatch, parse_status};
use crate::{CdpConnection, CdpEvent};
use tabref_cdp::{ScriptedTransport, is_timeout};

fn status(since: Option<f64>, empty: bool) -> Value {
    json!({ "result": { "result": { "type": "object", "value": {
        "wall": 1_000_000.0, "since": since, "empty": empty, "watched": true
    } } } })
}

fn network(method: &str, params: Value) -> Value {
    json!({ "method": method, "params": params, "sessionId": "S" })
}

fn event(params: Value) -> CdpEvent {
    CdpEvent {
        method: "Network.loadingFinished".into(),
        params,
        session_id: Some("S".into()),
    }
}

fn watch(script: Option<&str>) -> LoadWatch {
    LoadWatch {
        session: session(),
        script: script.map(str::to_owned),
    }
}

fn settle(transport: ScriptedTransport, script: Option<&str>) -> LoadSettle {
    settle_as(transport, script, true)
}

fn settle_as(transport: ScriptedTransport, script: Option<&str>, plain: bool) -> LoadSettle {
    let mut connection = CdpConnection::new(transport.boxed());
    connection.set_answer_dialogs(false);
    let scope = SettleScope {
        plain,
        reserve: Duration::ZERO,
    };
    settle_load(&mut connection, watch(script), deadline(), scope)
}

fn sent(id: &str, kind: &str, frame: &str) -> Value {
    network(
        "Network.requestWillBeSent",
        json!({ "requestId": id, "type": kind, "frameId": frame, "timestamp": 10.0, "wallTime": 999.9,
                "documentURL": "https://a.example/", "request": { "url": "https://a.example/x" } }),
    )
}

fn stops(transport: ScriptedTransport) -> ScriptedTransport {
    transport
        .expect("Runtime.evaluate", json!({ "result": { "value": true } }))
        .expect("Network.disable", json!({}))
}

#[test]
fn a_page_with_nothing_left_to_do_costs_one_read() {
    let started = Instant::now();
    let transport = ScriptedTransport::new()
        .expect("Page.removeScriptToEvaluateOnNewDocument", json!({}))
        .expect_with_events("Runtime.evaluate", Vec::new(), status(None, false));
    let outcome = settle(stops(transport), Some("1"));
    assert!(outcome.settled);
    assert!(!outcome.still_rendering());
    assert!(started.elapsed() < Duration::from_millis(100));
}

#[test]
fn a_script_registered_after_the_start_timed_out_is_removed() {
    let transport = ScriptedTransport::new()
        .expect("Page.enable", json!({}))
        .expect_silence("Page.addScriptToEvaluateOnNewDocument")
        .expect_with_events(
            "Page.navigate",
            vec![json!({ "id": 2, "sessionId": "S", "result": { "identifier": "late" } })],
            json!({ "result": { "frameId": "T" } }),
        )
        .expect("Page.removeScriptToEvaluateOnNewDocument", json!({}));
    let sent = transport.sent_log();
    let mut connection = CdpConnection::new(transport.boxed());
    let error = LoadWatch::start(&mut connection, &session(), deadline()).unwrap_err();
    assert!(is_timeout(&error), "{error:?}");

    let reply = connection
        .call(
            "Page.navigate",
            json!({ "url": "https://a.example/" }),
            Some("S"),
            deadline(),
        )
        .unwrap();
    assert_eq!(reply["frameId"], "T");
    connection.poll_pending(deadline()).unwrap();
    let requests = sent.lock().unwrap();
    assert_eq!(requests.len(), 4, "{requests:?}");
    assert_eq!(requests[3]["params"]["identifier"], "late");
    assert_eq!(requests[3]["sessionId"], "S");
}

#[test]
fn fetches_that_ended_long_before_the_read_hold_nothing() {
    let sent = network(
        "Network.requestWillBeSent",
        json!({ "requestId": "r1", "type": "Fetch", "timestamp": 10.0, "wallTime": 900.0,
                "documentURL": "https://a.example/", "request": { "url": "https://a.example/data" } }),
    );
    let finished = network(
        "Network.loadingFinished",
        json!({ "requestId": "r1", "timestamp": 10.2 }),
    );
    let ignored = network(
        "Network.requestWillBeSent",
        json!({ "requestId": "d1", "type": "Document", "timestamp": 10.0, "wallTime": 900.0,
                "documentURL": "https://ads.example/", "request": { "url": "https://ads.example/slot" } }),
    );
    let transport = ScriptedTransport::new().expect_with_events(
        "Runtime.evaluate",
        vec![sent, finished, ignored],
        status(Some(500.0), false),
    );
    let outcome = settle(stops(transport), None);
    assert!(outcome.settled, "{outcome:?}");
}

#[test]
fn a_page_that_cannot_be_read_ends_the_wait_and_the_watch() {
    let transport = ScriptedTransport::new().expect(
        "Runtime.evaluate",
        json!({ "exceptionDetails": { "text": "gone" } }),
    );
    let outcome = settle(stops(transport), None);
    assert!(!outcome.settled);
    assert!(!outcome.near_empty);
}

#[test]
fn the_page_clock_turns_event_times_into_ages() {
    let read_at = Instant::now();
    let mut clock = EventClock::new(1_000_500.0, read_at);
    let finished = event(json!({ "timestamp": 100.2 }));
    assert_eq!(clock.when(&finished), read_at, "uncalibrated counts as now");
    clock.calibrate(&event(json!({ "timestamp": 100.0, "wallTime": 1_000.0 })));
    let age = read_at - clock.when(&finished);
    assert!(
        (age.as_secs_f64() - 0.3).abs() < 0.001,
        "age {age:?}, expected 300 ms"
    );
    let future = event(json!({ "timestamp": 101.0 }));
    assert_eq!(clock.when(&future), read_at, "never later than the read");
}

#[test]
fn the_status_reads_the_page_clock_the_last_change_and_emptiness() {
    let read = parse_status(&status(Some(120.0), true)["result"]).unwrap();
    assert_eq!(read.since_mutation, Some(Duration::from_millis(120)));
    assert!(read.near_empty);
    let quiet = parse_status(&status(None, false)["result"]).unwrap();
    assert_eq!(quiet.since_mutation, None);
    assert!(parse_status(&json!({ "exceptionDetails": {} })).is_none());
}

#[test]
fn a_second_document_request_of_the_main_frame_ends_the_wait() {
    let transport = ScriptedTransport::new().expect_with_events(
        "Runtime.evaluate",
        vec![
            sent("L1", "Document", "T"),
            sent("f1", "Fetch", "T"),
            sent("L2", "Document", "T"),
        ],
        status(None, false),
    );
    let outcome = settle(stops(transport), None);
    assert!(outcome.navigated, "{outcome:?}");
    assert!(!outcome.settled);
}

#[test]
fn a_frame_document_is_no_navigation() {
    let transport = ScriptedTransport::new().expect_with_events(
        "Runtime.evaluate",
        vec![sent("L1", "Document", "T"), sent("A1", "Document", "ad")],
        status(None, false),
    );
    let outcome = settle(stops(transport), None);
    assert!(!outcome.navigated, "{outcome:?}");
    assert!(outcome.settled);
}

#[test]
fn without_a_kept_capture_a_full_page_is_not_waited_for() {
    let started = Instant::now();
    let transport = ScriptedTransport::new().expect_with_events(
        "Runtime.evaluate",
        vec![sent("f1", "Fetch", "T")],
        status(None, false),
    );
    let outcome = settle_as(stops(transport), None, false);
    assert!(!outcome.settled);
    assert!(!outcome.still_rendering());
    assert!(started.elapsed() < Duration::from_millis(100));
}

#[test]
fn without_a_kept_capture_a_recent_mutation_does_not_cause_a_second_read() {
    let transport = ScriptedTransport::new().expect_with_events(
        "Runtime.evaluate",
        Vec::new(),
        status(Some(0.0), false),
    );
    let outcome = settle_as(stops(transport), None, false);
    assert!(!outcome.settled);
    assert!(!outcome.near_empty);
}

#[test]
fn only_a_near_empty_page_still_fetching_is_still_rendering() {
    let settle = |rendering: bool, near_empty: bool| LoadSettle {
        settled: !rendering,
        near_empty,
        rendering,
        navigated: false,
        waited: Duration::ZERO,
    };
    assert!(settle(true, true).still_rendering());
    assert!(!settle(false, true).still_rendering());
}
