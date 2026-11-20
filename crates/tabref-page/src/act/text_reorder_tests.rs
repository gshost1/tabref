#[test]
fn a_middle_exceeding_the_diff_matrix_limit_uses_ordinary_changes() {
    let mut before = vec!["Header".into()];
    before.extend((0..1001).map(|n| format!("Parcel {n}")));
    before.push("Footer".into());
    let mut after = before.clone();
    after[1..1002].reverse();
    let changes = diff_page(&before, &after, &Cells::new());
    assert!(changes.len() > 1000);
    assert!(
        changes
            .iter()
            .all(|change| !matches!(change, TextChange::Reordered { .. }))
    );
}

use super::{Cells, cells_of};
use crate::act::text_change::TextChange;
use crate::act::text_diff::{diff_lines, diff_page, text_lines};

fn rows(names: &[&str]) -> Vec<String> {
    let mut all = vec!["Shipments".to_owned(), "Tracking Weight".to_owned()];
    all.extend(names.iter().map(|name| format!("{name} kg")));
    all.push("Page 1 of 1".to_owned());
    all
}

fn kg(names: &[&str]) -> Vec<String> {
    names.iter().map(|name| format!("{name} kg")).collect()
}

fn reorder(above: &str, run: Vec<String>) -> TextChange {
    TextChange::Reordered {
        above: Some(above.to_owned()),
        run,
        table: false,
    }
}

#[test]
fn rows_that_only_moved_are_one_reorder_in_their_new_order() {
    let before = rows(&["A 1", "B 9", "C 4", "D 7", "E 2", "F 5", "G 3"]);
    let after = rows(&["B 9", "D 7", "F 5", "C 4", "G 3", "E 2", "A 1"]);
    assert_eq!(
        diff_lines(&before, &after),
        vec![reorder(
            "Tracking Weight",
            kg(&["B 9", "D 7", "F 5", "C 4", "G 3", "E 2", "A 1"])
        )]
    );
}

#[test]
fn a_first_row_that_kept_its_place_still_starts_the_reorder() {
    let before = rows(&["A 9", "B 1", "C 4", "D 7", "E 2", "F 5"]);
    let after = rows(&["A 9", "D 7", "F 5", "C 4", "E 2", "B 1"]);
    let changes = diff_lines(&before, &after);
    assert_eq!(
        changes,
        vec![reorder(
            "Tracking Weight",
            kg(&["A 9", "D 7", "F 5", "C 4", "E 2", "B 1"])
        )]
    );
    assert!(
        changes[0]
            .to_string()
            .contains("below \"Tracking Weight\", now starting:\n  A 9 kg"),
        "{}",
        changes[0]
    );
}

#[test]
fn a_reorder_that_also_rewrites_a_header_line_prints_the_rewrite_too() {
    let mut before = rows(&["A 1", "B 9", "C 4", "D 7", "E 2", "F 5"]);
    let mut after = rows(&["B 9", "D 7", "F 5", "C 4", "E 2", "A 1"]);
    before[1] = "Tracking Weight".into();
    after[1] = "Tracking Weight ▼".into();
    let changes = diff_lines(&before, &after);
    assert!(
        matches!(changes.as_slice(), [TextChange::Reordered { run, .. }, TextChange::Changed { after, .. }]
            if run[0] == "B 9 kg" && run.len() == 6 && after == "Tracking Weight ▼"),
        "{changes:?}"
    );
}

#[test]
fn a_cell_that_changed_during_a_sort_is_printed_after_the_reorder() {
    let before = rows(&["A 1", "B 9", "C 4", "D 7", "E 2", "F 5", "G 3"]);
    let after = rows(&["B 9", "D 7", "F 5", "C 4", "G 3", "E 2", "A 8"]);
    let changes = diff_lines(&before, &after);
    assert!(
        matches!(&changes[..], [TextChange::Reordered { .. }, TextChange::Changed { before, after }]
            if before == "A 1 kg" && after == "A 8 kg"),
        "{changes:?}"
    );
}

#[test]
fn a_flash_message_that_came_with_a_sort_is_printed_after_the_reorder() {
    let before = rows(&[
        "A 1", "B 9", "C 4", "D 7", "E 2", "F 5", "G 3", "H 6", "J 8",
    ]);
    let mut after = rows(&[
        "B 9", "J 8", "D 7", "H 6", "F 5", "C 4", "G 3", "E 2", "A 1",
    ]);
    after.insert(2, "Sorted by weight".into());
    let changes = diff_lines(&before, &after);
    assert!(
        matches!(&changes[..], [TextChange::Reordered { .. }, TextChange::Added(line)]
            if line == "Sorted by weight"),
        "{changes:?}"
    );
}

#[test]
fn a_new_page_of_cards_with_repeated_lines_is_a_plain_change() {
    let card = |name: &str, price: &str| {
        vec![
            name.to_owned(),
            price.to_owned(),
            "Add to cart".to_owned(),
            "In stock".to_owned(),
        ]
    };
    let page = |cards: &[(&str, &str)]| -> Vec<String> {
        let mut all = vec!["Shop".to_owned()];
        for (name, price) in cards {
            all.extend(card(name, price));
        }
        all.push("Next".to_owned());
        all
    };
    let one = page(&[
        ("Cap", "$12"),
        ("Mug", "$9"),
        ("Tee", "$20"),
        ("Bag", "$30"),
    ]);
    let two = page(&[("Pen", "$2"), ("Hat", "$15"), ("Cup", "$7"), ("Box", "$5")]);
    let changes = diff_lines(&one, &two);
    assert!(
        !changes
            .iter()
            .any(|change| matches!(change, TextChange::Reordered { .. })),
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
    let before = text_lines("A\nB\nC\nD");
    let after = text_lines("A\nC\nB\nD");
    assert!(
        !diff_lines(&before, &after)
            .iter()
            .any(|change| matches!(change, TextChange::Reordered { .. })),
    );
}

#[test]
fn a_table_drawn_again_is_rows_and_its_header_row_is_not_one() {
    let table = |order: &[(&str, &str)]| {
        let mut text = "Parcels\nCode\tWeight\n".to_owned();
        for (code, kg) in order {
            text.push_str(&format!("{code}\t{kg} kg\n"));
        }
        text
    };
    let first = [
        ("P-1", "12"),
        ("P-2", "44"),
        ("P-3", "3"),
        ("P-4", "27"),
        ("P-5", "9"),
        ("P-6", "61"),
        ("P-7", "18"),
        ("P-8", "33"),
    ];
    let mut sorted = first;
    sorted.sort_by_key(|(_, kg)| std::cmp::Reverse(kg.parse::<u32>().unwrap_or(0)));
    let (before, after) = (table(&first), table(&sorted));
    let cells: Cells = cells_of(&after);
    assert_eq!(cells.get("Code Weight"), Some(&2));
    let changes = diff_page(&text_lines(&before), &text_lines(&after), &cells);
    let shown = changes[0].to_string();
    assert!(
        shown.starts_with("reordered 8 rows below \"Code Weight\", now starting:\n  P-6 61 kg"),
        "{shown}"
    );
}
