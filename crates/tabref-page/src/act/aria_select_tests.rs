use serde_json::json;

use super::{OptionPick, pick_shown, took};
use crate::act::aria_select_read::{Listed, listed};

fn state(value: &str, open: bool, focused: bool, selected: usize) -> Listed {
    let option = |label: &str, at: usize| json!({ "label": label, "text": label, "value": "", "disabled": false, "shown": label != "Bergen", "selected": at == selected });
    listed(&json!({
        "kind": "combobox",
        "editable": true,
        "open": open,
        "focused": focused,
        "value": value,
        "describe": "<input role=combobox>",
        "options": [option("Bergen", 0), option("Berlin", 1), option("Bern", 2), option("Bern (BE)", 3)]
    }))
}

#[test]
fn only_shown_options_are_matched_and_mapped_back() {
    let now = state("Ber", true, true, 9);
    assert!(now.editable && now.open);
    let (pick, indices) = pick_shown(&now, "Bern");
    assert_eq!(pick, OptionPick::Chosen(1));
    assert_eq!(indices, vec![1, 2, 3]);
    assert_eq!(pick_shown(&now, "Bergen").0, OptionPick::Missing);
}

#[test]
fn typed_text_alone_never_proves_the_pick() {
    let before = state("Bern", true, true, 9);
    assert!(!took(&state("Bern", true, true, 9), &before, 2, "Bern"));
    assert!(!took(&state("Bern", false, false, 9), &before, 2, "Bern"));
    assert!(took(&state("Bern", false, true, 9), &before, 2, "Bern"));
    assert!(took(&state("Bern", true, true, 2), &before, 2, "Bern"));
}

#[test]
fn a_changed_value_proves_it_only_when_it_reads_as_the_label() {
    let before = state("Oslo", true, true, 9);
    assert!(took(&state("Berlin", true, false, 9), &before, 1, "Berlin"));
    assert!(!took(
        &state("Bern (BE)", false, true, 9),
        &before,
        2,
        "Bern"
    ));
    assert!(!took(&state("Lima", false, true, 9), &before, 2, "Bern"));
    assert!(took(
        &state("City: Berlin", false, false, 9),
        &before,
        1,
        "Berlin"
    ));
}

#[test]
fn a_listbox_took_the_option_when_it_is_selected_under_its_label() {
    let mut before = state("Berlin", true, true, 1);
    before.kind = "listbox".to_owned();
    let mut now = state("Bern", true, true, 2);
    now.kind = "listbox".to_owned();
    assert!(took(&now, &before, 2, "Bern"));
    assert!(!took(&now, &before, 2, "Bern (BE)"));
    assert!(!took(&before, &before, 2, "Bern"));
}
