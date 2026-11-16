use serde_json::json;
use tabref_core::Deadline;

use super::{attach_to_target, attach_to_target_with, create_background_tab, list_page_targets};
use crate::CdpConnection;
use crate::ScriptedTransport;

fn deadline() -> Deadline {
    Deadline::after(1_000).unwrap()
}

#[test]
fn only_page_targets_are_listed() {
    let transport = ScriptedTransport::new().expect(
        "Target.getTargets",
        json!({ "targetInfos": [
            { "targetId": "P1", "type": "page", "url": "https://a.test/", "title": "A", "attached": false },
            { "targetId": "W1", "type": "service_worker", "url": "https://a.test/sw.js", "title": "" },
            { "targetId": "P2", "type": "page", "url": "about:blank", "title": "", "attached": true, "browserContextId": "C" },
        ] }),
    );
    let mut connection = CdpConnection::new(transport.boxed());

    let pages = list_page_targets(&mut connection, deadline()).unwrap();

    assert_eq!(pages.len(), 2);
    assert_eq!(pages[0].target_id, "P1");
    assert_eq!(pages[0].title, "A");
    assert!(pages[1].attached);
    assert_eq!(pages[1].browser_context_id.as_deref(), Some("C"));
}

#[test]
fn attach_flattens_and_releases_a_paused_target() {
    let transport = ScriptedTransport::new()
        .expect("Target.attachToTarget", json!({ "sessionId": "S9" }))
        .expect("Runtime.runIfWaitingForDebugger", json!({}));
    let sent = transport.sent_log();
    let mut connection = CdpConnection::new(transport.boxed());

    let session = attach_to_target(&mut connection, "P1", deadline()).unwrap();

    assert_eq!(session.session_id, "S9");
    assert_eq!(session.target_id, "P1");
    let sent = sent.lock().unwrap();
    assert_eq!(
        sent[0]["params"],
        json!({ "targetId": "P1", "flatten": true })
    );
    assert_eq!(sent[1]["sessionId"], "S9");
}

#[test]
fn follow_ups_go_out_with_the_release_before_any_reply_is_awaited() {
    let transport = ScriptedTransport::new()
        .expect("Target.attachToTarget", json!({ "sessionId": "S9" }))
        .expect_silence("Runtime.runIfWaitingForDebugger")
        .expect("Page.enable", json!({}))
        .expect("Page.setLifecycleEventsEnabled", json!({}));
    let mut connection = CdpConnection::new(transport.boxed());

    let follow_ups = [
        ("Page.enable", json!({})),
        ("Page.setLifecycleEventsEnabled", json!({ "enabled": true })),
    ];
    let error = attach_to_target_with(
        &mut connection,
        "P1",
        &follow_ups,
        Deadline::after(50).unwrap(),
    )
    .unwrap_err();

    assert_eq!(error.code, tabref_core::ErrorCode::Timeout);
}

#[test]
fn follow_ups_answer_in_order_and_are_remembered() {
    let transport = ScriptedTransport::new()
        .expect("Target.attachToTarget", json!({ "sessionId": "S9" }))
        .expect("Runtime.runIfWaitingForDebugger", json!({}))
        .expect("Page.enable", json!({}));
    let sent = transport.sent_log();
    let mut connection = CdpConnection::new(transport.boxed());

    let session = attach_to_target_with(
        &mut connection,
        "P1",
        &[("Page.enable", json!({}))],
        deadline(),
    )
    .unwrap();
    connection
        .call(
            "Page.enable",
            json!({}),
            Some(&session.session_id),
            deadline(),
        )
        .unwrap();

    let sent = sent.lock().unwrap();
    assert_eq!(sent.len(), 3);
    assert_eq!(sent[2]["sessionId"], "S9");
}

#[test]
fn new_tabs_open_in_the_background() {
    let transport =
        ScriptedTransport::new().expect("Target.createTarget", json!({ "targetId": "T7" }));
    let sent = transport.sent_log();
    let mut connection = CdpConnection::new(transport.boxed());

    let target = create_background_tab(&mut connection, "about:blank", deadline()).unwrap();

    assert_eq!(target, "T7");
    assert_eq!(sent.lock().unwrap()[0]["params"]["background"], true);
}

#[test]
fn a_tab_that_never_answers_its_attach_is_said_to_be_maybe_blocked_by_a_dialog() {
    let transport = ScriptedTransport::new()
        .expect("Target.attachToTarget", json!({ "sessionId": "S9" }))
        .expect_silence("Runtime.runIfWaitingForDebugger")
        .expect_silence("Page.enable");
    let mut connection = CdpConnection::new(transport.boxed());

    let error = attach_to_target_with(
        &mut connection,
        "P1",
        &[("Page.enable", json!({}))],
        Deadline::after(50).unwrap(),
    )
    .unwrap_err();

    assert_eq!(error.code, tabref_core::ErrorCode::Timeout);
    let hint = error.suggestion.unwrap_or_default();
    assert!(
        hint.contains("blocked by a dialog that opened after the last command"),
        "{hint}"
    );
}
