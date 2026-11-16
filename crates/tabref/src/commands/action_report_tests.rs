use tabref_page::TextChange;

use super::distinct_changes;

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
