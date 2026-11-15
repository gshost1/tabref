use serde_json::json;

use super::SessionLedger;

#[test]
fn an_enabled_domain_repeats_locally_until_disabled_or_detached() {
    let mut ledger = SessionLedger::default();
    assert!(!ledger.already_on("Page.enable", &json!({}), Some("S")));
    ledger.record("Page.enable", &json!({}), Some("S"));
    assert!(ledger.already_on("Page.enable", &json!({}), Some("S")));
    assert!(!ledger.already_on("Page.enable", &json!({}), Some("T")));

    ledger.record("Page.disable", &json!({}), Some("S"));
    assert!(!ledger.already_on("Page.enable", &json!({}), Some("S")));

    ledger.record("Page.enable", &json!({}), Some("S"));
    ledger.mark_detached("S");
    assert!(!ledger.already_on("Page.enable", &json!({}), Some("S")));
    assert!(ledger.is_detached("S"));
}

#[test]
fn a_toggle_repeats_locally_only_when_turned_on_with_the_same_lone_flag() {
    let mut ledger = SessionLedger::default();
    let on = json!({ "discover": true });
    ledger.record("Target.setDiscoverTargets", &on, None);
    assert!(ledger.already_on("Target.setDiscoverTargets", &on, None));
    assert!(!ledger.already_on(
        "Target.setDiscoverTargets",
        &json!({ "discover": true, "filter": [] }),
        None
    ));
    ledger.record(
        "Target.setDiscoverTargets",
        &json!({ "discover": false }),
        None,
    );
    assert!(!ledger.already_on("Target.setDiscoverTargets", &on, None));
}

#[test]
fn other_methods_are_never_answered_locally() {
    let mut ledger = SessionLedger::default();
    ledger.record("Network.enable", &json!({}), Some("S"));
    assert!(!ledger.already_on("Network.enable", &json!({}), Some("S")));
    assert!(!SessionLedger::tracks("DOM.getDocument"));
    assert!(SessionLedger::tracks("Page.disable"));
}
