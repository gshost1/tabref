use tabref_page::ScrollOutcome;

use super::first_line;

#[test]
fn scroll_loop_flags_parse_and_conflict() {
    use crate::cli::Cli;
    use clap::Parser;
    assert!(Cli::try_parse_from(["tabref", "scroll", "--until", "Entry 810"]).is_ok());
    assert!(Cli::try_parse_from(["tabref", "scroll", "left", "--to-end"]).is_ok());
    assert!(Cli::try_parse_from(["tabref", "scroll", "--until", "X", "--to-end"]).is_err());
}

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
fn the_bottom_says_only_that_nothing_new_loaded() {
    assert_eq!(
        first_line("down", 600.0, &outcome(true, true)),
        "scrolled down 600px; at the bottom: nothing new loaded"
    );
    assert_eq!(
        first_line("down", 600.0, &outcome(false, true)),
        "did not move: at the bottom, nothing new loaded"
    );
    assert_eq!(
        first_line("up", 600.0, &outcome(false, true)),
        "did not move: at the top, nothing new loaded"
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
        r#"scrolled down 600px: 1 new item loaded ("Entry 51"); more may still be loading"#
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
        "scrolled down 600px; more may still be loading"
    );
}

#[test]
fn what_leaves_the_end_unknown_replaces_the_claim() {
    let hidden = ScrollOutcome {
        hidden: true,
        watched: true,
        ..outcome(true, true)
    };
    assert_eq!(
        first_line("down", 600.0, &hidden),
        "scrolled down 600px; more may still be loading (tab not showing)"
    );
    let stuck = ScrollOutcome {
        hidden: true,
        ..outcome(false, true)
    };
    assert_eq!(
        first_line("down", 600.0, &stuck),
        "did not move: at the bottom; more may still be loading (tab not showing)"
    );
    let changing = ScrollOutcome {
        loading: true,
        ..outcome(true, true)
    };
    assert_eq!(
        first_line("down", 600.0, &changing),
        "scrolled down 600px; more may still be loading"
    );
    let failed = ScrollOutcome {
        unwatched: true,
        ..outcome(true, true)
    };
    assert_eq!(
        first_line("down", 600.0, &failed),
        "scrolled down 600px; could not check what loaded"
    );
}

#[test]
fn loaded_lines_keep_no_control_characters() {
    assert_eq!(
        crate::view::node_line::escape_line("Esc\u{1b}[31mred\rback \"q\"", false),
        "Esc\\u001b[31mred\\rback \"q\""
    );
}
