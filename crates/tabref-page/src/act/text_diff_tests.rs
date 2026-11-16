use super::{diff_lines, text_lines};
use crate::act::text_change::TextChange;

fn lines(text: &str) -> Vec<String> {
    text_lines(text)
}

#[test]
fn rendered_text_becomes_trimmed_non_empty_lines() {
    assert_eq!(
        text_lines("  ▲ 3 ▼\tBooks  \n\n Next "),
        vec!["▲ 3 ▼ Books", "Next"]
    );
}

#[test]
fn a_counter_that_moved_is_one_rewritten_line() {
    let before = lines("Hot\n▲ 3 ▼ Books set in winter\nSubmitted 5 hours ago");
    let after = lines("Hot\n▲ 4 ▼ Books set in winter\nSubmitted 5 hours ago");
    assert_eq!(
        diff_lines(&before, &after),
        vec![TextChange::Changed {
            before: "▲ 3 ▼ Books set in winter".into(),
            after: "▲ 4 ▼ Books set in winter".into(),
        }]
    );
}

#[test]
fn new_and_removed_lines_are_reported_in_page_order() {
    let before = lines("A\nB\nC\nD");
    let after = lines("A\nC\nNew one\nD");
    assert_eq!(
        diff_lines(&before, &after),
        vec![
            TextChange::Removed("B".into()),
            TextChange::Added("New one".into()),
        ]
    );
}

#[test]
fn unchanged_text_has_no_changes() {
    let page = lines("A\nB");
    assert!(diff_lines(&page, &page).is_empty());
    assert!(diff_lines(&[], &[]).is_empty());
}

#[test]
fn a_line_added_between_repeated_lines_is_found() {
    let before = lines("Reply\nReply\nReply");
    let after = lines("Reply\nReply\nPost\nReply");
    assert_eq!(
        diff_lines(&before, &after),
        vec![TextChange::Added("Post".into())]
    );
}

fn rows(names: &[&str]) -> Vec<String> {
    let mut all = vec!["Shipments".to_owned(), "Tracking Weight".to_owned()];
    all.extend(names.iter().map(|name| format!("{name} kg")));
    all.push("Page 1 of 1".to_owned());
    all
}

#[test]
fn rows_that_only_moved_are_one_reorder_with_the_new_first_rows() {
    let before = rows(&["A 1", "B 9", "C 4", "D 7", "E 2", "F 5", "G 3"]);
    let after = rows(&["B 9", "D 7", "F 5", "C 4", "G 3", "E 2", "A 1"]);
    assert_eq!(
        diff_lines(&before, &after),
        vec![TextChange::Reordered {
            count: 7,
            first: [
                "B 9 kg", "D 7 kg", "F 5 kg", "C 4 kg", "G 3 kg", "E 2 kg", "A 1 kg"
            ]
            .map(String::from)
            .to_vec(),
        }]
    );
}

#[test]
fn a_reorder_that_also_rewrites_a_header_line_is_still_a_reorder() {
    let mut before = rows(&["A 1", "B 9", "C 4", "D 7", "E 2", "F 5"]);
    let mut after = rows(&["B 9", "D 7", "F 5", "C 4", "E 2", "A 1"]);
    before[1] = "Tracking Weight".into();
    after[1] = "Tracking Weight ▼".into();
    let changes = diff_lines(&before, &after);
    assert!(
        matches!(changes.as_slice(), [TextChange::Reordered { count: 7, first }] if first[0] == "Tracking Weight ▼"),
        "{changes:?}"
    );
}

#[test]
fn rows_removed_in_order_stay_removed_lines() {
    let before = rows(&["A 1", "B 9", "C 4", "D 7"]);
    let after = rows(&["A 1", "C 4", "D 7"]);
    assert_eq!(
        diff_lines(&before, &after),
        vec![TextChange::Removed("B 9 kg".into())]
    );
}

#[test]
fn two_lines_that_swap_stay_a_plain_diff() {
    let before = lines("A\nB\nC\nD");
    let after = lines("A\nC\nB\nD");
    assert!(
        !diff_lines(&before, &after)
            .iter()
            .any(|change| matches!(change, TextChange::Reordered { .. })),
    );
}
