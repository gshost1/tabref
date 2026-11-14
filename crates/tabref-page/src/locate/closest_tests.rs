use super::{closest, similarity};

#[test]
fn a_typo_scores_above_an_unrelated_name() {
    assert!(similarity("Sav", "Save") > similarity("Sav", "Back"));
    assert!(similarity("Pirce", "Price ($)") > similarity("Pirce", "Quantity"));
    assert_eq!(similarity("", "Save"), 0.0);
}

#[test]
fn the_closest_names_come_best_first_without_repeats() {
    let labels = vec![
        (1, "Back".to_owned()),
        (2, "Save".to_owned()),
        (3, "Save".to_owned()),
        (4, "Saved searches".to_owned()),
        (5, "Zebra".to_owned()),
    ];
    assert_eq!(closest("Sav", &labels, 3), vec![2, 4]);
}
