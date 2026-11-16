use serde_json::json;

use super::{OptionPick, listed, pick_shown, took};

fn read() -> serde_json::Value {
    json!({
        "kind": "combobox",
        "editable": true,
        "open": true,
        "value": "Ber",
        "describe": "<input role=combobox>",
        "options": [
            { "label": "Bergen", "text": "Bergen", "value": "", "disabled": false, "shown": false, "selected": false },
            { "label": "Berlin", "text": "Berlin", "value": "", "disabled": false, "shown": true, "selected": false },
            { "label": "Bern", "text": "Bern", "value": "", "disabled": false, "shown": true, "selected": true }
        ]
    })
}

#[test]
fn only_shown_options_are_matched_and_mapped_back() {
    let state = listed(&read());
    assert!(state.editable && state.open);
    let (pick, indices) = pick_shown(&state, "Bern");
    assert_eq!(pick, OptionPick::Chosen(1));
    assert_eq!(indices, vec![1, 2]);
    assert_eq!(pick_shown(&state, "Bergen").0, OptionPick::Missing);
    assert_eq!(
        pick_shown(&state, "ber").0,
        OptionPick::Ambiguous(vec![0, 1])
    );
}

#[test]
fn the_control_took_the_option_when_it_shows_its_label() {
    let mut state = listed(&read());
    state.value = "Bern".to_owned();
    assert!(took(&state, 2, "Bern", "Oslo"));
    state.value = "Colour: Bern".to_owned();
    assert!(took(&state, 2, "Bern", "Oslo"));
    state.value = "Item 187".to_owned();
    assert!(!took(&state, 2, "Item 18", "Item 187"));
    state.value = "Oslo".to_owned();
    assert!(!took(&state, 2, "Bern", "Oslo"));
}

#[test]
fn a_listbox_took_the_option_when_it_is_selected() {
    let mut state = listed(&read());
    state.kind = "listbox".to_owned();
    assert!(took(&state, 2, "Bern", ""));
    assert!(!took(&state, 1, "Berlin", ""));
}
