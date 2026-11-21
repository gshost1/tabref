use serde_json::json;

use super::{ARM, CHECK, has_closed_shadow_root, script};

#[test]
fn a_closed_shadow_root_in_any_document_is_found() {
    let capture = json!({
        "strings": ["open", "closed", "user-agent"],
        "documents": [
            { "nodes": { "shadowRootType": { "index": [3], "value": [0] } } },
            { "nodes": { "shadowRootType": { "index": [7, 9], "value": [2, 1] } } }
        ]
    });
    assert!(has_closed_shadow_root(&capture));
}

#[test]
fn open_and_user_agent_roots_are_not_closed() {
    let capture = json!({
        "strings": ["open", "closed", "user-agent"],
        "documents": [
            { "nodes": { "shadowRootType": { "index": [3, 4], "value": [0, 2] } } },
            { "nodes": {} }
        ]
    });
    assert!(!has_closed_shadow_root(&capture));
    assert!(!has_closed_shadow_root(&json!({})));
}

#[test]
fn the_token_is_quoted_and_every_placeholder_filled() {
    for template in [ARM, CHECK] {
        let filled = script(template, "s1-'x\"");
        assert!(filled.contains(r#""s1-'x\"""#), "{filled}");
        assert!(!filled.contains("KEY_AND_SCAN"));
        assert!(!filled.contains("TOKEN"));
        assert!(!filled.contains("IDLE_MS"));
        assert!(filled.contains("Symbol.for('tabref.capture-watch')"));
    }
}

#[test]
fn the_watch_is_armed_once_the_page_is_idle_or_the_wait_ran_out() {
    let filled = script(ARM, "s1");
    assert!(filled.starts_with("await new Promise"), "{filled}");
    assert!(filled.contains("requestIdleCallback"), "{filled}");
    assert!(filled.contains("{ timeout: 50 }"), "{filled}");
    assert!(
        filled.contains("if (document.visibilityState === 'hidden') arm();"),
        "{filled}"
    );
}

#[test]
fn large_pages_skip_only_visibility_and_bound_unobserved_state() {
    let filled = script(CHECK, "s1");
    assert!(filled.contains("const visibilityScan = total <= 1000;"));
    assert!(filled.contains("visibilityScan && el.checkVisibility"));
    assert!(!filled.contains("total > 1000"));
    assert!(filled.contains("styles([...(root.styleSheets"));
    assert!(filled.contains("forms > 1000"));
    assert!(filled.contains("state.length > 100_000"));
    assert!(filled.contains("styleSize > 100_000"));
    assert!(filled.contains("realm.matchMedia"));
}
