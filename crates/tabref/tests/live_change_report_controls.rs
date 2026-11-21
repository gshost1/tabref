//! Report regressions for new control names and native versus ARIA options.
#![cfg(unix)]
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use common::{Harness, harness, live, serve};

fn site() -> (Harness, String) {
    let port = serve(&[
        (
            "/custom",
            include_str!("fixtures/change_report_custom_combo.html"),
        ),
        (
            "/meaningful",
            include_str!("fixtures/change_report_meaningful.html"),
        ),
        (
            "/native",
            include_str!("fixtures/change_report_native_frame.html"),
        ),
        (
            "/native-fields",
            include_str!("fixtures/change_report_native_fields.html"),
        ),
        (
            "/owned",
            include_str!("fixtures/change_report_owned_combo.html"),
        ),
        (
            "/form",
            include_str!("fixtures/change_report_form_frame.html"),
        ),
        (
            "/form-fields",
            include_str!("fixtures/change_report_form_fields.html"),
        ),
        (
            "/form-error",
            include_str!("fixtures/change_report_form_error.html"),
        ),
        (
            "/form-validate",
            "<!doctype html><iframe src=/form-submit style='width:600px;height:300px'></iframe>",
        ),
        (
            "/form-submit",
            "<!doctype html><form action=/form-error><label>Status <select name=status><option>Open</option><option>Pending</option><option>Resolved</option></select></label><label>Priority <select name=priority><option>Normal</option><option>High</option></select></label><button>Save ticket</button></form>",
        ),
        (
            "/load-more",
            include_str!("fixtures/change_report_load_more.html"),
        ),
        (
            "/board",
            include_str!("fixtures/change_report_board_redraw.html"),
        ),
    ]);
    (
        harness("new-control-report"),
        format!("http://127.0.0.1:{port}"),
    )
}

fn report(label: &str, text: &str) {
    println!("=== {label}: {} bytes ===\n{text}=== END ===", text.len());
}

#[test]
fn custom_open_prints_options_once_and_caps_them() {
    if !live() {
        return;
    }
    let (tabref, base) = site();
    tabref.ok(&["open", &format!("{base}/custom")]);
    let text = tabref.ok(&["click", "combobox \"Category\""]).stdout;
    report("CUSTOM OPEN", &text);
    assert!(!text.contains("+ Announcements"), "{text}");
    assert!(text.contains("option \"Announcements\""), "{text}");
    assert_eq!(
        text.lines().filter(|line| line.starts_with("new ")).count(),
        8
    );
    assert!(text.contains("+11 more new: tabref snap"), "{text}");
    assert!(text.contains("+ Sports"), "{text}");
}

#[test]
fn native_options_after_frame_reload_are_not_new_controls_or_json_entries() {
    if !live() {
        return;
    }
    let (tabref, base) = site();
    tabref.ok(&["open", &format!("{base}/native")]);
    let text = tabref.ok(&["click", "Reload editor"]).stdout;
    report("NATIVE RELOAD", &text);
    assert!(!text.contains(" option "), "{text}");
    assert!(text.contains("value=\"Open\""), "{text}");
    assert!(text.contains("button \"Save ticket\""), "{text}");
    let data = tabref.ok(&["--json", "click", "Reload editor"]).json();
    assert!(
        data["data"]["new"]
            .as_array()
            .unwrap()
            .iter()
            .all(|entry| !entry.as_str().unwrap().contains(" option ")),
        "{data}"
    );
}

#[test]
fn aria_owns_options_are_still_reported_once() {
    if !live() {
        return;
    }
    let (tabref, base) = site();
    tabref.ok(&["open", &format!("{base}/owned")]);
    let text = tabref.ok(&["click", "Search categories"]).stdout;
    report("ARIA OWNS OPEN", &text);
    assert!(text.contains("option \"Announcements\""), "{text}");
    assert!(!text.contains("+ Announcements"), "{text}");
    assert!(text.contains("+ Events"), "{text}");
}

#[test]
fn frame_form_reload_without_status_text() {
    if !live() {
        return;
    }
    let (tabref, base) = site();
    tabref.ok(&["open", &format!("{base}/form")]);
    let text = tabref.ok(&["click", "Save ticket"]).stdout;
    report("FRAME FORM RELOAD", &text);
    assert!(!text.contains(" option "), "{text}");
}

#[test]
fn frame_form_validation_response_keeps_error_and_omits_native_options() {
    if !live() {
        return;
    }
    let (tabref, base) = site();
    tabref.ok(&["open", &format!("{base}/form-validate")]);
    let text = tabref.ok(&["click", "Save ticket"]).stdout;
    report("FRAME FORM VALIDATION", &text);
    assert!(
        text.contains("Ticket could not be saved: enter an owner."),
        "{text}"
    );
    assert!(!text.contains(" option "), "{text}");
}

#[test]
fn custom_close_is_one_summary() {
    if !live() {
        return;
    }
    let (tabref, base) = site();
    tabref.ok(&["open", &format!("{base}/custom")]);
    tabref.ok(&["click", "combobox \"Category\""]);
    let text = tabref
        .ok(&["press", "Escape", "combobox \"Category\""])
        .stdout;
    report("CUSTOM CLOSE", &text);
    assert_eq!(
        text,
        "pressed Escape in \"Category\"\n- listbox \"Category\" closed (18 options)\n"
    );
}

#[test]
fn changed_values_errors_and_newly_enabled_controls_still_print() {
    if !live() {
        return;
    }
    let (tabref, base) = site();
    tabref.ok(&["open", &format!("{base}/meaningful")]);
    let text = tabref.ok(&["click", "Reveal controls"]).stdout;
    assert!(text.contains("Balance {10 -> 20}"), "{text}");
    assert!(text.contains("+ Retry"), "{text}");
    assert!(text.contains("button \"Retry\""), "{text}");
    assert!(!text.contains("[disabled]"), "{text}");
}

fn new_ref(text: &str, needle: &str) -> String {
    text.lines()
        .find(|line| line.starts_with("new ") && line.contains(needle))
        .and_then(|line| line.split_whitespace().nth(1))
        .unwrap_or_else(|| panic!("missing new ref for {needle:?} in:\n{text}"))
        .to_owned()
}

#[test]
fn non_final_do_steps_keep_new_control_refs_for_next_step() {
    if !live() {
        return;
    }
    let (tabref, base) = site();
    tabref.ok(&["open", &format!("{base}/load-more")]);

    let older = tabref
        .ok(&["do", "click \"Show older releases\"\nclick \"Version 4.0.0\""])
        .stdout;
    report("DO SHOW OLDER", &older);
    let version_ref = new_ref(&older, "button \"Version 3.1.0\"");
    let clicked = tabref.ok(&["click", &version_ref]).stdout;
    assert!(clicked.contains("Added maxSockets option"), "{clicked}");

    let plan = tabref.ok(&["do", "check \"Team\"\ncheck \"Annual\""]).stdout;
    report("DO CONDITIONAL FIELD", &plan);
    let seats_ref = new_ref(&plan, "\"Number of seats\"");
    let filled = tabref.ok(&["fill", &seats_ref, "12"]).stdout;
    assert!(filled.contains("\"5\" -> \"12\""), "{filled}");

    let rows = tabref
        .ok(&["do", "click \"Load more rows\"\nclick \"Version 3.7.0\""])
        .stdout;
    report("DO LOAD MORE ROWS", &rows);
    for row in ["Row 9", "Row 10", "Row 11", "Row 12"] {
        let needle = format!("button \"Open\" in \"{row}\"");
        assert!(rows.contains(&needle), "missing {needle} in:\n{rows}");
    }
    let row_ref = new_ref(&rows, "button \"Open\" in \"Row 12\"");
    let opened = tabref.ok(&["click", &row_ref]).stdout;
    assert!(opened.contains("Opened Row 12"), "{opened}");
}

#[test]
fn board_redraw_labels_containers_and_prefers_distinct_controls() {
    if !live() {
        return;
    }
    let (tabref, base) = site();
    tabref.ok(&["open", &format!("{base}/board")]);
    tabref.ok(&["click", "\"Card actions\" in \"Fix login bug\""]);
    let text = tabref.ok(&["click", "\"Move to Done\""]).stdout;
    report("BOARD REDRAW", &text);

    let new_lines: Vec<_> = text
        .lines()
        .filter(|line| line.starts_with("new "))
        .collect();
    assert_eq!(new_lines.len(), 8, "{text}");
    assert!(text.contains("+9 more new: tabref snap"), "{text}");
    assert!(
        new_lines
            .iter()
            .any(|line| line.contains("button \"Undo move\"")),
        "distinct Undo button was crowded out:\n{text}"
    );
    for line in new_lines
        .iter()
        .filter(|line| line.contains("button \"Card actions\""))
    {
        assert!(line.contains(" in \""), "missing container label: {line}");
    }
    let moved_ref = new_ref(&text, "button \"Card actions\" in \"Fix login bug\"");
    let reopened = tabref.ok(&["click", &moved_ref]).stdout;
    assert!(reopened.contains("Assign"), "{reopened}");
}
