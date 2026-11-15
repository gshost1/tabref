use serde_json::json;
use tabref_core::{AppError, ErrorCode};

use super::{add_warning, warned};
use crate::output::emit::error_text;
use crate::output::outcome::Outcome;

#[test]
fn an_error_on_an_unverified_connection_carries_the_warning() {
    let failed: Result<Outcome, AppError> = Err(AppError::new(ErrorCode::StaleRef, "gone")
        .with_details(json!({ "kind": "cdp_node_drift" })));

    let error = warned(failed, Some("port 9333 is held by \"socat\" (pid 7)")).unwrap_err();

    let details = error.details.clone().unwrap();
    assert_eq!(details["kind"], "cdp_node_drift");
    assert_eq!(
        details["warnings"],
        json!(["port 9333 is held by \"socat\" (pid 7)"])
    );
    assert!(
        error_text(&error).starts_with("warning: port 9333"),
        "{}",
        error_text(&error)
    );
}

#[test]
fn every_command_on_an_unverified_connection_is_warned() {
    for _ in 0..2 {
        let outcome = warned(Ok(Outcome::new("tabs", json!({}))), Some("unverified")).unwrap();
        assert_eq!(outcome.data["warnings"], json!(["unverified"]));
    }
    let plain = warned(Ok(Outcome::new("tabs", json!({}))), None).unwrap();
    assert!(plain.data.get("warnings").is_none());
}

#[test]
fn the_warning_leads_the_text_and_joins_the_json_data() {
    let mut outcome = Outcome::new("page Example", json!({ "url": "https://a.test/" }));
    add_warning(&mut outcome, "port 9333 is held by socat (pid 7)");

    assert_eq!(
        outcome.text,
        "warning: port 9333 is held by socat (pid 7)\npage Example"
    );
    assert_eq!(
        outcome.data["warnings"],
        json!(["port 9333 is held by socat (pid 7)"])
    );
    assert_eq!(outcome.data["url"], "https://a.test/");
}
