use serde_json::json;
use tabref_cdp::CdpEvent;

use super::is_page_opened_by;

fn created(kind: &str, opener: Option<&str>) -> CdpEvent {
    CdpEvent {
        method: "Target.targetCreated".into(),
        params: json!({ "targetInfo": { "targetId": "NEW", "type": kind, "openerId": opener } }),
        session_id: None,
    }
}

#[test]
fn only_a_page_the_watched_tab_opened_counts() {
    assert!(is_page_opened_by(&created("page", Some("TAB")), "TAB"));
    assert!(!is_page_opened_by(&created("page", Some("OTHER")), "TAB"));
    assert!(!is_page_opened_by(&created("page", None), "TAB"));
    assert!(!is_page_opened_by(&created("iframe", Some("TAB")), "TAB"));
}

#[test]
fn a_failure_after_the_action_was_sent_is_never_safe_to_retry() {
    use tabref_core::{AppError, DeliverySemantics, ErrorCode};

    use super::after_delivery;

    let busy = AppError::new(ErrorCode::Timeout, "Tab T stayed busy")
        .with_details(json!({ "kind": "tab_busy", "tab": "T" }))
        .with_disposition(DeliverySemantics::not_delivered());
    let error = after_delivery(busy);
    assert_eq!(error.disposition, DeliverySemantics::delivered_unverified());
    assert_eq!(
        error.message,
        "the action was delivered, but Tab T stayed busy"
    );
    let details = error.details.unwrap();
    assert_eq!(details["kind"], "tab_busy");
    assert_eq!(
        details["report_error"],
        "could not read what the action set off (Tab T stayed busy); run tabref snap to see it"
    );
    let unsure = AppError::new(ErrorCode::ActionFailed, "gone")
        .with_disposition(DeliverySemantics::uncertain());
    assert_eq!(
        after_delivery(unsure).disposition,
        DeliverySemantics::uncertain()
    );
}
