//! A short `--rows` window past a budget cut: the command a shortened row
//! and the footer give, run as printed, shows that row whole. Opt-in:
//! `TABREF_LIVE=1 cargo test -p tabref --test live_rows_window`.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use common::{harness, live, serve};

fn long_page() -> String {
    let paragraphs: String = (1..=60)
        .map(|n| format!("<p>Paragraph {n} {}</p>", "word ".repeat(40)))
        .collect();
    let rows: String = (1..=40)
        .map(|row| {
            let notes = "by a long party ".repeat(15);
            format!("<tr><td>{row}</td><td>Peak {row} {notes}</td><td>{notes} end{row}</td></tr>")
        })
        .collect();
    format!(
        "<!doctype html><html><head><title>Peaks</title></head><body><h1>Peaks</h1>{paragraphs}\
         <table><tr><th>Rank</th><th>Mountain</th><th>Notes</th></tr>{rows}</table></body></html>"
    )
}

/// The command in `…(+N chars: COMMAND)` at the end of the line holding
/// `needle`.
fn pointer(view: &str, needle: &str) -> String {
    let line = view
        .lines()
        .find(|line| line.contains(needle))
        .unwrap_or_else(|| panic!("no {needle} in:\n{view}"));
    let (_, after) = line.rsplit_once(" chars: ").unwrap();
    after.trim_end_matches(')').to_owned()
}

/// The `tabref snap --full ...` command the footer's `more:` list gives.
fn footer_full(view: &str) -> String {
    let footer = view.lines().last().unwrap();
    let (_, more) = footer.split_once("more: ").unwrap();
    more.split(" | ")
        .find(|command| command.contains("--full"))
        .unwrap_or_else(|| panic!("no --full command in {footer}"))
        .to_owned()
}

fn args(command: &str) -> Vec<&str> {
    command.split_whitespace().skip(1).collect()
}

#[test]
fn the_commands_for_a_shortened_window_row_show_that_row_whole() {
    if !live() {
        return;
    }
    let page = long_page();
    let port = serve(&[("/peaks", Box::leak(page.into_boxed_str()))]);
    let tabref = harness("rows-window");
    tabref.ok(&["open", &format!("http://127.0.0.1:{port}/peaks")]);
    let view = tabref.ok(&["snap", "--rows", "22-24"]).stdout;
    assert!(view.contains("cut here to fit"), "{view}");
    for row in 22..=24 {
        assert!(
            view.contains(&format!("#{row} {row} | Peak {row} ")),
            "{view}"
        );
        assert!(!view.contains(&format!("end{row}")), "{view}");
    }
    let whole = tabref.ok(&args(&pointer(&view, "#23 23 |"))).stdout;
    let first = whole.lines().nth(1).unwrap();
    assert!(
        first.trim_start().starts_with("#23 23 | Peak 23 ") && first.ends_with("end23"),
        "{whole}"
    );
    let whole = tabref.ok(&args(&footer_full(&view))).stdout;
    let first = whole.lines().nth(1).unwrap();
    assert!(
        first.trim_start().starts_with("#22 22 | Peak 22 ") && first.ends_with("end22"),
        "{whole}"
    );
}
