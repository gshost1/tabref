use serde_json::json;

use super::dialog_line;

#[test]
fn a_dialog_line_says_which_dialog_opened_and_how_it_was_answered() {
    let confirm = json!({ "type": "confirm", "message": "Sure?", "answer": { "accept": false } });
    assert_eq!(dialog_line(&confirm), "dialog confirm \"Sure?\" dismissed");
    let prompt = json!({ "type": "prompt", "message": "Name?",
        "answer": { "accept": true, "text": "Bob" } });
    assert_eq!(
        dialog_line(&prompt),
        "dialog prompt \"Name?\" answered \"Bob\""
    );
    let alert = json!({ "type": "alert", "message": "Hi", "answer": { "accept": true } });
    assert_eq!(dialog_line(&alert), "dialog alert \"Hi\" accepted");
    let leave = json!({ "type": "beforeunload", "message": "", "answer": { "accept": true } });
    assert_eq!(dialog_line(&leave), "dialog beforeunload accepted");
    let unanswered = json!({ "type": "alert", "message": "Hi" });
    assert_eq!(dialog_line(&unanswered), "dialog alert \"Hi\" accepted");
}
