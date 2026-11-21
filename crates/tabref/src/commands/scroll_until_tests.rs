use super::{accumulate, stopped};
use tabref_page::ScrollOutcome;

#[test]
fn only_a_quiet_stationary_end_stops_the_loop() {
    let end = ScrollOutcome {
        at_end: true,
        ..ScrollOutcome::default()
    };
    assert!(stopped(&end));
    for uncertain in [
        ScrollOutcome {
            loading: true,
            ..end.clone()
        },
        ScrollOutcome {
            hidden: true,
            ..end.clone()
        },
        ScrollOutcome {
            unwatched: true,
            ..end.clone()
        },
        ScrollOutcome {
            items: 1,
            ..end.clone()
        },
        ScrollOutcome {
            moved: true,
            ..end.clone()
        },
        ScrollOutcome {
            grew: true,
            ..end.clone()
        },
    ] {
        assert!(!stopped(&uncertain));
    }
    assert!(!stopped(&ScrollOutcome::default()));
}

#[test]
fn totals_keep_batch_edges_and_only_final_doubts() {
    let mut total = ScrollOutcome::default();
    accumulate(
        &mut total,
        ScrollOutcome {
            items: 30,
            first: Some("A".into()),
            last: Some("B".into()),
            loading: true,
            moved: true,
            ..ScrollOutcome::default()
        },
    );
    accumulate(
        &mut total,
        ScrollOutcome {
            items: 30,
            first: Some("C".into()),
            last: Some("D".into()),
            ..ScrollOutcome::default()
        },
    );
    accumulate(
        &mut total,
        ScrollOutcome {
            at_end: true,
            ..ScrollOutcome::default()
        },
    );
    assert_eq!(total.items, 60);
    assert_eq!(total.first.as_deref(), Some("A"));
    assert_eq!(total.last.as_deref(), Some("D"));
    assert!(!total.loading);
    assert!(total.moved);
}
