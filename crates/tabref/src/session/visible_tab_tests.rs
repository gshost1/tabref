use super::{TabState, pick_visible};

fn state(id: &str, visible: bool, focused: bool) -> TabState {
    TabState {
        target_id: id.into(),
        visible,
        focused,
    }
}

#[test]
fn the_focused_tab_comes_first_then_the_first_visible_one() {
    let focused = [
        state("A", true, false),
        state("B", true, true),
        state("C", false, false),
    ];
    assert_eq!(pick_visible(&focused).as_deref(), Some("B"));
    let visible = [state("A", false, false), state("B", true, false)];
    assert_eq!(pick_visible(&visible).as_deref(), Some("B"));
    let hidden = [state("A", false, false), state("B", false, false)];
    assert_eq!(pick_visible(&hidden), None);
    assert_eq!(pick_visible(&[]), None);
}
