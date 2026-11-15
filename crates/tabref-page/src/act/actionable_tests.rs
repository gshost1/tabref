use serde_json::{Value, json};
use tabref_core::{Deadline, ErrorCode};

use super::wait_actionable;
use crate::act::act_fixture::{deadline, resolved, session, token};
use crate::act::blocker::Blocker;
use crate::{CdpConnection, LocateIntent};
use tabref_cdp::ScriptedTransport;

fn checked(transport: ScriptedTransport, state: Value) -> ScriptedTransport {
    transport.expect(
        "Runtime.callFunctionOn",
        json!({ "result": { "value": state } }),
    )
}

fn released(transport: ScriptedTransport) -> ScriptedTransport {
    transport.expect("Runtime.releaseObject", json!({}))
}

fn calls(sent: &[Value], method: &str) -> usize {
    sent.iter()
        .filter(|request| request["method"] == method)
        .count()
}

#[test]
fn a_ready_element_costs_one_check_and_no_wait() {
    let transport = released(checked(
        resolved(ScriptedTransport::new()),
        json!({ "ready": true }),
    ));
    let sent = transport.sent_log();
    let mut connection = CdpConnection::new(transport.boxed());

    let blocked = wait_actionable(
        &mut connection,
        &session(),
        &token(3),
        LocateIntent::Click,
        deadline(),
    )
    .unwrap();

    assert_eq!(blocked, None);
    let sent = sent.lock().unwrap();
    assert_eq!(calls(&sent, "Runtime.callFunctionOn"), 1);
    let need = &sent[2]["params"]["arguments"][0]["value"];
    assert_eq!(need["enabled"], true);
    assert_eq!(need["editable"], false);
}

#[test]
fn an_element_that_becomes_enabled_is_waited_for_on_one_resolve() {
    let disabled = json!({ "blocker": "disabled", "detail": "disabled" });
    let transport = released(checked(
        checked(
            checked(resolved(ScriptedTransport::new()), disabled.clone()),
            disabled,
        ),
        json!({ "ready": true }),
    ));
    let sent = transport.sent_log();
    let mut connection = CdpConnection::new(transport.boxed());

    let blocked = wait_actionable(
        &mut connection,
        &session(),
        &token(3),
        LocateIntent::Click,
        deadline(),
    )
    .unwrap();

    assert_eq!(blocked, None);
    let sent = sent.lock().unwrap();
    assert_eq!(calls(&sent, "Runtime.callFunctionOn"), 3);
    assert_eq!(calls(&sent, "DOM.resolveNode"), 1);
}

#[test]
fn an_element_still_blocked_when_the_wait_ends_is_reported_not_acted_on() {
    let transport = released(checked(
        resolved(ScriptedTransport::new()),
        json!({ "blocker": "hidden", "detail": "display:none" }),
    ));
    let mut connection = CdpConnection::new(transport.boxed());

    let blocked = wait_actionable(
        &mut connection,
        &session(),
        &token(3),
        LocateIntent::Hover,
        Deadline::after(60).unwrap(),
    )
    .unwrap()
    .unwrap();

    assert_eq!(blocked.blocker, Blocker::Hidden);
    assert_eq!(blocked.detail, "display:none");
}

#[test]
fn a_fill_asks_for_an_editable_field() {
    let transport = released(checked(
        resolved(ScriptedTransport::new()),
        json!({ "blocker": "readonly", "detail": "read-only" }),
    ));
    let sent = transport.sent_log();
    let mut connection = CdpConnection::new(transport.boxed());

    let blocked = wait_actionable(
        &mut connection,
        &session(),
        &token(3),
        LocateIntent::Fill,
        Deadline::after(60).unwrap(),
    )
    .unwrap()
    .unwrap();

    assert_eq!(blocked.blocker, Blocker::ReadOnly);
    let sent = sent.lock().unwrap();
    assert_eq!(sent[2]["params"]["arguments"][0]["value"]["editable"], true);
}

#[test]
fn a_detached_element_is_a_stale_ref_at_once() {
    let transport = released(checked(
        resolved(ScriptedTransport::new()),
        json!({ "blocker": "detached" }),
    ));
    let mut connection = CdpConnection::new(transport.boxed());

    let error = wait_actionable(
        &mut connection,
        &session(),
        &token(3),
        LocateIntent::Click,
        deadline(),
    )
    .unwrap_err();

    assert_eq!(error.code, ErrorCode::StaleRef);
}
