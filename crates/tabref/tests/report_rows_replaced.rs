#![allow(dead_code, clippy::unwrap_used, clippy::expect_used, clippy::panic)]

#[path = "../src/view/rows_replaced.rs"]
mod rows_replaced;

use tabref_page::{PageChanges, TextChange};

#[test]
fn previews_are_five_rows_and_other_changes_are_not_removed() {
    let rows: Vec<_> = (0..8).map(|i| format!("New {i}")).collect();
    let mut changes = vec![TextChange::Added("Saved".into())];
    changes.extend(rows.iter().cloned().map(TextChange::Added));
    let page = PageChanges {
        changes,
        replaced_rows: vec![(10, rows)],
        ..Default::default()
    };
    assert_eq!(
        rows_replaced::other_changes(&page),
        [TextChange::Added("Saved".into())]
    );
    let preview = rows_replaced::preview(&page);
    assert_eq!(preview.len(), 7);
    assert_eq!(preview[0], "- 10 rows replaced, 8 new");
    assert_eq!(preview[5], "  New 4");
    assert_eq!(preview[6], "  (+3 more: tabref snap)");
    assert_eq!(rows_replaced::omitted(&page, page.changes[..4].iter()), 5);
    assert_eq!(rows_replaced::changes(&page).len(), 10);
}

#[test]
fn empty_reports_and_duplicate_rows_have_exact_omission_counts() {
    let page = PageChanges::default();
    assert!(rows_replaced::preview(&page).is_empty());
    assert_eq!(rows_replaced::omitted(&page, std::iter::empty()), 0);
    let page = PageChanges {
        replaced_rows: vec![(3, vec!["Shared".into(), "Shared".into(), "Other".into()])],
        ..Default::default()
    };
    let shown = [TextChange::Added("Shared".into())];
    assert_eq!(rows_replaced::omitted(&page, shown.iter()), 1);
    assert_eq!(rows_replaced::omitted(&page, std::iter::empty()), 3);
}
