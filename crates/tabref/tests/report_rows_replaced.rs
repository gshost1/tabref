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
        replaced_rows: vec![(
            10,
            rows.into_iter()
                .enumerate()
                .map(|(id, text)| (id as u64, text))
                .collect(),
        )],
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
    assert_eq!(rows_replaced::data(&page).unwrap()["omitted"], 0);
    assert_eq!(rows_replaced::changes(&page).len(), 10);
}

#[test]
fn empty_reports_and_duplicate_rows_have_exact_omission_counts() {
    let page = PageChanges::default();
    assert!(rows_replaced::preview(&page).is_empty());
    assert!(rows_replaced::data(&page).is_none());
    let page = PageChanges {
        replaced_rows: vec![(
            3,
            vec![
                (1, "Shared".into()),
                (2, "Shared".into()),
                (3, "Other".into()),
            ],
        )],
        ..Default::default()
    };
    let data = rows_replaced::data(&page).unwrap();
    assert_eq!(data["omitted"], 0);
    assert_eq!(data["preview"][0]["id"], 1);
    assert_eq!(data["preview"][1]["id"], 2);
}

#[test]
fn toast_text_cannot_change_row_identity_or_omission_counts() {
    let rows: Vec<_> = (0..51).map(|id| (id, "Saved".to_owned())).collect();
    let mut changes = vec![TextChange::Added("Saved".into())];
    changes.extend(rows.iter().map(|(_, text)| TextChange::Added(text.clone())));
    let page = PageChanges {
        changes,
        replaced_rows: vec![(51, rows)],
        ..Default::default()
    };
    let data = rows_replaced::data(&page).unwrap();
    assert_eq!(data["preview"].as_array().unwrap().len(), 50);
    assert_eq!(data["preview"][49]["id"], 49);
    assert_eq!(data["omitted"], 1);
    assert_eq!(
        rows_replaced::other_changes(&page),
        [TextChange::Added("Saved".into())]
    );
}
