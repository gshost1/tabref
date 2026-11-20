use serde_json::json;
use tabref_page::TextChange;

use std::time::{Duration, Instant};

use super::super::action_report_changes::{
    change_data, counted, reload_head, step_change, within_budget,
};
use super::{NOTHING_READ, NOTHING_SEEN, dialog_line, dialog_lines, nothing_seen};

#[test]
fn a_quiet_action_selects_the_unread_frame_note() {
    let acted = super::Acted {
        verb: "click",
        summary: String::new(),
        data: json!({}),
        effects: Some(super::ActionEffects {
            page: Some(tabref_page::PageChanges {
                mutations: Some(0),
                unread_frames: 1,
                ..Default::default()
            }),
            ..Default::default()
        }),
        known: Default::default(),
    };
    assert_eq!(nothing_seen(&acted), Some(NOTHING_READ));
    let mut acted = acted;
    if let Some(page) = acted
        .effects
        .as_mut()
        .and_then(|effects| effects.page.as_mut())
    {
        page.unread_frames = 0;
    }
    assert_eq!(nothing_seen(&acted), Some(NOTHING_SEEN));
}

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
        within_budget(&counted(&changes)),
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
    let counts = counted(&changes);
    assert_eq!(within_budget(&counts), vec!["+ Required ×2", "+ Saved"]);
    assert_eq!(
        change_data(&counts),
        json!({
            "changes": ["+ Required ×2", "+ Saved"],
            "change_counts": [
                { "line": "+ Required", "count": 2 },
                { "line": "+ Saved", "count": 1 }
            ],
            "distinct_changes_omitted": 0
        })
    );
}

#[test]
fn the_reload_preview_reports_only_counted_additions() {
    let changes = [
        TextChange::Removed("Old draft".into()),
        TextChange::Added("Required".into()),
        TextChange::Added("Required".into()),
        TextChange::Added("Try again".into()),
    ];
    assert_eq!(
        reload_head(&counted(&changes)),
        ["+ Required ×2", "+ Try again"]
    );
}

#[test]
fn lines_alike_only_in_their_shown_part_stay_apart() {
    let head = "word ".repeat(100);
    let changes = vec![
        TextChange::Added(format!("{head}first")),
        TextChange::Added(format!("{head}second")),
    ];
    let counted = counted(&changes);
    assert_eq!(counted.len(), 2);
    assert!(counted.iter().all(|line| line.count == 1));
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
    let lines: Vec<_> = (0..50)
        .map(|n| TextChange::Added(format!("{n} {}", "y".repeat(297))))
        .collect();
    let shown = within_budget(&counted(&lines));
    assert_eq!(shown.len(), 3, "{shown:?}");
    assert_eq!(shown[0], lines[0].to_string());
    assert!(shown[1].ends_with('…'), "{}", shown[1]);
    assert_eq!(shown[2], "+48 more: tabref snap");
    let used: usize = shown[..2].iter().map(String::len).sum();
    assert!(used <= 600, "{used}");
}

#[test]
fn the_first_change_keeps_to_the_budget_too() {
    let wide = TextChange::Added("字".repeat(400));
    let shown = within_budget(&counted(&[wide.clone(), wide]));
    assert_eq!(shown.len(), 1, "{shown:?}");
    assert!(shown[0].len() <= 600, "{}", shown[0].len());
    assert!(shown[0].ends_with("… ×2"), "{}", shown[0]);
}

#[test]
fn a_rewritten_line_that_does_not_fit_keeps_its_marker_intact() {
    let added = TextChange::Added("x".repeat(399));
    let rewrite = TextChange::Changed {
        before: "字".repeat(50),
        after: "語".repeat(50),
    };
    let changes = [added.clone(), rewrite.clone()];
    assert_eq!(
        within_budget(&counted(&changes)),
        vec![added.to_string(), "+1 more: tabref snap".to_owned()]
    );
    assert_eq!(
        within_budget(&counted(std::slice::from_ref(&rewrite))),
        vec![rewrite.to_string()]
    );
}

#[test]
fn a_long_first_rewrite_shortens_its_context_and_keeps_its_count() {
    let head = "𠮷".repeat(40);
    let tail = "𩸽".repeat(40);
    let before = "🟠".repeat(60);
    let after = "🟣".repeat(60);
    let rewrite = TextChange::Changed {
        before: format!("{head} {before} {tail}"),
        after: format!("{head} {after} {tail}"),
    };
    assert!(rewrite.to_string().len() > 600);
    let shown = within_budget(&counted(&[rewrite.clone(), rewrite]));
    assert_eq!(shown.len(), 1, "{shown:?}");
    assert!(shown[0].len() <= 600, "{}", shown[0]);
    assert!(shown[0].contains(&format!("{{{before} -> {after}}}")));
    assert!(shown[0].ends_with("… ×2"), "{}", shown[0]);
}

#[test]
fn thousands_of_changes_keep_first_seen_order_under_a_time_bound() {
    let mut changes: Vec<_> = (0..12_000)
        .map(|index| TextChange::Added(format!("Item number {index:05}")))
        .collect();
    changes.push(changes[34].clone());
    let started = Instant::now();
    let counts = counted(&changes);
    assert!(
        started.elapsed() < Duration::from_millis(500),
        "{:?}",
        started.elapsed()
    );
    assert_eq!(counts.len(), 12_000);
    assert_eq!(counts[34].change, &changes[34]);
    assert_eq!(counts[34].count, 2);
    assert_eq!(counts[11_999].change, &changes[11_999]);
    let json = change_data(&counts);
    assert_eq!(json["changes"].as_array().unwrap().len(), 50);
    assert_eq!(json["change_counts"].as_array().unwrap().len(), 50);
    assert_eq!(json["distinct_changes_omitted"], 11_950);
    assert_eq!(json["changes"][34], "+ Item number 00034 ×2");
    assert_eq!(
        json["change_counts"][34],
        json!({ "line": "+ Item number 00034", "count": 2 })
    );
}
