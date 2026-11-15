use tabref_core::{DeliverySemantics, ErrorCode};

use super::Unactionable;
use crate::act::blocker::Blocker;

fn blocked(blocker: Blocker, detail: &str) -> Unactionable {
    Unactionable {
        blocker,
        detail: detail.into(),
        waited_ms: 1500,
    }
}

#[test]
fn a_disabled_button_names_itself_and_the_next_step() {
    let error = blocked(Blocker::Disabled, "disabled").error(r#"button "Save""#);
    assert_eq!(error.code, ErrorCode::ActionFailed);
    assert_eq!(error.kind(), Some("element_disabled"));
    assert_eq!(
        error.message,
        r#"button "Save" is disabled; wait for the page to enable it, or fill what it needs"#
    );
    assert_eq!(error.disposition, DeliverySemantics::not_delivered());
}

#[test]
fn a_disabled_reason_other_than_the_attribute_is_named() {
    let error = blocked(Blocker::Disabled, "inside a disabled fieldset").error(r#"button "Ship""#);
    assert!(
        error
            .message
            .starts_with(r#"button "Ship" is disabled (inside a disabled fieldset);"#),
        "{}",
        error.message
    );
}

#[test]
fn a_hidden_element_says_how_it_is_hidden() {
    let error = blocked(Blocker::Hidden, "display:none").error("e4 button \"Peek\"");
    assert_eq!(error.kind(), Some("element_hidden"));
    assert!(
        error
            .message
            .starts_with("e4 button \"Peek\" is hidden (display:none); open"),
        "{}",
        error.message
    );
}

#[test]
fn a_read_only_field_is_its_own_kind() {
    let error = blocked(Blocker::ReadOnly, "read-only").error(r#"textfield "Code""#);
    assert_eq!(error.kind(), Some("element_readonly"));
    assert!(error.message.contains("is read-only"), "{}", error.message);
}

#[test]
fn an_element_with_no_size_says_what_to_click_instead() {
    let error = blocked(Blocker::NoSize, "zero size").error(r#"link "Floaty""#);
    assert_eq!(error.kind(), Some("element_zero_size"));
    assert_eq!(
        error.message,
        r#"link "Floaty" has no size on screen; click the text or label that draws it"#
    );
}

#[test]
fn an_image_map_area_says_it_cannot_be_pressed() {
    let error = blocked(Blocker::Unpressable, "image-map area").error(r#"link "Area go""#);
    assert_eq!(error.kind(), Some("element_not_pressable"));
    assert!(
        error.message.contains("image-map area"),
        "{}",
        error.message
    );
    assert!(Blocker::Unpressable.is_settled());
}
