use serde_json::json;

use super::{Room, room_from, room_note};

#[test]
fn a_page_that_fetches_nothing_gets_no_note_however_far_it_scrolls() {
    assert_eq!(room_from(&json!({ "loads": false })), None);
    assert_eq!(
        room_from(&json!({ "page_left": 9000, "view": 800, "list": "x" })),
        None
    );
}

#[test]
fn a_page_that_fetches_and_scrolls_further_names_the_window() {
    assert_eq!(
        room_from(&json!({ "loads": true, "page_left": 4000, "view": 800 })),
        Some(Room::Page)
    );
    assert_eq!(
        room_from(&json!({ "loads": true, "page_left": 1, "view": 800 })),
        None
    );
}

#[test]
fn a_scroll_box_wins_when_the_window_has_little_left() {
    assert_eq!(
        room_from(&json!({ "loads": true, "page_left": 150, "view": 800, "list": "Message 20" })),
        Some(Room::List(Some("Message 20".to_owned())))
    );
    assert_eq!(
        room_from(&json!({ "loads": true, "page_left": 0, "view": 800, "list": "" })),
        Some(Room::List(None))
    );
}

#[test]
fn a_box_on_a_page_with_more_than_a_screen_left_names_both() {
    assert_eq!(
        room_from(&json!({ "loads": true, "page_left": 3000, "view": 800, "list": "Message 20" })),
        Some(Room::Both(Some("Message 20".to_owned())))
    );
    let both = room_note(&Room::Both(Some("Message 20".to_owned())));
    assert!(
        both.contains("`scroll down` (page) or `scroll down \"Message 20\"` (list)"),
        "{both}"
    );
}

#[test]
fn a_box_names_its_item_only_when_short_and_otherwise_a_placeholder() {
    assert_eq!(
        room_from(&json!({ "loads": true, "list": "x".repeat(41) })),
        Some(Room::List(None))
    );
    let generic = room_note(&Room::List(None));
    assert!(
        generic.contains("`scroll down \"<text in that list>\"`"),
        "{generic}"
    );
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
