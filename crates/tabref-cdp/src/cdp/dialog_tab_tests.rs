use serde_json::json;
use tabref_core::Deadline;

use crate::{CdpConnection, ScriptedTransport};

fn deadline() -> Deadline {
    Deadline::after(1_000).unwrap()
}

#[test]
fn a_dialog_names_its_tab_also_when_an_iframe_of_that_tab_opened_it() {
    let transport = ScriptedTransport::new()
        .expect_with_events(
            "Input.dispatchMouseEvent",
            vec![
                json!({ "method": "Target.attachedToTarget", "sessionId": "S",
                    "params": { "sessionId": "C", "targetInfo": { "targetId": "FRAME", "type": "iframe" } } }),
                json!({ "method": "Page.javascriptDialogOpening", "sessionId": "C",
                    "params": { "type": "alert", "message": "From the frame" } }),
                json!({ "method": "Page.javascriptDialogOpening", "sessionId": "S",
                    "params": { "type": "alert", "message": "From the page" } }),
                json!({ "method": "Page.javascriptDialogOpening", "sessionId": "ELSEWHERE",
                    "params": { "type": "alert", "message": "Unknown" } }),
            ],
            json!({ "result": {} }),
        )
        .expect_silence("Page.handleJavaScriptDialog")
        .expect_silence("Page.handleJavaScriptDialog")
        .expect_silence("Page.handleJavaScriptDialog");
    let mut connection = CdpConnection::new(transport.boxed());
    connection.note_session_tab("S", "TAB1");

    connection
        .call("Input.dispatchMouseEvent", json!({}), Some("S"), deadline())
        .unwrap();

    let tabs: Vec<serde_json::Value> = connection
        .drain_events(|event| event.method == "Page.javascriptDialogOpening")
        .into_iter()
        .map(|event| event.params["tab"].clone())
        .collect();
    assert_eq!(
        tabs,
        [json!("TAB1"), json!("TAB1"), serde_json::Value::Null]
    );
}

#[test]
fn the_dialogs_buffered_so_far_are_marked_as_late() {
    let transport = ScriptedTransport::new()
        .expect_with_events(
            "Runtime.evaluate",
            vec![
                json!({ "method": "Page.javascriptDialogOpening", "sessionId": "S",
                "params": { "type": "confirm", "message": "Later?" } }),
            ],
            json!({ "result": {} }),
        )
        .expect_silence("Page.handleJavaScriptDialog");
    let mut connection = CdpConnection::new(transport.boxed());
    connection
        .call("Runtime.evaluate", json!({}), Some("S"), deadline())
        .unwrap();

    connection.mark_buffered_dialogs_late();

    let dialogs = connection.drain_events(|event| event.method == "Page.javascriptDialogOpening");
    assert_eq!(dialogs.len(), 1);
    assert_eq!(dialogs[0].params["late"], true);
    assert_eq!(dialogs[0].params["answer"]["accept"], true);
}
