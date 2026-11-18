use std::time::Duration;

use super::{TabState, pick_visible, probe_budget, settled};

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
    assert_eq!(pick_visible(&focused, &[]).as_deref(), Some("B"));
    let visible = [state("A", false, false), state("B", true, false)];
    assert_eq!(pick_visible(&visible, &[]).as_deref(), Some("B"));
    let hidden = [state("A", false, false), state("B", false, false)];
    assert_eq!(pick_visible(&hidden, &[]), None);
    assert_eq!(pick_visible(&[], &[]), None);
}

#[test]
fn the_probe_takes_at_most_half_the_time_left_and_never_more_than_a_second_and_a_half() {
    assert_eq!(
        probe_budget(Duration::from_secs(30)),
        Duration::from_millis(1_500)
    );
    assert_eq!(
        probe_budget(Duration::from_millis(1_000)),
        Duration::from_millis(500)
    );
    assert_eq!(probe_budget(Duration::ZERO), Duration::ZERO);
}

#[test]
fn the_wait_ends_on_a_focused_answer_on_all_answers_or_shortly_after_a_visible_one() {
    let short = Duration::from_millis(20);
    let long = Duration::from_millis(150);
    assert!(settled(&[state("A", true, true)], 1, 3, Some(short)));
    assert!(settled(
        &[state("A", false, false), state("B", false, false)],
        2,
        2,
        None
    ));
    assert!(!settled(&[state("A", true, false)], 1, 3, Some(short)));
    assert!(settled(&[state("A", true, false)], 1, 3, Some(long)));
    assert!(!settled(&[state("A", false, false)], 1, 3, None));
    assert!(!settled(&[], 0, 3, None));
}

#[test]
fn a_tab_tabref_emulates_counts_only_when_no_other_tab_is_showing() {
    let emulated = ["A".to_owned()];
    let both = [state("A", true, true), state("B", true, false)];
    assert_eq!(pick_visible(&both, &emulated).as_deref(), Some("B"));
    let alone = [state("A", true, true), state("B", false, false)];
    assert_eq!(pick_visible(&alone, &emulated).as_deref(), Some("A"));
}
