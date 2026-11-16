use serde_json::json;

use super::{dialog_line, dialog_lines};

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

#[test]
fn a_dialog_says_when_it_opened_after_the_last_command_or_in_another_tab() {
    let late = json!({ "type": "confirm", "message": "Later?", "late": true,
        "answer": { "accept": true } });
    assert_eq!(
        dialog_line(&late),
        "dialog confirm \"Later?\" accepted (opened after the last command)"
    );
    let elsewhere = json!({ "type": "alert", "message": "Hi", "other_tab": true,
        "answer": { "accept": true } });
    assert_eq!(
        dialog_line(&elsewhere),
        "dialog alert \"Hi\" accepted (in another tab)"
    );
}

#[test]
fn repeated_dialogs_collapse_and_the_list_is_capped() {
    let alert = |message: &str| {
        json!({ "type": "alert", "message": message,
        "answer": { "accept": true } })
    };
    let mut dialogs = vec![alert("Again"), alert("Again"), alert("Again")];
    for index in 0..12 {
        dialogs.push(alert(&format!("Other {index}")));
    }
    let lines = dialog_lines(&dialogs);
    assert_eq!(lines[0], "dialog alert \"Again\" accepted \u{d7}3");
    assert_eq!(lines.len(), 11, "{lines:?}");
    assert_eq!(lines[9], "dialog alert \"Other 8\" accepted");
    assert_eq!(lines[10], "3 more dialogs");
    assert!(dialog_lines(&[]).is_empty());
}

#[test]
fn only_repeats_next_to_each_other_collapse_and_one_more_is_singular() {
    let alert = |message: &str| json!({ "type": "alert", "message": message, "answer": { "accept": true } });
    let lines = dialog_lines(&[alert("A"), alert("B"), alert("A"), alert("A")]);
    assert_eq!(
        lines,
        [
            "dialog alert \"A\" accepted",
            "dialog alert \"B\" accepted",
            "dialog alert \"A\" accepted \u{d7}2",
        ]
    );
    let many: Vec<_> = (0..11).map(|index| alert(&format!("N{index}"))).collect();
    assert_eq!(dialog_lines(&many).last().unwrap(), "1 more dialog");
}
