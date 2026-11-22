//! Live review regressions for combobox read-back and virtualized options.
#![cfg(unix)]
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use std::time::{Duration, Instant};

use common::{Harness, harness, live, serve};

const PAGE: &str = include_str!("fixtures/combobox_select_round3.html");

fn open(name: &str) -> Harness {
    let port = serve(&[("/p", PAGE)]);
    let tabref = harness(name);
    tabref.ok(&["open", &format!("http://127.0.0.1:{port}/p")]);
    tabref
}

#[test]
fn secondary_option_text_can_be_omitted_from_the_kept_value() {
    if !live() {
        return;
    }
    let tabref = open("combo-secondary");
    let picked = tabref.ok(&["--json", "select", "Person", "Alice"]).json();
    assert_eq!(picked["data"]["value"], "Alice", "{picked}");
    assert_eq!(picked["data"]["verified"], true, "{picked}");
}

#[test]
fn a_repick_keeps_the_existing_primary_label() {
    if !live() {
        return;
    }
    let tabref = open("combo-repick");
    tabref.ok(&["js", "person.textContent = 'Alice'; 1"]);
    let picked = tabref.ok(&["--json", "select", "Person", "Alice"]).json();
    assert_eq!(picked["data"]["previous"], "Alice", "{picked}");
    assert_eq!(picked["data"]["value"], "Alice", "{picked}");
    assert_eq!(picked["data"]["verified"], true, "{picked}");
}

#[test]
fn an_unrelated_new_value_is_reported_as_a_note() {
    if !live() {
        return;
    }
    let tabref = open("combo-unrelated-value");
    tabref.ok(&["js", "person.dataset.mode = 'Assigned'; 1"]);
    let picked = tabref.ok(&["select", "Person", "Bob"]);
    assert!(
        picked
            .stdout
            .contains("note: \"Person\" now reads \"Assigned\""),
        "{}",
        picked.stdout
    );
    tabref.ok(&["js", "person.textContent = 'Nobody'; 1"]);
    let picked = tabref.ok(&["--json", "select", "Person", "Bob"]).json();
    assert_eq!(picked["data"]["verified"], false, "{picked}");
    tabref.ok(&[
        "js",
        "person.textContent = 'Nobody'; person.dataset.mode = 'value-text'; 1",
    ]);
    let picked = tabref.ok(&["select", "Person", "Bob"]);
    assert!(
        picked
            .stdout
            .contains("note: \"Person\" now reads \"Assigned\""),
        "{}",
        picked.stdout
    );
}

#[test]
fn explicit_selection_is_kept_even_when_the_control_text_is_unchanged_in_a_multiselect() {
    if !live() {
        return;
    }
    let tabref = open("combo-aria-selected");
    tabref.ok(&[
        "js",
        "person.dataset.mode = 'selected'; people.setAttribute('aria-multiselectable', 'true'); 1",
    ]);
    let picked = tabref.ok(&["--json", "select", "Person", "Bob"]).json();
    assert_eq!(picked["data"]["verified"], true, "{picked}");
}

#[test]
fn a_new_chip_counts_as_kept_when_the_input_is_empty() {
    if !live() {
        return;
    }
    let tabref = open("combo-new-chip");
    let picked = tabref.ok(&["--json", "select", "Tags", "Alpha"]).json();
    assert_eq!(picked["data"]["verified"], true, "{picked}");
    assert_eq!(picked["data"]["value"], "", "{picked}");
    assert_eq!(
        tabref
            .ok(&["js", "chips.lastElementChild.getAttribute('aria-label')"])
            .stdout
            .trim(),
        "Remove Alpha"
    );
    let again = tabref.ok(&["--json", "select", "Tags", "Alpha"]).json();
    assert_eq!(again["data"]["verified"], true, "{again}");
    assert_eq!(
        tabref
            .ok(&["js", "chips.lastElementChild.getAttribute('aria-label')"])
            .stdout
            .trim(),
        "Remove Alpha"
    );
    assert_eq!(tabref.ok(&["js", "tagPicks"]).stdout.trim(), "1");
}

#[test]
fn an_empty_closed_menu_does_not_receive_escape() {
    if !live() {
        return;
    }
    let tabref = open("combo-downshift");
    let picked = tabref.ok(&["--json", "select", "Fruit", "Banana"]).json();
    assert_eq!(picked["data"]["verified"], true, "{picked}");
    assert_eq!(tabref.ok(&["js", "fruit.value"]).stdout.trim(), "Banana");
    let missing = tabref.run(&["--json", "select", "Fruit", "Kiwi"]).json();
    assert_eq!(
        missing["error"]["disposition"]["delivery"], "not_delivered",
        "{missing}"
    );
    assert_eq!(tabref.ok(&["js", "fruit.value"]).stdout.trim(), "Banana");
    tabref.ok(&["js", "fruit.onclick = null; 1"]);
    let missing = tabref.run(&["--json", "select", "Fruit", "Kiwi"]).json();
    assert_eq!(
        missing["error"]["disposition"]["delivery"], "not_delivered",
        "{missing}"
    );
    assert_eq!(tabref.ok(&["js", "fruit.value"]).stdout.trim(), "Banana");
}

#[test]
fn a_later_virtual_window_can_make_an_early_partial_match_ambiguous() {
    if !live() {
        return;
    }
    let tabref = open("combo-virtual-ambiguity");
    let result = tabref.run(&["--json", "select", "Place", "Virgin Islands"]);
    let reply = result.json();
    assert!(!result.ok, "{reply}");
    assert_eq!(reply["error"]["code"], "AMBIGUOUS_TARGET", "{reply}");
    let message = reply["error"]["message"].as_str().unwrap();
    assert!(
        message.contains("British Virgin Islands") && message.contains("U.S. Virgin Islands"),
        "{reply}"
    );
    assert_eq!(tabref.ok(&["js", "place.value"]).stdout.trim(), "None");
}

#[test]
fn the_final_virtual_window_must_stabilize_before_accepting_a_partial() {
    if !live() {
        return;
    }
    let tabref = open("combo-virtual-end-update");
    tabref.ok(&["js", "place.dataset.delayedEnd = 'yes'; 1"]);
    let result = tabref.run(&["--json", "select", "Place", "British"]);
    let reply = result.json();
    assert!(!result.ok, "{reply}");
    assert_eq!(reply["error"]["code"], "AMBIGUOUS_TARGET", "{reply}");
    let message = reply["error"]["message"].as_str().unwrap();
    assert!(
        message.contains("British Virgin Islands") && message.contains("British Isles"),
        "{reply}"
    );
    assert_eq!(tabref.ok(&["js", "place.value"]).stdout.trim(), "None");
}

#[test]
fn an_exact_match_beyond_the_first_virtual_windows_is_selected() {
    if !live() {
        return;
    }
    let tabref = open("combo-virtual-late");
    tabref.ok(&["js", "place.dataset.indexed = 'yes'; 1"]);
    let picked = tabref
        .ok(&["--json", "select", "Place", "Place 050"])
        .json();
    assert_eq!(picked["data"]["value"], "Place 050", "{picked}");
}

#[test]
fn a_complete_virtual_scan_reports_the_total_and_can_return_to_a_unique_partial() {
    if !live() {
        return;
    }
    let tabref = open("combo-virtual-complete");
    let missing = tabref
        .run(&["--json", "select", "Place", "Atlantis"])
        .json();
    let error = &missing["error"];
    assert_eq!(error["code"], "ELEMENT_NOT_FOUND", "{missing}");
    assert_eq!(error["details"]["options_total"], 100, "{missing}");
    assert_eq!(error["disposition"]["retry"], "safe", "{missing}");
    assert_eq!(tabref.ok(&["js", "places.scrollTop"]).stdout.trim(), "0");
    let picked = tabref.ok(&["--json", "select", "Place", "British"]).json();
    assert_eq!(
        picked["data"]["value"], "British Virgin Islands",
        "{picked}"
    );
}

#[test]
fn an_incomplete_scan_cannot_pick_a_partial_match_or_claim_the_total() {
    if !live() {
        return;
    }
    let tabref = open("combo-virtual-incomplete");
    let result = tabref.run(&[
        "--json",
        "--timeout",
        "300",
        "select",
        "Place",
        "Virgin Islands",
    ]);
    let reply = result.json();
    assert!(!result.ok, "{reply}");
    let error = &reply["error"];
    let message = error["message"].as_str().unwrap();
    assert!(message.contains("list not fully searched (saw"), "{reply}");
    assert!(
        error["details"]["options_seen"].as_u64().unwrap() > 0,
        "{reply}"
    );
    assert!(error["details"].get("options_total").is_none(), "{reply}");
    assert_eq!(error["disposition"]["retry"], "safe", "{reply}");
    assert_eq!(tabref.ok(&["js", "place.value"]).stdout.trim(), "None");
}

#[test]
fn a_stable_non_scrollable_partial_match_finishes_early() {
    if !live() {
        return;
    }
    let tabref = open("combo-static-prefix");
    tabref.ok(&["select", "Prefix", "Spain"]);
    let start = Instant::now();
    tabref.ok(&["select", "Prefix", "New"]);
    assert!(
        start.elapsed() < Duration::from_millis(850),
        "{:?}",
        start.elapsed()
    );
}

#[test]
fn an_editable_combobox_filters_after_an_incomplete_scan() {
    if !live() {
        return;
    }
    let tabref = open("combo-virtual-filter");
    tabref.ok(&[
        "js",
        "place.readOnly = false; place.setAttribute('aria-autocomplete', 'list'); 1",
    ]);
    let picked = tabref
        .ok(&[
            "--json",
            "--timeout",
            "1500",
            "select",
            "Place",
            "Place 099",
        ])
        .json();
    assert_eq!(picked["data"]["value"], "Place 099", "{picked}");
    assert_eq!(picked["data"]["verified"], true, "{picked}");
    assert_eq!(
        tabref.ok(&["js", "place.dataset.filter"]).stdout.trim(),
        "Place 099"
    );
}
