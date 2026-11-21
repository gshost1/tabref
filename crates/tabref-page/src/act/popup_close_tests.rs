use super::compare_replies;
use crate::TextChange;
use serde_json::{Value, json};

fn reply(text: &str, visible: bool, role: &str, items: &str, count: usize) -> Value {
    json!({"blocks": [[0, text]], "popups": if visible {
        vec![json!({"id": 1, "watch":"first", "position":"0.0", "block": 0, "role": role, "name": "Category", "text": items, "count": count})]
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
fn empty_menu_and_open_popup_do_not_claim_a_close() {
    let before = reply("Keep", true, "menu", "", 0);
    let after = reply("Keep", false, "menu", "", 0);
    assert!(compare_replies(&before, &after).is_empty());
    assert!(compare_replies(&before, &before).is_empty());
}

#[test]
fn remounted_or_renumbered_popups_do_not_close() {
    let before = reply(
        "Alpha\nBeta\nGamma",
        true,
        "listbox",
        "Alpha\nBeta\nGamma",
        3,
    );
    let mut after = before.clone();
    after["popups"][0]["id"] = json!(12);
    assert!(compare_replies(&before, &after).is_empty());
    after["blocks"][0][1] = json!("Delta");
    after["popups"][0]["text"] = json!("Delta");
    let changes = compare_replies(&before, &after);
    assert!(
        !changes
            .iter()
            .any(|change| change.to_string().contains("closed")),
        "{changes:?}"
    );
}

#[test]
fn surviving_text_outside_the_popup_does_not_prevent_a_close() {
    let before = reply("Beta\nAlpha\nBeta", true, "listbox", "Alpha\nBeta", 2);
    let after = reply("Beta", false, "listbox", "", 0);
    let changes = compare_replies(&before, &after);
    assert_eq!(
        changes,
        [TextChange::Removed(
            "listbox \"Category\" closed (2 options)".into()
        )]
    );
}

#[test]
fn distinct_unnamed_menus_are_not_the_same_popup() {
    let static_menu =
        json!({"id":1,"block":0,"position":"0.0","role":"menu","name":"","text":"Other","count":1});
    let closing_menu = json!({"id":2,"block":0,"position":"0.1","role":"menu","name":"","text":"Rename\nDelete\nArchive","count":3});
    let before = json!({"blocks":[[0,"Other\nRename\nDelete\nArchive"]],"popups":[static_menu,closing_menu]});
    let after = json!({"blocks":[[0,"Other"]],"popups":[static_menu]});
    assert_eq!(
        compare_replies(&before, &after),
        [TextChange::Removed("menu closed (3 items)".into())]
    );
}

#[test]
fn unnamed_remounts_match_position_and_items_not_a_reset_id_collision() {
    let mut before = reply("Alpha\nBeta", true, "menu", "Alpha\nBeta", 2);
    before["popups"][0]["name"] = json!("");
    before["popups"][0]["position"] = json!("0.1");
    let mut after = before.clone();
    after["popups"][0]["id"] = json!(12);
    assert!(compare_replies(&before, &after).is_empty());
    after["popups"][0]["id"] = json!(1);
    after["popups"][0]["watch"] = Value::Null;
    after["popups"][0]["position"] = json!("0.0");
    after["popups"][0]["text"] = json!("Other");
    after["blocks"][0][1] = json!("Other");
    assert!(
        compare_replies(&before, &after)
            .iter()
            .any(|change| change.to_string() == "- menu closed (2 items)")
    );
}

#[test]
fn unreadable_block_does_not_claim_a_close() {
    let before = reply("Announcements", true, "listbox", "Announcements", 1);
    let after = json!({"blocks": [[0, null]], "popups": []});
    assert!(compare_replies(&before, &after).is_empty());
    assert!(compare_replies(&before, &Value::Null).is_empty());
}

fn remounted_after_saved(old_name: &str, new_name: &str) -> Vec<TextChange> {
    let mut before = reply("Red\nBlue", true, "listbox", "Red\nBlue", 2);
    before["popups"][0]["name"] = json!(old_name);
    before["popups"][0]["position"] = json!("0.1.2");
    let mut after = before.clone();
    after["popups"][0]["id"] = json!(12);
    after["popups"][0]["name"] = json!(new_name);
    after["popups"][0]["position"] = json!("0.1.3");
    after["blocks"][0][1] = json!("Saved\nRed\nBlue");
    compare_replies(&before, &after)
}

#[test]
fn unnamed_remounts_keep_their_items_when_position_changes() {
    assert_eq!(
        remounted_after_saved("", ""),
        [TextChange::Added("Saved".into())]
    );
}

#[test]
fn named_remounts_keep_their_items_when_label_changes() {
    assert_eq!(
        remounted_after_saved("Results (5)", "Results (4)"),
        [TextChange::Added("Saved".into())]
    );
}

#[test]
fn a_static_unnamed_menu_shifting_into_a_closed_menus_position_does_not_match() {
    let closing = json!({"id":1,"watch":"first","block":0,"position":"0.1.0","role":"menu","name":"","text":"Rename\nDelete","count":2});
    let static_menu = json!({"id":2,"watch":"first","block":0,"position":"0.1.1","role":"menu","name":"","text":"Other\nHelp","count":2});
    let before =
        json!({"blocks":[[0,"Rename\nDelete\nOther\nHelp"]],"popups":[closing,static_menu]});
    let mut after = json!({"blocks":[[0,"Other\nHelp"]],"popups":[static_menu]});
    after["popups"][0]["position"] = json!("0.1.0");
    assert_eq!(
        compare_replies(&before, &after),
        [TextChange::Removed("menu closed (2 items)".into())]
    );
}

#[test]
fn matching_item_text_in_another_block_does_not_hide_a_close() {
    let before = reply("Red\nBlue", true, "listbox", "Red\nBlue", 2);
    let mut after = reply("", true, "listbox", "Red\nBlue", 2);
    after["popups"][0]["id"] = json!(12);
    after["popups"][0]["block"] = json!(2);
    after["blocks"] = json!([[0, ""], [2, "Red\nBlue"]]);
    assert!(
        compare_replies(&before, &after).contains(&TextChange::Removed(
            "listbox \"Category\" closed (2 options)".into()
        ))
    );
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
