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
    let lines: Vec<&str> = view.stdout.lines().collect();
    let marker = lines
        .iter()
        .position(|line| line.starts_with("… lines ") && line.contains("cut here to fit"))
        .unwrap_or_else(|| panic!("no cut:\n{}", view.stdout));
    let mut listed = Vec::new();
    for part in 1..=12 {
        let label = format!("## {part}. Part {part}");
        let in_body = lines[..marker].iter().any(|line| line.trim() == label);
        let outline = lines[marker..]
            .iter()
            .find(|line| line.starts_with(&format!("{label} (")));
        assert!(
            in_body || outline.is_some(),
            "part {part} neither shown nor listed:\n{}",
            view.stdout
        );
        if let Some(line) = outline {
            listed.push((part, (*line).to_owned()));
        }
    }
    assert!(listed.len() >= 2, "{}", view.stdout);
    let part = listed.last().cloned().unwrap();
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

fn form_page(rows: usize) -> String {
    let mut html = String::from(
        "<!doctype html><html><head><title>Order</title></head><body><main><h1>Order</h1>",
    );
    for row in 1..=rows {
        html.push_str(&format!(
            "<p>Line {row} of the order terms, which the buyer reads before paying.</p>"
        ));
    }
    html.push_str(
        "<form><label>Card number <input name=card></label>\
         <label><input type=checkbox name=agree> I agree</label>\
         <button type=submit>Pay now</button></form></main></body></html>",
    );
    html
}

#[test]
fn a_form_just_past_the_budget_shows_its_controls_or_points_at_them() {
    if !live() {
        return;
    }
    let tabref = common::harness("form-page");
    for rows in [150, 160, 175, 190] {
        let page = form_page(rows);
        let port = serve(&[("/", page.as_str())]);
        let view = tabref.ok(&["open", &format!("http://127.0.0.1:{port}/")]);
        let shown = view.stdout.contains("button Pay now");
        let pointed = view.stdout.contains("form controls follow, from line");
        assert!(shown || pointed, "{rows} rows:\n{}", view.stdout);
        let chars = view.stdout.chars().count();
        assert!(
            chars <= SNAP_BUDGET + 1_200 + 400,
            "{rows} rows: {chars} chars"
        );
        if pointed {
            let from = view
                .stdout
                .split("form controls follow, from line ")
                .nth(1)
                .and_then(|rest| rest.split(':').next())
                .unwrap()
                .to_owned();
            let rest = tabref.ok(&["snap", "--from", &from]);
            assert!(rest.stdout.contains("button Pay now"), "{}", rest.stdout);
        }
    }
}
