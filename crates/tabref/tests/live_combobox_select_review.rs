//! Live coverage for option read-back, popup lifetime and post-pick page changes.

#![cfg(unix)]
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use common::{Harness, harness, live, serve};

const PAGE: &str = include_str!("fixtures/combobox_select_review.html");

fn open(name: &str) -> Harness {
    let port = serve(&[("/p", PAGE)]);
    let tabref = harness(name);
    tabref.ok(&["open", &format!("http://127.0.0.1:{port}/p")]);
    tabref
}

#[test]
fn a_click_without_a_pick_fails_and_closes_the_popup() {
    if !live() {
        return;
    }
    let tabref = open("combo-no-commit");
    let result = tabref.run(&["--json", "select", "Dead", "South"]);
    let reply = result.json();
    assert!(!result.ok, "{reply}");
    let error = &reply["error"];
    assert_eq!(error["code"], "ACTION_FAILED", "{reply}");
    let message = error["message"].as_str().unwrap();
    assert!(message.contains("did not keep it"), "{reply}");
    assert_eq!(error["disposition"]["delivery"], "delivered_unverified");
    assert_eq!(tabref.ok(&["js", "deadlist.hidden"]).stdout.trim(), "true");
}

#[test]
fn a_pick_reports_dependent_controls_and_suppresses_only_its_popup() {
    if !live() {
        return;
    }
    let tabref = open("combo-dependent");
    let result = tabref.ok(&["select", "Country", "Canada"]);
    let lines = &result.stdout;
    assert!(lines.contains("Confirm province"), "{lines}");
    assert!(lines.contains("Province"), "{lines}");
}

#[test]
fn a_pick_only_hides_its_own_popup_from_the_changes() {
    if !live() {
        return;
    }
    let tabref = open("combo-popup-report");
    tabref.ok(&["click", "Country"]);
    let result = tabref.ok(&["select", "Country", "Canada"]);
    let lines = &result.stdout;
    assert!(lines.contains("listbox \"Unrelated\" closed"), "{lines}");
    assert!(!lines.contains("listbox \"Countries\" closed"), "{lines}");
    assert!(!lines.contains("- Spain"), "{lines}");
    assert!(lines.contains("+ Spain"), "{lines}");
    let confirm = result.ref_for("button Confirm province");
    assert!(lines.contains("Province"), "{lines}");
    tabref.ok(&["click", &confirm]);
}

#[test]
fn it_picks_an_option_when_the_toggle_is_already_open() {
    if !live() {
        return;
    }
    let tabref = open("combo-open");
    tabref.ok(&["click", "Toggle"]);
    let picked = tabref.ok(&["select", "Toggle", "Beta"]);
    assert_eq!(
        picked.stdout.trim_end(),
        r#"selected "Toggle" combobox "Alpha" -> "Beta""#
    );
}

#[test]
fn a_late_exact_option_beats_an_early_prefix() {
    if !live() {
        return;
    }
    let tabref = open("combo-render");
    let picked = tabref.ok(&["select", "Category", "product"]);
    assert!(
        picked.stdout.contains(r#"-> "Product""#),
        "{}",
        picked.stdout
    );
}

#[test]
fn it_scrolls_a_virtualized_popup_before_accepting_a_prefix() {
    if !live() {
        return;
    }
    let tabref = open("combo-virtual");
    let picked = tabref.ok(&["select", "Topic", "Engineering"]);
    assert!(
        picked.stdout.contains(r#"-> "Engineering""#),
        "{}",
        picked.stdout
    );
}

#[test]
fn only_a_confirmed_closed_popup_with_the_original_value_is_retry_safe() {
    if !live() {
        return;
    }
    let tabref = open("combo-retry");
    let missing = || tabref.run(&["--json", "select", "Dead", "Atlantis"]).json();
    let clean = missing();
    assert_eq!(clean["error"]["code"], "ELEMENT_NOT_FOUND", "{clean}");
    assert_eq!(clean["error"]["disposition"]["retry"], "safe", "{clean}");
    tabref.ok(&["js", "dead.dataset.stuck = 'true'"]);
    let stuck = missing();
    let disposition = &stuck["error"]["disposition"];
    assert_eq!(disposition["delivery"], "delivery_uncertain", "{stuck}");
    assert_eq!(disposition["retry"], "unsafe", "{stuck}");
}

#[test]
fn ambiguous_options_offer_a_name_the_select_command_can_use() {
    if !live() {
        return;
    }
    let tabref = open("combo-candidates");
    tabref.ok(&[
        "js",
        "slow.onclick = () => { slowlist.hidden = false; slowlist.innerHTML = '<li role=option>Product Updates</li><li role=option>Product Ideas</li>'; }; 1",
    ]);
    let result = tabref.run(&["select", "Category", "Product"]);
    assert!(!result.ok);
    let message = result.stderr;
    assert!(message.contains("AMBIGUOUS_TARGET"), "{message}");
    assert!(
        message.contains("Product Updates") && message.contains("Product Ideas"),
        "{message}"
    );
    assert!(message.contains("full option name"), "{message}");
    assert!(!message.contains("add nth"), "{message}");
}

#[test]
fn a_native_select_that_reverts_still_prints_the_main_warning() {
    if !live() {
        return;
    }
    let tabref = open("native-reverts");
    let picked = tabref.ok(&["select", "Native", "New"]);
    assert_eq!(
        picked.stdout.lines().next().unwrap(),
        r#"selected "Native" "Old" -> "New" (the select did not keep it)"#
    );
}
