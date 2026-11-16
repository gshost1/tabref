use super::{OptionEntry, OptionPick, listing, pick_option, printed};

fn option(label: &str, value: &str) -> OptionEntry {
    OptionEntry {
        label: label.into(),
        text: label.into(),
        value: value.into(),
        disabled: false,
    }
}

fn disabled(label: &str, value: &str) -> OptionEntry {
    OptionEntry {
        disabled: true,
        ..option(label, value)
    }
}

fn doctors() -> Vec<OptionEntry> {
    vec![
        option("Choose a doctor", ""),
        option("Dr. Ada Lind", "d01"),
        option("Dr. Ada Moreau", "d02"),
        option("Dr. Bo Quist", "d11"),
        disabled("Dr. Cy Varga", "d12"),
    ]
}

#[test]
fn the_printed_form_adds_the_value_only_when_the_label_lacks_it() {
    assert_eq!(
        printed(&option("Dr. Ada Lind", "d01")),
        "Dr. Ada Lind (d01)"
    );
    assert_eq!(printed(&option("Small (S)", "s")), "Small (S)");
    assert_eq!(printed(&option("Choose", "")), "Choose");
    assert_eq!(printed(&option("Red", "red")), "Red");
}

#[test]
fn the_label_the_printed_form_and_the_value_all_pick_the_option() {
    let options = doctors();
    for choice in ["Dr. Bo Quist", "Dr. Bo Quist (d11)", "d11"] {
        assert_eq!(
            pick_option(&options, choice),
            OptionPick::Chosen(3),
            "{choice}"
        );
    }
}

#[test]
fn case_and_spacing_do_not_matter_after_an_exact_miss() {
    let options = doctors();
    for choice in ["  dr. bo   quist ", "DR. BO QUIST (D11)", "D11"] {
        assert_eq!(
            pick_option(&options, choice),
            OptionPick::Chosen(3),
            "{choice}"
        );
    }
}

#[test]
fn an_exact_label_wins_over_a_longer_one_it_starts() {
    let options = vec![option("Pro", "p"), option("Pro Max", "pm")];
    assert_eq!(pick_option(&options, "Pro"), OptionPick::Chosen(0));
    assert_eq!(pick_option(&options, "pro"), OptionPick::Chosen(0));
}

#[test]
fn a_unique_start_picks_and_a_shared_start_is_ambiguous() {
    let options = doctors();
    assert_eq!(pick_option(&options, "Dr. Bo"), OptionPick::Chosen(3));
    assert_eq!(
        pick_option(&options, "Dr. Ada"),
        OptionPick::Ambiguous(vec![1, 2])
    );
}

#[test]
fn the_view_cut_form_with_an_ellipsis_picks_the_option() {
    let long = "Department of Cardiovascular and Thoracic Surgery, North Wing";
    let options = vec![option(long, "cts"), option("Pediatrics", "ped")];
    assert_eq!(
        pick_option(&options, "Department of Cardiovascular and Thorac… (cts)"),
        OptionPick::Chosen(0)
    );
    assert_eq!(
        pick_option(&options, "Department of Cardiovascular and Thorac…"),
        OptionPick::Chosen(0)
    );
}

#[test]
fn escaped_view_text_is_read_as_the_page_text() {
    let options = vec![option("A | B [beta]", "ab")];
    assert_eq!(
        pick_option(&options, r"A \| B \[beta\] (ab)"),
        OptionPick::Chosen(0)
    );
}

#[test]
fn duplicate_labels_are_told_apart_by_the_printed_value() {
    let options = vec![
        option("Springfield", "il"),
        option("Springfield", "ma"),
        option("Salem", "or"),
    ];
    assert_eq!(
        pick_option(&options, "Springfield"),
        OptionPick::Ambiguous(vec![0, 1])
    );
    assert_eq!(
        pick_option(&options, "Springfield (ma)"),
        OptionPick::Chosen(1)
    );
}

#[test]
fn a_disabled_twin_leaves_the_enabled_one_chosen() {
    let options = vec![disabled("Room 4", "r4old"), option("Room 4", "r4")];
    assert_eq!(pick_option(&options, "Room 4"), OptionPick::Chosen(1));
}

#[test]
fn a_disabled_option_named_exactly_is_reported_as_disabled() {
    let options = doctors();
    assert_eq!(
        pick_option(&options, "Dr. Cy Varga (d12)"),
        OptionPick::Disabled(4)
    );
    assert_eq!(
        pick_option(&options, "dr. cy varga"),
        OptionPick::Disabled(4)
    );
    assert_eq!(
        pick_option(&options, "Dr. Cy Varga (d12) (disabled)"),
        OptionPick::Disabled(4)
    );
}

#[test]
fn a_choice_that_names_nothing_is_missing() {
    let options = doctors();
    assert_eq!(pick_option(&options, "Dr. Zed"), OptionPick::Missing);
    assert_eq!(
        pick_option(&options, "Dr. Bo Quist (d99)"),
        OptionPick::Missing
    );
    assert_eq!(pick_option(&[], "x"), OptionPick::Missing);
}

#[test]
fn a_label_and_another_option_s_value_alike_are_ambiguous() {
    let options = vec![option("2", "1"), option("3", "2")];
    assert_eq!(pick_option(&options, "2"), OptionPick::Ambiguous(vec![0, 1]));
    let printed_alike = vec![option("2", "x"), option("Two", "2")];
    assert_eq!(
        pick_option(&printed_alike, "2"),
        OptionPick::Ambiguous(vec![0, 1])
    );
    assert_eq!(pick_option(&printed_alike, "Two (2)"), OptionPick::Chosen(1));
}

#[test]
fn the_listing_prints_each_option_as_the_view_does_and_caps_the_rest() {
    let options = doctors();
    let all: Vec<usize> = (0..options.len()).collect();
    assert_eq!(
        listing(&options, &all),
        "Choose a doctor | Dr. Ada Lind (d01) | Dr. Ada Moreau (d02) | Dr. Bo Quist (d11) | Dr. Cy Varga (d12) (disabled)"
    );
    let many: Vec<OptionEntry> = (0..100)
        .map(|n| option(&format!("Item {n}"), &format!("i{n}")))
        .collect();
    let every: Vec<usize> = (0..many.len()).collect();
    let text = listing(&many, &every);
    assert!(text.starts_with("Item 0 (i0) | Item 1 (i1)"), "{text}");
    assert!(text.ends_with(" … and 70 more"), "{text}");
    assert!(!text.contains("Item 30 "), "{text}");
}

#[test]
fn a_start_shared_with_a_disabled_option_is_ambiguous() {
    let options = vec![
        disabled("Saturday 9:00", "sat9"),
        option("Saturday 11:00", "sat11"),
    ];
    assert_eq!(
        pick_option(&options, "Sat"),
        OptionPick::Ambiguous(vec![0, 1])
    );
    let long = "Department of Cardiovascular and Thoracic Surgery";
    let cut = vec![
        disabled(&format!("{long}, North"), "n"),
        option(&format!("{long}, South"), "s"),
    ];
    assert_eq!(
        pick_option(&cut, "Department of Cardiovascular and Thorac…"),
        OptionPick::Ambiguous(vec![0, 1])
    );
}

#[test]
fn the_exact_printed_form_picks_a_duplicate_label() {
    let options = vec![option("Paris", "paris"), option("Paris", "paris-tx")];
    assert_eq!(pick_option(&options, "Paris"), OptionPick::Chosen(0));
    assert_eq!(
        pick_option(&options, "Paris (paris-tx)"),
        OptionPick::Chosen(1)
    );
}

#[test]
fn a_label_that_ends_in_disabled_is_its_own_label() {
    let options = vec![option("Seat (disabled)", "s1"), option("Seat", "s2")];
    assert_eq!(
        pick_option(&options, "Seat (disabled)"),
        OptionPick::Chosen(0)
    );
}

#[test]
fn the_listing_escapes_bars_and_control_characters() {
    let options = vec![option("A | B\u{1b}[1m", "ab")];
    assert_eq!(listing(&options, &[0]), "A \\| B\\u001b[1m (ab)");
}
