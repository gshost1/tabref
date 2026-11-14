use serde_json::{Value, json};

use super::{settle_action, watch_navigation};
use crate::CdpConnection;
use crate::act::act_fixture::{deadline, session};
use tabref_cdp::ScriptedTransport;

fn page(method: &str) -> Value {
    json!({ "method": method, "params": { "frameId": "F" }, "sessionId": "S" })
}

fn settle(frames: &[Value]) -> super::SettleOutcome {
    let transport = frames
        .iter()
        .fold(ScriptedTransport::new(), |transport, frame| {
            transport.push_inbound(frame.clone())
        });
    let mut connection = CdpConnection::new(transport.boxed());
    connection.set_answer_dialogs(false);
    settle_action(&mut connection, deadline()).unwrap()
}

#[test]
fn an_action_that_starts_nothing_returns_at_once() {
    let started = std::time::Instant::now();
    let outcome = settle(&[]);
    assert!(!outcome.navigated);
    assert!(started.elapsed() < std::time::Duration::from_millis(500));
}

#[test]
fn a_navigation_that_loads_a_page_is_reported() {
    let outcome = settle(&[
        page("Page.frameRequestedNavigation"),
        page("Page.frameStartedNavigating"),
        page("Page.loadEventFired"),
        page("Page.frameStoppedLoading"),
    ]);
    assert!(outcome.navigated);
    assert!(outcome.dialogs.is_empty());
}

#[test]
fn a_dialog_the_action_opened_is_reported() {
    let outcome = settle(&[json!({ "method": "Page.javascriptDialogOpening",
        "params": { "type": "alert", "message": "Saved" }, "sessionId": "S" })]);
    assert_eq!(
        outcome.dialogs,
        vec![json!({ "type": "alert", "message": "Saved" })]
    );
}

#[test]
fn watching_navigation_enables_page_events_on_the_session() {
    let transport = ScriptedTransport::new().expect("Page.enable", json!({}));
    let sent = transport.sent_log();
    let mut connection = CdpConnection::new(transport.boxed());

    watch_navigation(&mut connection, &session(), deadline()).unwrap();

    assert_eq!(sent.lock().unwrap()[0]["sessionId"], "S");
}
