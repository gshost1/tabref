use std::sync::Arc;
use std::sync::atomic::AtomicBool;
use std::time::Duration;

use super::UserBrowserTurns;

#[test]
fn a_second_command_waits_for_the_first_to_finish() {
    let turns = Arc::new(UserBrowserTurns::default());
    let cancel = AtomicBool::new(false);
    let (first, waited) = turns.take(Duration::from_secs(1), &cancel).unwrap();
    assert!(waited < Duration::from_millis(50));
    let other = Arc::clone(&turns);
    let second = std::thread::spawn(move || {
        let cancel = AtomicBool::new(false);
        other
            .take(Duration::from_secs(5), &cancel)
            .map(|(_, waited)| waited)
            .unwrap()
    });
    std::thread::sleep(Duration::from_millis(200));
    drop(first);
    let waited = second.join().unwrap();
    assert!(waited >= Duration::from_millis(150), "{waited:?}");
}

#[test]
fn a_command_that_cannot_get_a_turn_in_time_is_told_why() {
    let turns = UserBrowserTurns::default();
    let cancel = AtomicBool::new(false);
    let _first = turns.take(Duration::from_secs(1), &cancel).unwrap();
    let Err(error) = turns.take(Duration::from_millis(120), &cancel) else {
        panic!("got a second turn while the first was held");
    };
    assert_eq!(error.kind(), Some("user_chrome_busy"));
    assert!(error.suggestion.unwrap().contains("one at a time"));
}

#[test]
fn a_cancelled_wait_ends_at_once() {
    let turns = UserBrowserTurns::default();
    let _first = turns
        .take(Duration::from_secs(1), &AtomicBool::new(false))
        .unwrap();
    let Err(error) = turns.take(Duration::from_secs(5), &AtomicBool::new(true)) else {
        panic!("got a turn while the first was held");
    };
    assert_eq!(error.kind(), Some("cancelled"));
}
