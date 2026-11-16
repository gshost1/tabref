//! Live run of a long page's view against headless Chrome: the whole
//! output, its outline of the headings after the cut included, stays
//! within the snap budget, every top-level section stays listed with the
//! line it starts at, and `snap --from` that line shows it.
//! Opt-in: `TABREF_LIVE=1 cargo test -p tabref --test live_long_page`.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use common::{live, serve};

const SNAP_BUDGET: usize = 12_000;

fn long_page() -> String {
    let mut html = String::from(
        "<!doctype html><html><head><title>Long</title></head><body><main><h1>Spec</h1>",
    );
    for part in 1..=12 {
        html.push_str(&format!("<h2>{part}. Part {part}</h2>"));
        for sub in 1..=15 {
            html.push_str(&format!(
                "<h3>{part}.{sub}. Topic {part}.{sub}</h3><p>Text of topic {part}.{sub}: {}</p>",
                "words that fill the section. ".repeat(6)
            ));
        }
    }
    html.push_str("</main></body></html>");
    html
}

#[test]
fn a_long_page_view_stays_in_its_budget_and_lists_every_part() {
    if !live() {
        return;
    }
    let tabref = common::harness("long-page");
    let page = long_page();
    let port = serve(&[("/", page.as_str())]);
    let view = tabref.ok(&["open", &format!("http://127.0.0.1:{port}/")]);
    let chars = view.stdout.chars().count();
    assert!(chars <= SNAP_BUDGET, "{chars} chars:\n{}", view.stdout);
    let part = (1..=12)
        .rev()
        .find_map(|part| {
            let label = format!("## {part}. Part {part} (");
            view.stdout
                .lines()
                .find(|line| line.starts_with(&label))
                .map(|line| (part, line.to_owned()))
        })
        .unwrap_or_else(|| panic!("no part listed:\n{}", view.stdout));
    for earlier in 2..=part.0 {
        assert!(
            view.stdout
                .contains(&format!("## {earlier}. Part {earlier}")),
            "part {earlier} missing:\n{}",
            view.stdout
        );
    }
    assert!(
        view.stdout
            .contains("more headings; tabref find <text> or tabref snap --from")
    );
    let from = part
        .1
        .rsplit("--from ")
        .next()
        .unwrap()
        .trim_end_matches(')');
    let section = tabref.ok(&["snap", "--from", from]);
    assert!(
        section
            .stdout
            .contains(&format!("{0}.1. Topic {0}.1", part.0)),
        "{}",
        section.stdout
    );
}
