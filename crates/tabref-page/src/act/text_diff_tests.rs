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
