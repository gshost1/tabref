use std::time::{Duration, Instant};

use tabref_core::{AppError, Deadline};

use super::FrameBudget;
use tabref_cdp::transport_timeout;

#[test]
fn before_the_page_is_read_iframes_may_use_all_but_the_reserve() {
    let budget = FrameBudget::reserved(Deadline::after(3_000).unwrap());

    let remaining = budget.iframes(None).remaining();

    assert!(remaining <= Duration::from_millis(2_400), "{remaining:?}");
    assert!(remaining >= Duration::from_millis(2_300), "{remaining:?}");
}

#[test]
fn once_the_page_is_read_iframes_get_a_short_window() {
    let budget = FrameBudget::reserved(Deadline::after(10_000).unwrap());

    let early = budget.iframes(Some(Instant::now())).remaining();
    assert!(early <= Duration::from_millis(300), "{early:?}");
    assert!(early >= Duration::from_millis(250), "{early:?}");

    std::thread::sleep(Duration::from_millis(320));
    let late = budget.iframes(Some(Instant::now())).remaining();
    assert!(late <= Duration::from_millis(100), "{late:?}");
    assert!(late >= Duration::from_millis(60), "{late:?}");
}

#[test]
fn a_short_deadline_still_keeps_the_minimum_reserve() {
    let budget = FrameBudget::reserved(Deadline::after(300).unwrap());

    assert!(budget.iframes(None).remaining() <= Duration::from_millis(200));
}

#[test]
fn a_deadline_inside_the_reserve_leaves_iframes_no_time() {
    assert!(FrameBudget::reserved(Deadline::after(50).unwrap()).is_spent(None));
}

#[test]
fn only_a_timeout_with_snapshot_time_left_is_a_missed_budget() {
    let live = FrameBudget::reserved(Deadline::after(1_000).unwrap());
    let spent = FrameBudget::reserved(Deadline::after(1).unwrap());
    std::thread::sleep(Duration::from_millis(5));

    assert!(live.missed(&transport_timeout("receive")));
    assert!(!spent.missed(&transport_timeout("receive")));
    assert!(!live.missed(&AppError::internal("getFullAXTree")));
}

#[test]
fn a_whole_deadline_read_never_leaves_an_iframe_out() {
    let whole = FrameBudget::whole(Deadline::after(50).unwrap());

    assert!(!whole.missed(&transport_timeout("receive")));
    assert!(!whole.is_spent(Some(Instant::now())));
}
