//! Pins the combobox verification, stability, and Escape rules.
#![cfg(unix)]
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use common::{Harness, harness, live, serve};
use serde_json::Value;

const PAGE: &str = include_str!("fixtures/combobox_select_round4.html");

fn open(name: &str) -> Harness {
    let port = serve(&[("/p", PAGE)]);
    let tabref = harness(name);
    tabref.ok(&["open", &format!("http://127.0.0.1:{port}/p")]);
    tabref
}

fn js(tabref: &Harness, expression: &str) -> String {
    tabref.ok(&["js", expression]).stdout.trim().to_owned()
}

fn not_kept(reply: &Value) -> &str {
    let error = &reply["error"];
    assert_eq!(error["code"], "ACTION_FAILED", "{reply}");
    assert_eq!(
        error["details"]["kind"], "combobox_option_not_kept",
        "{reply}"
    );
    assert_eq!(
        error["disposition"]["delivery"], "delivered_unverified",
        "{reply}"
    );
    let message = error["message"].as_str().unwrap();
    assert!(message.contains("did not keep it"), "{reply}");
    message
}

#[test]
fn containment_is_not_a_match_in_either_direction_of_the_action() {
    if !live() {
        return;
    }
    let tabref = open("combo-containment");
    for (choice, actual) in [("New York", "New York City"), ("Al", "Alice")] {
        js(&tabref, "city.textContent = 'Nowhere'; 1");
        let picked = tabref.ok(&["--json", "select", "City", choice]).json();
        assert_eq!(picked["data"]["value"], actual, "{picked}");
        assert_eq!(picked["data"]["verified"], false, "{picked}");
        assert_eq!(
            picked["data"]["note"],
            format!("\"City\" now reads \"{actual}\""),
            "{picked}"
        );
    }
    js(
        &tabref,
        "city.dataset.ignore = 'yes'; city.textContent = 'New York City'; 1",
    );
    not_kept(&tabref.run(&["--json", "select", "City", "New York"]).json());
}

#[test]
fn single_select_hover_is_not_evidence_of_a_pick() {
    if !live() {
        return;
    }
    let tabref = open("combo-hover");
    let reply = tabref.run(&["--json", "select", "Hover", "Two"]).json();
    not_kept(&reply);
    assert_eq!(js(&tabref, "hoverBox.textContent"), "None");
    assert_eq!(
        js(
            &tabref,
            "hoverMenu.lastElementChild.getAttribute('aria-selected')"
        ),
        "true"
    );
}

#[test]
fn a_multiselect_option_transitions_to_selected_then_stays_selected_on_repick() {
    if !live() {
        return;
    }
    let tabref = open("combo-multi-repick");
    js(
        &tabref,
        "hoverMenu.setAttribute('aria-multiselectable', 'true'); 1",
    );
    for _ in 0..2 {
        let reply = tabref.ok(&["--json", "select", "Hover", "Two"]).json();
        assert_eq!(reply["data"]["verified"], true, "{reply}");
        assert_eq!(
            js(
                &tabref,
                "hoverMenu.lastElementChild.getAttribute('aria-selected')"
            ),
            "true"
        );
        assert_eq!(js(&tabref, "hoverMenu.hidden"), "true");
    }
}

#[test]
fn multiselect_hover_is_not_a_selection_transition() {
    if !live() {
        return;
    }
    let tabref = open("combo-multi-hover");
    js(
        &tabref,
        "hoverMenu.setAttribute('aria-multiselectable', 'true'); hoverBox.dataset.hoverOnly = 'yes'; 1",
    );
    let reply = tabref.run(&["--json", "select", "Hover", "Two"]).json();
    not_kept(&reply);
    assert_eq!(js(&tabref, "hoverBox.textContent"), "None");
}

#[test]
fn an_option_intercepted_before_press_is_retry_safe_after_cleanup() {
    if !live() {
        return;
    }
    let tabref = open("combo-covered-option");
    js(&tabref, "stuck.dataset.cover = 'yes'; 1");
    let reply = tabref.run(&["--json", "select", "Stuck", "South"]).json();
    let error = &reply["error"];
    assert_eq!(error["code"], "ACTION_FAILED", "{reply}");
    assert_eq!(error["details"]["kind"], "cdp_click_intercepted", "{reply}");
    assert_eq!(error["disposition"]["delivery"], "not_delivered", "{reply}");
    assert_eq!(
        js(
            &tabref,
            "stuck.textContent + ':' + stuckMenu.hidden + ':' + stuckEscapes"
        ),
        "North:true:1"
    );
}

#[test]
fn failed_autocomplete_dismisses_then_restores_without_another_input_event() {
    if !live() {
        return;
    }
    let tabref = open("combo-restore-after-escape");
    js(&tabref, "auto.dataset.clearOpen = 'yes'; 1");
    let reply = tabref.run(&["--json", "select", "Auto", "Kiwi"]).json();
    assert_eq!(reply["error"]["code"], "ELEMENT_NOT_FOUND", "{reply}");
    assert_eq!(
        reply["error"]["disposition"]["delivery"], "not_delivered",
        "{reply}"
    );
    assert_eq!(
        js(
            &tabref,
            "auto.value + ':' + autoMenu.hidden + ':' + autoInputs + ':' + autoEscapes"
        ),
        "Banana:true:1:1"
    );
}

#[test]
fn closing_the_popup_on_mouse_down_does_not_confirm_the_typed_value() {
    if !live() {
        return;
    }
    let tabref = open("combo-typed-unconfirmed");
    js(&tabref, "auto.dataset.mouseDownClose = 'yes'; 1");
    let reply = tabref.ok(&["--json", "select", "Auto", "Cherry"]).json();
    assert_eq!(reply["data"]["value"], "Cherry", "{reply}");
    assert_eq!(reply["data"]["verified"], false, "{reply}");
    assert_eq!(
        reply["data"]["note"], "could not confirm the pick; the box shows the typed text",
        "{reply}"
    );
}

#[test]
fn growing_non_scrollable_options_are_incomplete_until_stable() {
    if !live() {
        return;
    }
    let tabref = open("combo-growing");
    for timeout in ["600", "1200", "5000"] {
        let result = tabref.run(&["--json", "--timeout", timeout, "select", "Grow", "Virgin"]);
        let reply = result.json();
        assert!(!result.ok, "{reply}");
        let error = &reply["error"];
        if timeout == "5000" {
            assert_eq!(error["code"], "AMBIGUOUS_TARGET", "{reply}");
            let message = error["message"].as_str().unwrap();
            assert!(
                message.contains("Virgin Alpha") && message.contains("Virgin Beta"),
                "{reply}"
            );
        } else {
            assert_eq!(
                error["details"]["kind"], "combobox_list_incomplete",
                "{reply}"
            );
            assert!(
                error["details"]["options_seen"].as_u64().unwrap() > 0,
                "{reply}"
            );
            assert!(error["details"].get("options_total").is_none(), "{reply}");
        }
        assert_eq!(error["disposition"]["delivery"], "not_delivered", "{reply}");
        assert_eq!(js(&tabref, "grow.textContent"), "None");
        assert_eq!(js(&tabref, "growing.hidden"), "true");
    }
}

#[test]
fn a_popup_that_never_opens_does_not_receive_escape() {
    if !live() {
        return;
    }
    let tabref = open("combo-no-escape");
    let reply = tabref.run(&["--json", "select", "Closed", "Cherry"]).json();
    let error = &reply["error"];
    assert_eq!(error["code"], "ELEMENT_NOT_FOUND", "{reply}");
    assert_eq!(error["disposition"]["delivery"], "not_delivered", "{reply}");
    assert_eq!(
        js(&tabref, "closedBox.textContent + ':' + closedEscapes"),
        "Banana:0"
    );
}

#[test]
fn a_stuck_popup_adds_the_same_note_to_a_failed_or_kept_pick() {
    if !live() {
        return;
    }
    let tabref = open("combo-stuck-note");
    let reply = tabref.run(&["--json", "select", "Stuck", "South"]).json();
    assert!(
        not_kept(&reply).contains("stayed open after Escape"),
        "{reply}"
    );
    assert_eq!(js(&tabref, "stuckEscapes"), "1");
    js(&tabref, "stuck.dataset.commit = 'yes'; 1");
    let result = tabref.ok(&["select", "Stuck", "South"]);
    assert!(
        result.stdout.contains("stayed open after Escape"),
        "{}",
        result.stdout
    );
    let picked = tabref.ok(&["--json", "select", "Stuck", "South"]).json();
    assert_eq!(picked["data"]["verified"], true, "{picked}");
    assert_eq!(js(&tabref, "stuckEscapes"), "3");
}

#[test]
fn autocomplete_restores_a_failed_query_and_verifies_a_self_closing_pick() {
    if !live() {
        return;
    }
    let tabref = open("combo-restore-query");
    for timeout in ["300", "600", "1200"] {
        let missing = tabref
            .run(&["--json", "--timeout", timeout, "select", "Auto", "Kiwi"])
            .json();
        assert_eq!(missing["error"]["code"], "ELEMENT_NOT_FOUND", "{missing}");
        assert_eq!(
            missing["error"]["disposition"]["delivery"], "not_delivered",
            "{missing}"
        );
        assert_eq!(
            js(&tabref, "auto.value + ':' + autoMenu.hidden"),
            "Banana:true"
        );
    }
    let picked = tabref.ok(&["--json", "select", "Auto", "Cherry"]).json();
    assert_eq!(picked["data"]["verified"], true, "{picked}");
    js(
        &tabref,
        "auto.dataset.dead = 'yes'; auto.value = 'Banana'; 1",
    );
    let reply = tabref.run(&["--json", "select", "Auto", "Cherry"]).json();
    not_kept(&reply);
}
