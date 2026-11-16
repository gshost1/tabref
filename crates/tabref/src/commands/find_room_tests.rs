use serde_json::json;

use super::{Room, room_from, room_note};

#[test]
fn a_page_that_scrolls_further_wins_over_a_list() {
    assert_eq!(
        room_from(&json!({ "page": true, "list": "x" })),
        Some(Room::Page)
    );
}

#[test]
fn a_list_names_its_last_item_when_it_is_short_and_on_one_line() {
    assert_eq!(
        room_from(&json!({ "list": "More below" })),
        Some(Room::List(Some("More below".to_owned())))
    );
    assert_eq!(
        room_from(&json!({ "list": "x".repeat(41) })),
        Some(Room::List(None))
    );
    assert_eq!(room_from(&json!({ "list": "" })), Some(Room::List(None)));
}

#[test]
fn nothing_that_scrolls_adds_no_note() {
    assert_eq!(room_from(&json!(null)), None);
}

#[test]
fn the_note_names_the_step_to_repeat_in_one_batch() {
    let page = room_note(&Room::Page);
    assert!(page.starts_with("the page scrolls further"), "{page}");
    assert!(page.contains("`scroll down` loads it"), "{page}");
    assert!(page.contains("one tabref do"), "{page}");
    let list = room_note(&Room::List(Some("Say \"hi\"".to_owned())));
    assert!(list.contains("`scroll down \"Say \\\"hi\\\"\"`"), "{list}");
}
