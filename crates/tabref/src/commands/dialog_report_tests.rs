use serde_json::json;
use tabref_core::AppError;

use super::{after_report, error_with_dialogs, with_dialogs};
use crate::output::outcome::Outcome;

fn confirm() -> serde_json::Value {
    json!({ "type": "confirm", "message": "Later?", "answer": { "accept": true } })
}

#[test]
fn dialogs_go_after_a_report_and_join_the_ones_it_has() {
    let mut report = Outcome::new(
        "clicked \"Go\"",
        json!({ "dialogs": [ { "type": "alert", "message": "Hi" } ] }),
    );
    after_report(&mut report, &[confirm()]);
    assert_eq!(
        report.text,
        "clicked \"Go\"\ndialog confirm \"Later?\" accepted"
    );
    assert_eq!(report.data["dialogs"].as_array().unwrap().len(), 2);

    let mut plain = Outcome::new("2", json!({ "value": 2 }));
    after_report(&mut plain, &[]);
    assert_eq!(plain.text, "2");
    assert!(plain.data.get("dialogs").is_none());
}

#[test]
fn dialogs_go_ahead_of_an_open_report() {
    let mut report = Outcome::new("page \"Away\"", json!({}));
    with_dialogs(&mut report, &[confirm()]);
    assert_eq!(
        report.text,
        "dialog confirm \"Later?\" accepted\npage \"Away\""
    );
    assert_eq!(report.data["dialogs"][0]["message"], "Later?");
}

#[test]
fn an_error_carries_the_dialogs_that_opened() {
    let error = AppError::invalid_args("Navigation failed")
        .with_details(json!({ "kind": "cdp_navigation_failed" }));
    let error = error_with_dialogs(error, &[confirm()]);
    assert_eq!(
        error.message,
        "Navigation failed (dialog confirm \"Later?\" accepted)"
    );
    let details = error.details.unwrap();
    assert_eq!(details["kind"], "cdp_navigation_failed");
    assert_eq!(details["dialogs"][0]["type"], "confirm");
}
