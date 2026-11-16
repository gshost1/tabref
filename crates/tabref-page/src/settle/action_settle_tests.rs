use std::time::{Duration, Instant};

use serde_json::{Value, json};

use super::{settle_action, watch_navigation};
use crate::CdpConnection;
use crate::act::act_fixture::{deadline, session};
use tabref_cdp::ScriptedTransport;

fn page(method: &str, params: Value) -> Value {
    json!({ "method": method, "params": params, "sessionId": "S" })
}

fn turn_reply() -> Value {
    json!({ "result": { "result": { "type": "number", "value": 0 } } })
}

fn settle(events: Vec<Value>) -> super::SettleOutcome {
    let transport = ScriptedTransport::new()
        .expect_with_events("Runtime.evaluate", events, turn_reply())
        .expect("Network.disable", json!({}));
    let mut connection = CdpConnection::new(transport.boxed());
    connection.set_answer_dialogs(false);
    settle_action(&mut connection, &session(), "F", deadline()).unwrap()
}

#[test]
fn an_action_that_starts_nothing_costs_one_page_turn() {
    let started = Instant::now();
    let outcome = settle(Vec::new());
    assert!(!outcome.navigated);
    assert!(!outcome.network);
    assert!(started.elapsed() < Duration::from_millis(50));
}

#[test]
fn a_navigation_is_followed_until_it_commits_and_its_content_loads() {
    let outcome = settle(vec![
        page(
            "Page.frameRequestedNavigation",
            json!({ "frameId": "F", "disposition": "currentTab" }),
        ),
        page(
            "Page.frameNavigated",
            json!({ "frame": { "id": "F", "loaderId": "L2" } }),
        ),
        page("Page.domContentEventFired", json!({ "timestamp": 1 })),
    ]);
    assert!(outcome.navigated);
    assert!(outcome.dialogs.is_empty());
}

#[test]
fn a_link_that_opens_a_new_tab_is_not_a_navigation_of_this_tab() {
    let outcome = settle(vec![page(
        "Page.frameRequestedNavigation",
        json!({ "frameId": "F", "disposition": "newTab" }),
    )]);
    assert!(!outcome.navigated);
}

#[test]
fn a_dialog_the_action_opened_is_reported() {
    let outcome = settle(vec![page(
        "Page.javascriptDialogOpening",
        json!({ "type": "alert", "message": "Saved" }),
    )]);
    assert_eq!(
        outcome.dialogs,
        vec![json!({ "type": "alert", "message": "Saved", "url": null, "answer": null })]
    );
}

#[test]
fn watching_enables_page_and_network_events_and_names_the_main_frame() {
    let transport = ScriptedTransport::new()
        .expect("Page.enable", json!({}))
        .expect("Network.enable", json!({}))
        .expect(
            "Page.getFrameTree",
            json!({ "frameTree": { "frame": { "id": "MAIN", "loaderId": "L" } } }),
        );
    let sent = transport.sent_log();
    let mut connection = CdpConnection::new(transport.boxed());

    let main = watch_navigation(&mut connection, &session(), deadline()).unwrap();

    assert_eq!(main, "MAIN");
    assert_eq!(sent.lock().unwrap()[0]["sessionId"], "S");
}
