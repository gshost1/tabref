use super::{MAX_TIME, accumulate, cap_reason, end_summary, found_head, stopped};
use std::time::Duration;
use tabref_core::Deadline;
use tabref_page::ScrollOutcome;

#[test]
fn capped_no_match_warns_that_more_may_load() {
    for reason in ["60-step cap", "time cap (8 s or session deadline)"] {
        assert_eq!(
            super::not_found_head("X", 60, reason, true),
            format!(
                "\"X\" is not on the page after 60 scrolls; stopped at the {reason}; more may load"
            )
        );
    }
    assert_eq!(
        super::not_found_head("X", 2, "no more loaded within 3.2s; more may load", false),
        "\"X\" is not on the page after 2 scrolls; no more loaded within 3.2s; more may load"
    );
}

#[test]
fn caps_are_stops_not_destinations() {
    for reason in ["60-step cap", "time cap (8 s or session deadline)"] {
        let line = end_summary("down", 600.0, 60, &ScrollOutcome::default(), (reason, true));
        assert_eq!(
            line,
            format!("scrolled down 60 times, stopped at the {reason}; more may load")
        );
        let loaded = ScrollOutcome {
            items: 30,
            first: Some("A".into()),
            last: Some("B".into()),
            ..ScrollOutcome::default()
        };
        let line = end_summary("down", 600.0, 60, &loaded, (reason, true));
        assert!(line.contains("30 new items loaded"), "{line}");
        assert!(line.ends_with("; more may load"), "{line}");
    }
}

#[test]
fn a_probe_hit_without_find_lines_does_not_promise_refs() {
    assert_eq!(
        found_head("X", 27, false),
        "\"X\" is on the page after 27 scrolls; use `tabref find` to locate it"
    );
    assert_eq!(found_head("X", 27, true), "found \"X\" after 27 scrolls:");
}

#[test]
fn the_eight_second_limit_and_earlier_session_deadline_stop_the_loop() {
    assert_eq!(MAX_TIME, Duration::from_secs(8));
    let deadline = Deadline::after(60_000).unwrap().capped(MAX_TIME);
    assert!(deadline.remaining() <= Duration::from_secs(8));
    assert_eq!(cap_reason(27, deadline.is_expired()), None);
    let expired = deadline.capped(Duration::ZERO);
    assert_eq!(
        cap_reason(27, expired.is_expired()),
        Some("time cap (8 s or session deadline)")
    );
    let earlier = Deadline::after(50).unwrap().capped(MAX_TIME);
    assert!(earlier.remaining() <= Duration::from_millis(50));
    assert_eq!(cap_reason(60, false), Some("60-step cap"));
}

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
