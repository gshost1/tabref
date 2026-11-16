use serde_json::json;

use super::mark_other_tab;

#[test]
fn a_dialog_from_another_tab_than_the_commands_is_marked() {
    let mut elsewhere = json!({ "type": "alert", "tab": "OTHER" });
    mark_other_tab(&mut elsewhere, Some("MINE"));
    assert_eq!(elsewhere["other_tab"], true);

    let mut mine = json!({ "type": "alert", "tab": "MINE" });
    mark_other_tab(&mut mine, Some("MINE"));
    assert!(mine.get("other_tab").is_none());
}

#[test]
fn nothing_is_marked_when_the_commands_tab_or_the_dialogs_is_unknown() {
    let mut unknown_command = json!({ "type": "alert", "tab": "OTHER" });
    mark_other_tab(&mut unknown_command, None);
    assert!(unknown_command.get("other_tab").is_none());

    let mut unknown_dialog = json!({ "type": "alert", "tab": null });
    mark_other_tab(&mut unknown_dialog, Some("MINE"));
    assert!(unknown_dialog.get("other_tab").is_none());
}
