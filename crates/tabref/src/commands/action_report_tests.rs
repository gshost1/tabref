use serde_json::json;
use tabref_page::TextChange;

use super::{dialog_line, dialog_lines, distinct_changes, step_change, within_budget};

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

#[test]
fn a_line_the_page_shows_twice_is_reported_once() {
    let changes = vec![
        TextChange::Added("Please fix the following:".into()),
        TextChange::Added("Phone must be 555-123-4567.".into()),
        TextChange::Added("Phone must be 555-123-4567.".into()),
        TextChange::Removed("Draft".into()),
        TextChange::Added("Draft".into()),
    ];
    assert_eq!(
        distinct_changes(&changes),
        vec![
            "+ Please fix the following:",
            "+ Phone must be 555-123-4567. ×2",
            "- Draft",
            "+ Draft",
        ]
    );
}

#[test]
fn a_repeated_line_says_how_many_times_it_showed() {
    let changes = vec![
        TextChange::Added("Required".into()),
        TextChange::Added("Required".into()),
        TextChange::Added("Saved".into()),
    ];
    assert_eq!(distinct_changes(&changes), vec!["+ Required ×2", "+ Saved"]);
}

#[test]
fn lines_alike_only_in_their_shown_part_stay_apart() {
    let head = "word ".repeat(100);
    let changes = vec![
        TextChange::Added(format!("{head}first")),
        TextChange::Added(format!("{head}second")),
    ];
    let lines = distinct_changes(&changes);
    assert_eq!(lines.len(), 2, "{lines:?}");
    assert!(lines.iter().all(|line| !line.contains('×')), "{lines:?}");
}

#[test]
fn a_step_line_cuts_its_change_at_100_characters() {
    let changes = vec![
        TextChange::Added("x ".repeat(200)),
        TextChange::Added("Saved".into()),
    ];
    let line = step_change(&changes).unwrap();
    assert!(line.chars().count() <= 120, "{line}");
    assert!(line.ends_with(" (+1 more)"), "{line}");
}

#[test]
fn a_change_that_does_not_fit_is_cut_to_the_room_left() {
    let lines: Vec<String> = (0..50)
        .map(|n| format!("+ {n} {}", "y".repeat(297)))
        .collect();
    let shown = within_budget(&lines);
    assert_eq!(shown.len(), 3, "{shown:?}");
    assert_eq!(shown[0], lines[0]);
    assert!(shown[1].ends_with('…'), "{}", shown[1]);
    assert_eq!(shown[2], "+48 more: tabref snap");
    let used: usize = shown[..2].iter().map(String::len).sum();
    assert!(used <= 600, "{used}");
}

#[test]
fn the_first_change_keeps_to_the_budget_too() {
    let wide = format!("+ {}", "字".repeat(400));
    let shown = within_budget(std::slice::from_ref(&wide));
    assert_eq!(shown.len(), 1, "{shown:?}");
    assert!(shown[0].len() <= 600, "{}", shown[0].len());
    assert!(shown[0].ends_with('…'), "{}", shown[0]);
}
