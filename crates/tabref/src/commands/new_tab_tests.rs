use serde_json::json;
use tabref_core::AppError;

use super::in_new_tab;

#[test]
fn an_error_in_the_new_tab_names_it_and_is_no_longer_retryable() {
    let pending = AppError::timeout("http://a.test/ did not respond before the deadline")
        .with_details(
            json!({ "kind": "cdp_navigation_pending", "url": "http://a.test/", "retryable": true }),
        );

    let error = in_new_tab(pending, 2, "http://a.test/");

    let details = error.details.unwrap();
    assert_eq!(details["kind"], "cdp_navigation_pending");
    assert_eq!(details["new_tab_index"], 2);
    assert!(details.get("retryable").is_none());
    assert_eq!(
        error.suggestion.unwrap(),
        "new tab [2] is now current; run `open http://a.test/` without --new-tab"
    );
}
