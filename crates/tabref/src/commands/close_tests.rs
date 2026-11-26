use tabref_cdp::PageTarget;

use super::report;

fn tab(id: &str, title: &str) -> PageTarget {
    PageTarget {
        target_id: id.to_owned(),
        url: format!("https://example.com/{title}"),
        title: title.to_owned(),
        attached: false,
        browser_context_id: None,
    }
}

#[test]
fn the_default_report_counts_the_tabs_left_without_their_titles() {
    let left = [tab("AAAAAAAA11", "mail"), tab("BBBBBBBB22", "bank")];
    let outcome = report("CCCCCCCC33", &left, "BBBBBBBB22", false);
    assert_eq!(
        outcome.text,
        "closed id=CCCCCCCC; 2 tabs left, current [1] id=BBBBBBBB; 'tabref tabs' lists them"
    );
    assert_eq!(outcome.data["closed"], "CCCCCCCC33");
    assert_eq!(outcome.data["tabs_left"], 2);
    assert_eq!(outcome.data["current"]["index"], 1);
    assert_eq!(outcome.data["current"]["id"], "BBBBBBBB22");
    assert!(outcome.data.get("tabs").is_none());
    let json = outcome.data.to_string();
    assert!(!json.contains("mail") && !json.contains("bank"), "{json}");
}

#[test]
fn one_tab_left_is_singular() {
    let outcome = report(
        "CCCCCCCC33",
        &[tab("AAAAAAAA11", "mail")],
        "AAAAAAAA11",
        false,
    );
    assert!(
        outcome.text.contains("; 1 tab left, current [0]"),
        "{}",
        outcome.text
    );
}

#[test]
fn closing_the_last_tab_says_so() {
    let outcome = report("CCCCCCCC33", &[], "", false);
    assert_eq!(outcome.text, "closed id=CCCCCCCC, the last tab");
    assert_eq!(outcome.data["tabs_left"], 0);
    assert!(outcome.data["current"].is_null());
}

#[test]
fn a_current_tab_missing_from_the_list_is_left_out() {
    let outcome = report("CCCCCCCC33", &[tab("AAAAAAAA11", "mail")], "ZZZZ", false);
    assert_eq!(
        outcome.text,
        "closed id=CCCCCCCC; 1 tab left; 'tabref tabs' lists them"
    );
    assert!(outcome.data["current"].is_null());
}

#[test]
fn list_adds_the_tabs_and_keeps_the_count() {
    let left = [tab("AAAAAAAA11", "mail"), tab("BBBBBBBB22", "bank")];
    let outcome = report("CCCCCCCC33", &left, "AAAAAAAA11", true);
    assert!(
        outcome
            .text
            .starts_with("closed id=CCCCCCCC; 2 tabs left, current [0] id=AAAAAAAA:\n*[0] "),
        "{}",
        outcome.text
    );
    assert!(outcome.text.contains("bank"), "{}", outcome.text);
    assert_eq!(outcome.data["tabs_left"], 2);
    assert_eq!(outcome.data["tabs"].as_array().unwrap().len(), 2);
    let empty = report("CCCCCCCC33", &[], "", true);
    assert_eq!(empty.data["tabs"], serde_json::json!([]));
}
