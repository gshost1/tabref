use super::ActionNeed;

#[test]
fn only_a_press_of_the_pointer_needs_a_box_and_only_a_check_may_borrow_a_label() {
    for need in [
        ActionNeed::Fill,
        ActionNeed::Select,
        ActionNeed::Press,
        ActionNeed::Shown,
    ] {
        assert_eq!(need.spec()["box"], false, "{need:?}");
    }
    for need in [ActionNeed::Click, ActionNeed::Check, ActionNeed::Hover] {
        assert_eq!(need.spec()["box"], true, "{need:?}");
    }
    assert_eq!(ActionNeed::Check.spec()["label"], true);
    assert_eq!(ActionNeed::Click.spec()["label"], false);
    assert_eq!(ActionNeed::Press.spec()["enabled"], true);
    assert_eq!(ActionNeed::Shown.spec()["enabled"], false);
}
