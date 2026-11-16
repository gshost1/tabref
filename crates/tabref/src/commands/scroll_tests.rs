use tabref_page::ScrollOutcome;

use super::first_line;

fn outcome(moved: bool, at_end: bool) -> ScrollOutcome {
    ScrollOutcome {
        moved,
        at_end,
        ..ScrollOutcome::default()
    }
}

#[test]
fn a_plain_scroll_reads_as_before() {
    assert_eq!(
        first_line("down", 600.0, &outcome(true, false)),
        "scrolled down 600px"
    );
    assert_eq!(
        first_line("down", 600.0, &outcome(false, false)),
        "did not move: nothing left to scroll down"
    );
}

#[test]
fn the_end_of_a_list_says_nothing_more_loaded() {
    assert_eq!(
        first_line("down", 600.0, &outcome(true, true)),
        "scrolled down 600px; end of list: nothing more loaded"
    );
    assert_eq!(
        first_line("down", 600.0, &outcome(false, true)),
        "did not move: end of list, nothing more loaded"
    );
    assert_eq!(
        first_line("up", 600.0, &outcome(false, true)),
        "did not move: top of list, nothing more loaded"
    );
}

#[test]
fn loaded_items_name_the_first_and_the_last() {
    let loaded = ScrollOutcome {
        moved: true,
        watched: true,
        items: 20,
        first: Some("Entry 31".into()),
        last: Some("Entry 50".into()),
        ..ScrollOutcome::default()
    };
    assert_eq!(
        first_line("down", 600.0, &loaded),
        r#"scrolled down 600px: 20 new items loaded ("Entry 31" to "Entry 50")"#
    );
    let one = ScrollOutcome {
        items: 1,
        last: Some("Entry 51".into()),
        first: Some("Entry 51".into()),
        loading: true,
        ..loaded
    };
    assert_eq!(
        first_line("down", 600.0, &one),
        r#"scrolled down 600px: 1 new item loaded ("Entry 51"); more is still loading"#
    );
}

#[test]
fn a_request_still_running_is_said() {
    let pending = ScrollOutcome {
        loading: true,
        ..outcome(true, false)
    };
    assert_eq!(
        first_line("down", 600.0, &pending),
        "scrolled down 600px; more is still loading"
    );
}
