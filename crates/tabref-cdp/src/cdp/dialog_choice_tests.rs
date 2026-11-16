use serde_json::{Value, json};
use tabref_core::Deadline;

use super::DialogChoice;
use crate::{CdpConnection, ScriptedTransport};

fn deadline() -> Deadline {
    Deadline::after(1_000).unwrap()
}

fn opening(kind: &str, message: &str) -> Value {
    json!({ "method": "Page.javascriptDialogOpening",
        "params": { "type": kind, "message": message, "defaultPrompt": "def" },
        "sessionId": "S" })
}

/// A click during which `dialogs` open, each answered by the connection.
fn clicked_through(dialogs: Vec<Value>, choice: Option<DialogChoice>) -> (Vec<Value>, Vec<Value>) {
    let count = dialogs.len();
    let mut transport = ScriptedTransport::new().expect_with_events(
        "Input.dispatchMouseEvent",
        dialogs,
        json!({ "result": {} }),
    );
    for _ in 0..count {
        transport = transport.expect_silence("Page.handleJavaScriptDialog");
    }
    let sent = transport.sent_log();
    let mut connection = CdpConnection::new(transport.boxed());
    connection.answer_next_dialog(choice);
    connection
        .call("Input.dispatchMouseEvent", json!({}), Some("S"), deadline())
        .unwrap();
    let answers: Vec<Value> = sent
        .lock()
        .unwrap()
        .iter()
        .filter(|request| request["method"] == "Page.handleJavaScriptDialog")
        .map(|request| request["params"].clone())
        .collect();
    let reported: Vec<Value> = connection
        .drain_events(|event| event.method == "Page.javascriptDialogOpening")
        .into_iter()
        .map(|event| event.params["answer"].clone())
        .collect();
    (answers, reported)
}

#[test]
fn the_chosen_answer_takes_the_next_dialog_and_the_one_after_is_accepted() {
    let (answers, reported) = clicked_through(
        vec![opening("confirm", "Sure?"), opening("confirm", "Really?")],
        Some(DialogChoice::Dismiss),
    );
    assert_eq!(answers[0]["accept"], false);
    assert_eq!(answers[1]["accept"], true);
    assert_eq!(reported[0], json!({ "accept": false }));
    assert_eq!(reported[1], json!({ "accept": true }));
}

#[test]
fn a_prompt_is_answered_with_the_chosen_text() {
    let (answers, reported) = clicked_through(
        vec![opening("prompt", "Name?")],
        Some(DialogChoice::Accept {
            text: Some("Bob".to_owned()),
        }),
    );
    assert_eq!(answers[0], json!({ "accept": true, "promptText": "Bob" }));
    assert_eq!(reported[0], json!({ "accept": true, "text": "Bob" }));
}

#[test]
fn without_a_choice_a_dialog_is_accepted_with_its_default_text() {
    let (answers, reported) = clicked_through(vec![opening("prompt", "Name?")], None);
    assert_eq!(answers[0], json!({ "accept": true, "promptText": "def" }));
    assert_eq!(reported[0], json!({ "accept": true, "text": "def" }));
}

#[test]
fn a_choice_no_dialog_used_is_handed_back() {
    let transport = ScriptedTransport::new();
    let mut connection = CdpConnection::new(transport.boxed());
    connection.answer_next_dialog(Some(DialogChoice::Dismiss));
    assert_eq!(connection.take_dialog_choice(), Some(DialogChoice::Dismiss));
    assert_eq!(connection.take_dialog_choice(), None);
}

#[test]
fn a_choice_is_read_from_its_flag_value() {
    assert_eq!(DialogChoice::parse("dismiss"), Some(DialogChoice::Dismiss));
    assert_eq!(
        DialogChoice::parse("accept"),
        Some(DialogChoice::Accept { text: None })
    );
    assert_eq!(
        DialogChoice::parse("accept=Bob Smith"),
        Some(DialogChoice::Accept {
            text: Some("Bob Smith".to_owned())
        })
    );
    assert_eq!(
        DialogChoice::parse("accept="),
        Some(DialogChoice::Accept {
            text: Some(String::new())
        })
    );
    assert_eq!(DialogChoice::parse("maybe"), None);
    assert_eq!(DialogChoice::parse("dismiss=x"), None);
}
