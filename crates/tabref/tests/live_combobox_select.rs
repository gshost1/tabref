//! Opt-in live regressions for selecting an ARIA combobox option in one action.

#![cfg(unix)]
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use common::{Harness, Run, harness, live, serve};

const COMBOS: &str = include_str!("fixtures/combobox_select.html");
const DW7: &str = include_str!("fixtures/combobox_select_dw7.html");
const REMOVED: &str = r#"<!doctype html><title>Moving on</title>
<label>Category <select onchange="location.href = '/done'">
<option>Old</option><option>New</option></select></label>"#;

fn site(name: &str, page: &str) -> (Harness, Run) {
    let port = serve(&[
        ("/edit", page),
        ("/done", "<title>Done</title><p>Saved</p>"),
    ]);
    let tabref = harness(name);
    let opened = tabref.ok(&["open", &format!("http://127.0.0.1:{port}/edit")]);
    (tabref, opened)
}

#[test]
fn aria_controls_selects_by_accessible_name_and_prints_one_line() {
    if !live() {
        return;
    }
    let (tabref, _) = site("combo-controls", COMBOS);
    let picked = tabref.ok(&["select", "Service area", "south west"]);
    assert_eq!(
        picked.stdout.trim_end(),
        r#"selected "Service area" combobox "North" -> "SW""#
    );
}

#[test]
fn aria_owns_works_by_ref_in_a_do_step() {
    if !live() {
        return;
    }
    let (tabref, opened) = site("combo-owns", COMBOS);
    let target = opened.ref_for("combobox Team");
    let picked = tabref.ok(&[
        "do",
        &format!("select {target} Platform\nexpect {target} is visible"),
    ]);
    let line = picked.stdout.lines().next().unwrap();
    assert_eq!(
        line,
        format!(r#"ok select {target} combobox "Support" -> "Platform""#)
    );
    assert_eq!(picked.stdout.lines().count(), 2, "{}", picked.stdout);
}

#[test]
fn autocomplete_types_when_options_need_a_query() {
    if !live() {
        return;
    }
    let (tabref, _) = site("combo-autocomplete", COMBOS);
    let picked = tabref.ok(&["select", "City", "Kyoto"]);
    assert_eq!(
        picked.stdout.trim_end(),
        r#"selected "City" combobox "Lisbon" -> "Kyoto""#
    );
    assert_eq!(tabref.ok(&["js", "cities.hidden"]).stdout.trim(), "true");
}

#[test]
fn missing_option_lists_names_and_closes_the_popup() {
    if !live() {
        return;
    }
    let (tabref, _) = site("combo-missing", COMBOS);
    let missing = tabref.run(&["select", "Service area", "West Coast"]);
    let text = format!("{}\n{}", missing.stdout, missing.stderr);
    assert!(!missing.ok, "{text}");
    assert!(text.contains("ELEMENT_NOT_FOUND"), "{text}");
    for name in ["North", "South West", "South East"] {
        assert!(text.contains(name), "{text}");
    }
    assert_eq!(tabref.ok(&["js", "areas.hidden"]).stdout.trim(), "true");
}

#[test]
fn ambiguous_option_uses_target_candidates_and_closes_the_popup() {
    if !live() {
        return;
    }
    let (tabref, _) = site("combo-ambiguous", COMBOS);
    let ambiguous = tabref.run(&["select", "Service area", "South"]);
    let text = format!("{}\n{}", ambiguous.stdout, ambiguous.stderr);
    assert!(!ambiguous.ok, "{text}");
    for expected in ["AMBIGUOUS_TARGET", "South West", "South East"] {
        assert!(text.contains(expected), "{text}");
    }
    assert_eq!(tabref.ok(&["js", "areas.hidden"]).stdout.trim(), "true");
}

#[test]
fn newly_visible_popup_excludes_a_list_that_was_already_visible() {
    if !live() {
        return;
    }
    let (tabref, _) = site("combo-fallback", COMBOS);
    let picked = tabref.ok(&["select", "Priority", "High"]);
    assert_eq!(
        picked.stdout.trim_end(),
        r#"selected "Priority" combobox "Normal" -> "High""#
    );
}

#[test]
fn no_popup_reports_the_missing_option_without_sending_escape() {
    if !live() {
        return;
    }
    let (tabref, _) = site("combo-no-popup", COMBOS);
    let missing = tabref.run(&["select", "Unavailable", "Medium"]);
    let text = format!("{}\n{}", missing.stdout, missing.stderr);
    assert!(!missing.ok, "{text}");
    assert!(text.contains("ELEMENT_NOT_FOUND"), "{text}");
    assert_eq!(
        tabref
            .ok(&["js", "broken.dataset.escaped || 'no'"])
            .stdout
            .trim(),
        "no"
    );
}

#[test]
fn dw7_category_exact_match_wins_and_picks_the_hidden_form_value() {
    if !live() {
        return;
    }
    let (tabref, _) = site("combo-dw7", DW7);
    let picked = tabref.ok(&["select", "Category", "Engineering"]);
    assert_eq!(
        picked.stdout.trim_end(),
        r#"selected "Category" combobox "Product" -> "Engineering""#
    );
    assert_eq!(
        tabref.ok(&["js", "catval.value"]).stdout.trim(),
        "Engineering"
    );
}

#[test]
fn a_native_pick_stays_delivered_when_navigation_replaces_the_select() {
    if !live() {
        return;
    }
    let (tabref, _) = site("select-navigates", REMOVED);
    let picked = tabref.ok(&["--json", "select", "Category", "New"]);
    let reply: serde_json::Value = serde_json::from_str(&picked.stdout).unwrap();
    assert_eq!(reply["data"]["label"], "New", "{reply}");
    let error = reply["data"]["report_error"].as_str().unwrap_or_default();
    assert!(error.contains("could not read the select"), "{reply}");
    assert!(error.ends_with("run tabref snap to see it"), "{reply}");
    assert!(tabref.ok(&["snap"]).stdout.contains("Saved"));
}
