use tabref_page::TextChange;

use super::{distinct_changes, step_change, within_budget};

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
            "+ Phone must be 555-123-4567.",
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
        .map(|n| format!("+ {n} {}", "y".repeat(296)))
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
