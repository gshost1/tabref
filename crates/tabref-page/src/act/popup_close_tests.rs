use super::compare_replies;
use crate::TextChange;
use serde_json::{Value, json};

fn reply(text: &str, visible: bool, role: &str, items: &str, count: usize) -> Value {
    json!({"blocks": [[0, text]], "popups": if visible {
        vec![json!({"id": 1, "block": 0, "role": role, "name": "Category", "text": items, "count": count})]
    } else { vec![] }})
}

#[test]
fn closed_popup_replaces_only_its_lines() {
    let before = reply(
        "Old notice\nAnnouncements\nArts\nValue old",
        true,
        "listbox",
        "Announcements\nArts",
        2,
    );
    let after = reply("New validation error\nValue new", false, "listbox", "", 0);
    let changes = compare_replies(&before, &after);
    assert!(
        changes
            .iter()
            .any(|c| c.to_string() == "- listbox \"Category\" closed (2 options)"),
        "{changes:?}"
    );
    assert!(
        !changes
            .iter()
            .any(|c| matches!(c, TextChange::Removed(s) if s == "Arts" || s == "Announcements"))
    );
    assert!(
        changes
            .iter()
            .any(|c| c.to_string().contains("New validation error"))
    );
    assert!(changes.iter().any(|c| c.to_string().contains("new")));
}

#[test]
fn duplicate_text_outside_the_popup_is_not_swallowed() {
    let before = reply(
        "Announcements\nAnnouncements\nArts\nUnrelated",
        true,
        "listbox",
        "Announcements\nArts",
        2,
    );
    let after = reply("", false, "listbox", "", 0);
    let changes = compare_replies(&before, &after);
    assert!(
        changes.contains(&TextChange::Removed("Announcements".into())),
        "{changes:?}"
    );
    assert!(changes.contains(&TextChange::Removed("Unrelated".into())));
    assert!(
        changes
            .iter()
            .any(|c| c.to_string().contains("closed (2 options)")),
        "{changes:?}"
    );
}

#[test]
fn empty_menu_closes_and_open_popup_does_not() {
    let before = reply("Keep", true, "menu", "", 0);
    let after = reply("Keep", false, "menu", "", 0);
    assert_eq!(
        compare_replies(&before, &after)
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>(),
        ["- menu \"Category\" closed (0 items)"]
    );
    assert!(compare_replies(&before, &before).is_empty());
}

#[test]
fn unreadable_block_does_not_claim_a_close() {
    let before = reply("Announcements", true, "listbox", "Announcements", 1);
    let after = json!({"blocks": [[0, null]], "popups": []});
    assert!(compare_replies(&before, &after).is_empty());
    assert!(compare_replies(&before, &Value::Null).is_empty());
}

#[test]
fn inline_item_text_cannot_claim_an_unrelated_whole_line() {
    let mut before = reply("Shared\nPrefix Shared", true, "listbox", "Shared", 1);
    before["popups"][0]["text"] = Value::Null;
    let after = reply("", false, "listbox", "", 0);
    let changes = compare_replies(&before, &after);
    assert!(changes.contains(&TextChange::Removed("Shared".into())));
    assert!(changes.contains(&TextChange::Removed("Prefix Shared".into())));
    assert!(!changes.iter().any(|c| c.to_string().contains("closed")));
}
