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
